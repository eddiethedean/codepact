"""Qualify required runtime model probes without accepting skipped evidence."""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import hashlib
import importlib
import importlib.util
import json
from pathlib import Path
import platform
import re
import sys
from types import ModuleType
from typing import cast
import unittest


@dataclass(frozen=True)
class ModelSuite:
    identifier: str
    models: tuple[str, ...]
    python_versions: tuple[str, ...]
    module: str
    upstream_modules: tuple[str, ...]
    tests: tuple[str, ...]


def object_value(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    items = cast(dict[object, object], value)
    if not all(isinstance(key, str) for key in items):
        raise ValueError(f"{label} keys must be strings")
    return cast(dict[str, object], value)


def string_value(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a nonempty string")
    return value


def string_list(value: object, label: str) -> tuple[str, ...]:
    if not isinstance(value, list) or not value:
        raise ValueError(f"{label} must be a nonempty list")
    values = tuple(string_value(item, label) for item in cast(list[object], value))
    if len(set(values)) != len(values):
        raise ValueError(f"{label} contains duplicates")
    return values


def load_manifest(path: Path) -> tuple[ModelSuite, ...]:
    manifest = object_value(cast(object, json.loads(path.read_text())), "manifest")
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported model conformance manifest schema")
    raw_suites = manifest.get("suites")
    if not isinstance(raw_suites, list) or not raw_suites:
        raise ValueError("manifest must contain at least one suite")
    suites: list[ModelSuite] = []
    for raw_suite in cast(list[object], raw_suites):
        suite = object_value(raw_suite, "suite")
        module = string_value(suite.get("module"), "module")
        if not re.fullmatch(r"test_[A-Za-z0-9_]+", module):
            raise ValueError(f"invalid test module: {module}")
        tests = string_list(suite.get("tests"), "tests")
        if any(not test.startswith(f"{module}.") for test in tests):
            raise ValueError(f"test IDs must belong to {module}")
        versions = string_list(suite.get("python_versions"), "python_versions")
        if any(not re.fullmatch(r"3\.\d+\.\d+", version) for version in versions):
            raise ValueError("model runtimes must specify exact Python patch versions")
        suites.append(
            ModelSuite(
                string_value(suite.get("id"), "id"),
                string_list(suite.get("models"), "models"),
                versions,
                module,
                string_list(suite.get("upstream_modules"), "upstream_modules"),
                tests,
            )
        )
    for label, values in (
        ("suite IDs", [suite.identifier for suite in suites]),
        ("modules", [suite.module for suite in suites]),
        ("test IDs", [test for suite in suites for test in suite.tests]),
    ):
        if len(values) != len(set(values)):
            raise ValueError(f"duplicate {label} in model conformance manifest")
    return tuple(suites)


def environment() -> dict[str, str]:
    return {
        "implementation": sys.implementation.name,
        "python_version": platform.python_version(),
        "python_build": sys.version,
        "platform": sys.platform,
        "machine": platform.machine(),
        "executable": sys.executable,
    }


def cases(suite: unittest.TestSuite) -> list[unittest.TestCase]:
    result: list[unittest.TestCase] = []
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            result.extend(cases(item))
        else:
            result.append(item)
    return result


class ExecutionResult(unittest.TestResult):
    def __init__(self) -> None:
        super().__init__()
        self.started: list[str] = []
        self.passed: list[str] = []

    def startTest(self, test: unittest.TestCase) -> None:
        self.started.append(test.id())
        super().startTest(test)

    def addSuccess(self, test: unittest.TestCase) -> None:
        self.passed.append(test.id())
        super().addSuccess(test)


def source_fingerprint(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def qualify(root: Path, manifest_path: Path, python_version: str) -> dict[str, object]:
    actual = environment()
    errors: list[str] = []
    receipt: dict[str, object] = {
        "schema_version": 1,
        "status": "failed",
        "requested_python_version": python_version,
        "environment": actual,
        "qualification_errors": errors,
        "selected_suites": [],
        "required": [],
        "discovered": [],
        "started": [],
        "passed": [],
        "skipped": [],
        "failures": [],
        "errors": [],
        "expected_failures": [],
        "unexpected_successes": [],
        "test_source_sha256": {},
        "upstream_source_sha256": {},
    }
    try:
        suites = load_manifest(manifest_path)
        receipt["manifest_sha256"] = source_fingerprint(manifest_path)
    except (OSError, ValueError) as error:
        errors.append(str(error))
        return receipt
    selected = [suite for suite in suites if python_version in suite.python_versions]
    receipt["selected_suites"] = [
        {"id": suite.identifier, "models": suite.models} for suite in selected
    ]
    required = sorted(test for suite in selected for test in suite.tests)
    receipt["required"] = required
    if not selected:
        errors.append(f"no required model suite for Python {python_version}")
    if (
        actual["implementation"] != "cpython"
        or actual["python_version"] != python_version
    ):
        errors.append(
            f"expected CPython {python_version}; got "
            f"{actual['implementation']} {actual['python_version']}"
        )
    # Do not import or run probes on a runtime that cannot qualify the model.
    if errors:
        return receipt

    test_root = root / "tests/python"
    previous_path = list(sys.path)
    previous_modules: dict[str, ModuleType | None] = {}
    test_sources: dict[str, str] = {}
    upstream_sources: dict[str, str] = {}
    discovered: list[unittest.TestCase] = []
    try:
        sys.path.insert(0, str(test_root))
        loader = unittest.TestLoader()
        for suite in selected:
            expected_path = (test_root / f"{suite.module}.py").resolve()
            snapshot = expected_path.read_bytes()
            test_sources[suite.module] = hashlib.sha256(snapshot).hexdigest()
            # Execute the bytes recorded in the receipt, not an existing module
            # or a timestamp-valid .pyc left behind by an earlier qualification.
            spec = importlib.util.spec_from_file_location(suite.module, expected_path)
            if spec is None:
                raise ValueError(f"cannot load probe source {expected_path}")
            module = importlib.util.module_from_spec(spec)
            previous_modules[suite.module] = sys.modules.get(suite.module)
            sys.modules[suite.module] = module
            # Only future imports in the probe itself may change its semantics.
            exec(
                compile(snapshot, str(expected_path), "exec", dont_inherit=True),
                module.__dict__,
            )
            loaded = loader.loadTestsFromModule(module)
            discovered.extend(cases(loaded))
            for name in suite.upstream_modules:
                upstream = importlib.import_module(name)
                if upstream.__file__ is None:
                    errors.append(f"no source path for upstream module {name}")
                else:
                    upstream_sources[name] = source_fingerprint(Path(upstream.__file__))
        errors.extend(str(error) for error in loader.errors)
        actual_ids = sorted(case.id() for case in discovered)
        receipt["discovered"] = actual_ids
        if actual_ids != required:
            missing = sorted((Counter(required) - Counter(actual_ids)).elements())
            unexpected = sorted((Counter(actual_ids) - Counter(required)).elements())
            if missing:
                errors.append("missing required tests: " + ", ".join(missing))
            if unexpected:
                errors.append("unregistered tests: " + ", ".join(unexpected))
        if errors:
            return receipt

        result = ExecutionResult()
        _ = unittest.TestSuite(discovered).run(result)
        receipt["started"] = sorted(result.started)
        receipt["passed"] = sorted(result.passed)
        for field, outcomes in (
            ("skipped", result.skipped),
            ("failures", result.failures),
            ("errors", result.errors),
            ("expected_failures", result.expectedFailures),
        ):
            receipt[field] = [
                {"test_id": test.id(), "detail": detail} for test, detail in outcomes
            ]
        receipt["unexpected_successes"] = [
            test.id() for test in result.unexpectedSuccesses
        ]
        if sorted(result.started) != required or sorted(result.passed) != required:
            errors.append("every required test must start and pass exactly once")
        if (
            not result.wasSuccessful()
            or result.skipped
            or result.expectedFailures
            or result.unexpectedSuccesses
        ):
            errors.append("model qualification forbids failures, skips, and xfails")
        if not errors:
            receipt["status"] = "passed"
    except (Exception, SystemExit) as error:
        errors.append(f"model probe execution failed: {type(error).__name__}: {error}")
    finally:
        sys.path[:] = previous_path
        for name, previous in previous_modules.items():
            if previous is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = previous
        receipt["test_source_sha256"] = test_sources
        receipt["upstream_source_sha256"] = upstream_sources
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--python-version", required=True)
    _ = parser.add_argument("--manifest", type=Path)
    _ = parser.add_argument("--receipt", type=Path)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    manifest = cast(Path | None, arguments.manifest)
    receipt_path = cast(Path | None, arguments.receipt)
    receipt = qualify(
        root,
        manifest or root / "tests/conformance/runtime-models.json",
        cast(str, arguments.python_version),
    )
    serialized = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if receipt_path is not None:
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        _ = receipt_path.write_text(serialized, encoding="utf-8")
    print(serialized, end="")
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

"""Require every declared typing rejection in source and installed helpers."""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import cast


CHECKERS = ("mypy", "pyright", "basedpyright")
CASES: dict[str, dict[str, str]] = {
    "invalid.py": {
        "borrowed-assignment": "assignment",
        "must-use-assignment": "assignment",
    },
    "outcomes_invalid.py": {
        "ok-covariance": "assignment",
        "err-covariance": "assignment",
        "result-covariance": "assignment",
        "option-covariance": "assignment",
        "frozen-payload": "read-only",
        "final-outcome": "final",
    },
}
RULES = {
    "assignment": ("assignment", "reportAssignmentType", ""),
    "read-only": ("misc", "reportAttributeAccessIssue", "read-only"),
    "final": ("misc", "reportGeneralTypeIssues", "final"),
}
CASE_MARKER = re.compile(r"# typing-case: ([a-z][a-z0-9-]*)\s*$")


@dataclass(frozen=True)
class Diagnostic:
    file: Path
    line: int
    rule: str
    severity: str
    message: str = ""


def object_fields(value: object) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError("checker output must contain JSON objects")
    return cast(dict[str, object], value)


def string_field(fields: dict[str, object], name: str) -> str:
    value = fields.get(name)
    if not isinstance(value, str):
        raise ValueError(f"checker output is missing string {name!r}")
    return value


def integer_field(fields: dict[str, object], name: str) -> int:
    value = fields.get(name)
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ValueError(f"checker output is missing nonnegative integer {name!r}")
    return value


def expected_diagnostics(checker: str, files: list[Path]) -> list[Diagnostic]:
    expected: list[Diagnostic] = []
    for file in files:
        declared = CASES[file.name]
        seen: set[str] = set()
        for line, source in enumerate(file.read_text(encoding="utf-8").splitlines(), 1):
            if "typing-case:" not in source:
                continue
            marker = CASE_MARKER.search(source)
            if marker is None:
                raise ValueError(f"malformed typing case at {file}:{line}")
            case = marker.group(1)
            if case not in declared or case in seen:
                raise ValueError(f"unknown or repeated typing case {case!r} in {file}")
            seen.add(case)
            mypy_rule, pyright_rule, fragment = RULES[declared[case]]
            expected.append(
                Diagnostic(
                    file.resolve(),
                    line,
                    mypy_rule if checker == "mypy" else pyright_rule,
                    "error",
                    fragment,
                )
            )
        if seen != set(declared):
            missing = ", ".join(sorted(set(declared) - seen))
            raise ValueError(f"missing typing cases in {file}: {missing}")
    return expected


def parse_diagnostics(
    checker: str, output: str, cwd: Path, *, expected_files: int
) -> list[Diagnostic]:
    if expected_files < 1:
        raise ValueError("typing qualification requires at least one fixture")
    if checker == "mypy":
        records = [
            object_fields(cast(object, json.loads(line)))
            for line in output.splitlines()
            if line.strip()
        ]
    else:
        document = object_fields(cast(object, json.loads(output)))
        raw = document.get("generalDiagnostics")
        if not isinstance(raw, list):
            raise ValueError("checker output is missing generalDiagnostics")
        records = [object_fields(item) for item in cast(list[object], raw)]
        summary = object_fields(document.get("summary"))
        for severity, field in (
            ("error", "errorCount"),
            ("warning", "warningCount"),
            ("information", "informationCount"),
        ):
            if integer_field(summary, field) != sum(
                record.get("severity") == severity for record in records
            ):
                raise ValueError("checker diagnostic summary is inconsistent")
        analyzed = integer_field(summary, "filesAnalyzed")
        if analyzed == 0:
            raise ValueError("checker did not analyze any files")
        if analyzed != expected_files:
            raise ValueError(
                f"checker analyzed {analyzed} files; expected {expected_files} fixtures"
            )
    diagnostics: list[Diagnostic] = []
    for record in records:
        file = Path(string_field(record, "file"))
        if not file.is_absolute():
            file = cwd / file
        if checker == "mypy":
            line = integer_field(record, "line")
            rule = string_field(record, "code")
        else:
            start = object_fields(object_fields(record.get("range")).get("start"))
            line = integer_field(start, "line") + 1
            rule = string_field(record, "rule")
        diagnostics.append(
            Diagnostic(
                file.resolve(),
                line,
                rule,
                string_field(record, "severity"),
                string_field(record, "message"),
            )
        )
    return diagnostics


def verify_output(
    checker: str,
    result: subprocess.CompletedProcess[str],
    expected: list[Diagnostic],
    cwd: Path,
    *,
    expected_files: int,
) -> None:
    expected_exit = 1 if expected else 0
    if result.returncode != expected_exit or result.stderr.strip():
        raise ValueError(
            f"{checker} failed to complete the expected check "
            f"(exit {result.returncode}, expected {expected_exit}): "
            f"{result.stderr or result.stdout}"
        )
    actual = parse_diagnostics(
        checker, result.stdout, cwd, expected_files=expected_files
    )

    def identity(diagnostic: Diagnostic) -> tuple[Path, int, str, str]:
        return diagnostic.file, diagnostic.line, diagnostic.rule, diagnostic.severity

    wanted = Counter(identity(diagnostic) for diagnostic in expected)
    observed = Counter(identity(diagnostic) for diagnostic in actual)
    if wanted != observed:
        raise ValueError(
            f"{checker} typing diagnostic mismatch; "
            f"missing={list((wanted - observed).elements())}; "
            f"unexpected={list((observed - wanted).elements())}"
        )
    by_identity = {identity(diagnostic): diagnostic for diagnostic in actual}
    for diagnostic in expected:
        if diagnostic.message not in by_identity[identity(diagnostic)].message:
            raise ValueError(
                f"{checker} returned the wrong rejection at "
                f"{diagnostic.file}:{diagnostic.line}; "
                f"expected message containing {diagnostic.message!r}"
            )


def checker_environment(root: Path, *, installed: bool) -> dict[str, str]:
    environment = os.environ.copy()
    # Never let the caller's source path replace the wheel being qualified.
    for name in ("PYTHONPATH", "MYPYPATH"):
        environment.pop(name, None)
    if not installed:
        environment["PYTHONPATH"] = str(root / "python")
        environment["MYPYPATH"] = str(root / "python")
    return environment


def check_contracts(
    root: Path,
    fixtures: Path,
    interpreter: Path,
    *,
    installed: bool,
    outcomes_only: bool,
) -> None:
    # Make relative CLI paths usable from the fixture directory, preserving a
    # virtual environment's interpreter symlink instead of resolving its target.
    interpreter = interpreter.absolute()
    environment = checker_environment(root, installed=installed)
    version = subprocess.run(
        [str(interpreter), "-c", "import sys; print('%d.%d' % sys.version_info[:2])"],
        cwd=fixtures,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    names = ["outcomes_"] if outcomes_only else ["", "outcomes_"]
    valid = [(fixtures / f"{prefix}valid.py").resolve() for prefix in names]
    invalid = [(fixtures / f"{prefix}invalid.py").resolve() for prefix in names]
    for checker in CHECKERS:
        if checker == "mypy":
            command = [
                checker,
                "--strict",
                "--output=json",
                "--no-incremental",
                "--python-executable",
                str(interpreter),
                "--python-version",
                version,
            ]
        else:
            command = [
                checker,
                "--outputjson",
                "--pythonpath",
                str(interpreter),
                "--pythonversion",
                version,
            ]
            if checker == "basedpyright":
                command.extend(["--project", str(root / "pyproject.toml")])
        expected = expected_diagnostics(checker, invalid)
        for files, diagnostics in ((valid, []), (invalid, expected)):
            result = subprocess.run(
                [*command, *(str(file) for file in files)],
                cwd=fixtures,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            verify_output(
                checker, result, diagnostics, fixtures, expected_files=len(files)
            )
        print(
            f"{checker}: valid controls passed; all {len(expected)} rejections matched"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--installed-python", type=Path)
    parser.add_argument("--outcomes-only", action="store_true")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    fixtures = cast(Path | None, arguments.fixtures) or root / "tests/checkers"
    installed_python = cast(Path | None, arguments.installed_python)
    try:
        check_contracts(
            root,
            fixtures.resolve(),
            installed_python or Path(sys.executable),
            installed=installed_python is not None,
            outcomes_only=cast(bool, arguments.outcomes_only),
        )
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Typing contract qualification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

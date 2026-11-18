"""The qualification harness must reject missing or unexecuted evidence."""

from __future__ import annotations

from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
from typing import Iterator
import unittest
from unittest.mock import patch
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from check_model_conformance import (  # noqa: E402
    environment,
    load_manifest,
    qualify,
)


@contextmanager
def fixture(
    body: str,
    methods: tuple[str, ...] = ("test_probe",),
    version: str | None = None,
    preamble: str = "",
) -> Iterator[tuple[Path, Path, str]]:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        test_root = root / "tests/python"
        test_root.mkdir(parents=True)
        module = f"test_conformance_fixture_{uuid.uuid4().hex}"
        (test_root / f"{module}.py").write_text(
            preamble + "import unittest\n\nclass Probe(unittest.TestCase):\n" + body,
            encoding="utf-8",
        )
        manifest = root / "manifest.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "suites": [
                        {
                            "id": "probe",
                            "models": ["fixture-model"],
                            "python_versions": [version or platform.python_version()],
                            "module": module,
                            "upstream_modules": ["contextlib"],
                            "tests": [f"{module}.Probe.{method}" for method in methods],
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        try:
            yield root, manifest, module
        finally:
            sys.modules.pop(module, None)


class ModelConformanceRunnerTests(unittest.TestCase):
    def test_required_case_executes_and_records_source_fingerprints(self) -> None:
        with fixture("    def test_probe(self):\n        self.assertTrue(True)\n") as (
            root,
            manifest,
            module,
        ):
            result = qualify(root, manifest, platform.python_version())
        expected = [f"{module}.Probe.test_probe"]
        self.assertEqual(result["status"], "passed", result)
        for field in ("required", "discovered", "started", "passed"):
            self.assertEqual(result[field], expected)
        self.assertEqual(result["qualification_errors"], [])
        self.assertEqual(result["skipped"], [])
        self.assertIn(module, result["test_source_sha256"])
        self.assertIn("contextlib", result["upstream_source_sha256"])
        self.assertEqual(len(result["manifest_sha256"]), 64)

    def test_annotation_behavior_comes_from_the_probe_source(self) -> None:
        for preamble, expected in (
            ("", "int"),
            ("from __future__ import annotations\n", "'int'"),
        ):
            with self.subTest(preamble=preamble):
                with fixture(
                    "    def annotated(self, value: int):\n"
                    "        pass\n"
                    "    def test_probe(self):\n"
                    "        self.assertEqual(\n"
                    "            self.annotated.__annotations__['value'], "
                    f"{expected})\n",
                    preamble=preamble,
                ) as (root, manifest, module):
                    result = qualify(root, manifest, platform.python_version())
                self.assertEqual(result["status"], "passed", result)
                self.assertEqual(result["passed"], [f"{module}.Probe.test_probe"])

    def test_unresolved_probe_annotations_fail_qualification(self) -> None:
        with fixture(
            "    def annotated(self, value: MissingAnnotation):\n"
            "        pass\n"
            # Access annotations so deferred evaluation on Python 3.14 also
            # raises during import, before any probe can count as executed.
            "    annotation = annotated.__annotations__\n"
            "    def test_probe(self):\n"
            "        pass\n"
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed", result)
        self.assertEqual(result["started"], [])
        self.assertEqual(result["passed"], [])
        self.assertIn("NameError", str(result["qualification_errors"]))
        self.assertIn("MissingAnnotation", str(result["qualification_errors"]))

    def test_wrong_runtime_is_rejected_before_importing_probes(self) -> None:
        with fixture(
            "    raise RuntimeError('wrong runtime must not import probes')\n",
            version="3.99.0",
        ) as (root, manifest, module):
            result = qualify(root, manifest, "3.99.0")
            self.assertNotIn(module, sys.modules)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["discovered"], [])
        self.assertEqual(result["started"], [])
        self.assertIn("expected CPython 3.99.0", str(result["qualification_errors"]))

    def test_other_implementation_cannot_qualify_cpython_models(self) -> None:
        actual = environment()
        actual["implementation"] = "pypy"
        with fixture("    def test_probe(self):\n        pass\n") as (
            root,
            manifest,
            _,
        ):
            with patch("check_model_conformance.environment", return_value=actual):
                result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["started"], [])

    def test_missing_required_test_prevents_execution(self) -> None:
        with fixture(
            "    def test_probe(self):\n        pass\n",
            methods=("test_probe", "test_missing"),
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["started"], [])
        self.assertIn("missing required tests", str(result["qualification_errors"]))

    def test_unregistered_test_cannot_silently_change_qualification(self) -> None:
        with fixture(
            "    def test_probe(self):\n        pass\n"
            "    def test_extra(self):\n        pass\n"
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["started"], [])
        self.assertIn("unregistered tests", str(result["qualification_errors"]))

    def test_skips_xfails_failures_and_errors_all_fail_qualification(self) -> None:
        bodies = {
            "skipped": "    @unittest.skip('unavailable')\n"
            "    def test_probe(self):\n        pass\n",
            "expected_failures": "    @unittest.expectedFailure\n"
            "    def test_probe(self):\n        self.fail('known failure')\n",
            "unexpected_successes": "    @unittest.expectedFailure\n"
            "    def test_probe(self):\n        pass\n",
            "failures": "    def test_probe(self):\n        self.fail('regression')\n",
            "errors": "    def test_probe(self):\n"
            "        raise RuntimeError('broken')\n",
        }
        for field, body in bodies.items():
            with self.subTest(field=field):
                with fixture(body) as (root, manifest, _):
                    result = qualify(root, manifest, platform.python_version())
                self.assertEqual(result["status"], "failed", result)
                self.assertTrue(result[field], result)
                self.assertEqual(result["passed"], [])

    def test_class_setup_skip_does_not_count_as_executed_evidence(self) -> None:
        with fixture(
            "    @classmethod\n"
            "    def setUpClass(cls):\n"
            "        raise unittest.SkipTest('unsupported environment')\n"
            "    def test_probe(self):\n        pass\n"
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed", result)
        self.assertEqual(result["started"], [])
        self.assertTrue(result["skipped"])

    def test_failed_subtest_cannot_count_as_a_passed_required_case(self) -> None:
        with fixture(
            "    def test_probe(self):\n"
            "        with self.subTest(exit='exceptional'):\n"
            "            self.fail('missing exceptional behavior')\n"
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed", result)
        self.assertTrue(result["failures"])
        self.assertEqual(result["passed"], [])

    def test_duplicate_required_case_is_rejected(self) -> None:
        with fixture(
            "    def test_probe(self):\n        pass\n",
            methods=("test_probe", "test_probe"),
        ) as (root, manifest, _):
            result = qualify(root, manifest, platform.python_version())
        self.assertEqual(result["status"], "failed")
        self.assertIn("duplicates", str(result["qualification_errors"]))

    def test_unknown_runtime_cannot_qualify_an_empty_selection(self) -> None:
        with fixture("    def test_probe(self):\n        pass\n") as (
            root,
            manifest,
            _,
        ):
            result = qualify(root, manifest, "3.99.0")
        self.assertEqual(result["status"], "failed")
        self.assertIn("no required model suite", str(result["qualification_errors"]))

    def test_changed_probe_executes_hashed_bytes_despite_module_and_bytecode_cache(
        self,
    ) -> None:
        with fixture("    def test_probe(self):\n        self.assertEqual(1, 1)\n") as (
            root,
            manifest,
            name,
        ):
            source = root / "tests/python" / f"{name}.py"
            spec = importlib.util.spec_from_file_location(name, source)
            self.assertIsNotNone(spec)
            module = importlib.util.module_from_spec(spec)
            sys.modules[name] = module
            spec.loader.exec_module(module)
            first = qualify(root, manifest, platform.python_version())
            self.assertEqual(first["status"], "passed", first)
            self.assertIs(sys.modules[name], module)

            original_stat = source.stat()
            source.write_text(
                source.read_text().replace("assertEqual(1, 1)", "assertEqual(1, 2)"),
                encoding="utf-8",
            )
            # Same-length, same-timestamp edits also expose stale .pyc reuse.
            os.utime(source, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
            second = qualify(root, manifest, platform.python_version())
            self.assertEqual(second["status"], "failed", second)
            self.assertTrue(second["failures"])
            self.assertNotEqual(
                first["test_source_sha256"], second["test_source_sha256"]
            )
            self.assertIs(sys.modules[name], module)

    def test_probe_import_failures_emit_failed_cli_receipts(self) -> None:
        for exception in ("RuntimeError('import failed')", "SystemExit(0)"):
            with self.subTest(exception=exception):
                with fixture(f"    raise {exception}\n") as (root, manifest, _):
                    # CLI roots probe paths at its checkout, so use an isolated
                    # copy of the runner beside this deliberately broken module.
                    scripts = root / "scripts"
                    scripts.mkdir()
                    runner = scripts / "check_model_conformance.py"
                    runner.write_bytes(
                        (ROOT / "scripts/check_model_conformance.py").read_bytes()
                    )
                    receipt = root / "receipt.json"
                    process = subprocess.run(
                        [
                            sys.executable,
                            str(runner),
                            "--python-version",
                            platform.python_version(),
                            "--manifest",
                            str(manifest),
                            "--receipt",
                            str(receipt),
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    result = json.loads(receipt.read_text())
                self.assertEqual(process.returncode, 1, process.stderr)
                self.assertEqual(result["status"], "failed", result)
                self.assertEqual(result["passed"], [])
                self.assertIn(
                    exception.split("(")[0], str(result["qualification_errors"])
                )

    def test_registry_covers_all_current_runtime_probe_modules(self) -> None:
        suites = load_manifest(ROOT / "tests/conformance/runtime-models.json")
        registered = {suite.module for suite in suites}
        actual = {
            path.stem
            for path in (ROOT / "tests/python").glob("test_*_model_conformance.py")
        }
        self.assertEqual(registered, actual)
        versions = ("3.11.15", "3.12.13", "3.13.16")
        self.assertEqual(
            {
                suite.identifier: (suite.models, suite.python_versions)
                for suite in suites
            },
            {
                "contextlib": (("TPM-CTX-001", "TPM-CTX-002"), versions),
                "task-group": (("TPM-TASK-001",), versions),
                "thread-pool-executor": (("TPM-EXECUTOR-001",), ("3.13.16",)),
                "phase08-linux": (
                    (
                        "TPM-FILE-001",
                        "TPM-ASYNCIO-001",
                        "TPM-PROCESS-001",
                        "TPM-POOL-001",
                    ),
                    ("3.13.16",),
                ),
                "phase09-work-linux": (
                    ("TPM-ASYNCIO-001", "TPM-POOL-001", "TPM-EXECUTOR-001"),
                    ("3.13.16",),
                ),
            },
        )

    def test_cli_failure_emits_failed_receipt_and_nonzero_status(self) -> None:
        with fixture("    def test_probe(self):\n        pass\n", version="3.99.0") as (
            root,
            manifest,
            _,
        ):
            receipt = root / "evidence/receipt.json"
            process = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/check_model_conformance.py"),
                    "--python-version",
                    "3.99.0",
                    "--manifest",
                    str(manifest),
                    "--receipt",
                    str(receipt),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            result = json.loads(receipt.read_text())
        self.assertEqual(process.returncode, 1, process.stderr)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["started"], [])
        self.assertEqual(json.loads(process.stdout), result)


if __name__ == "__main__":
    unittest.main()

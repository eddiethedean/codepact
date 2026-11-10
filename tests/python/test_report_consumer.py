from __future__ import annotations

import copy
import hashlib
from pathlib import Path
import tempfile
import unittest

from scripts.consume_check_report import JsonObject, consume, editor_column, read_object


class ReportConsumerTests(unittest.TestCase):
    def test_external_editor_byte_columns_keep_unicode_and_crlf(self) -> None:
        self.assertEqual(editor_column("éx🙂\r\n", 3), 2)
        self.assertEqual(editor_column("éx🙂\r\n", 8), 4)
        for column in (0, 2, 5, 11):
            with self.assertRaises(ValueError):
                _ = editor_column("éx🙂\r\n", column)

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = b"pass\n"
        _ = (self.root / "main.py").write_bytes(self.source)
        self.report = read_object(
            Path("docs/schemas/fixtures/check-report-v2.valid.json")
        )
        self.report["selected_files"] = ["main.py"]
        self.report["snapshot_manifest"] = [
            {
                "logical_path": "main.py",
                "sha256": hashlib.sha256(self.source).hexdigest(),
            }
        ]
        self.snapshot = self.set_snapshot(self.report)

    @staticmethod
    def set_snapshot(report: JsonObject) -> str:
        value = "\0".join(
            str(report[key])
            for key in (
                "semantic_fingerprint",
                "policy_fingerprint",
                "model_fingerprint",
            )
        )
        snapshot = hashlib.sha256(value.encode()).hexdigest()
        report["snapshot_id"] = snapshot
        return snapshot

    def test_current_supported_report_preserves_policy_scope(self) -> None:
        self.assertEqual(
            consume(self.report, self.root, self.snapshot), "policy-passed"
        )

    def test_changed_source_config_baseline_or_model_needs_recheck(self) -> None:
        _ = (self.root / "main.py").write_text("# moved\npass\n")
        self.assertEqual(
            consume(self.report, self.root, self.snapshot), "stale-recheck-required"
        )
        _ = (self.root / "main.py").write_bytes(self.source)
        for key in ("semantic_fingerprint", "policy_fingerprint", "model_fingerprint"):
            changed = copy.deepcopy(self.report)
            changed[key] = "c" * 64
            fresh_snapshot = self.set_snapshot(changed)
            self.assertEqual(
                consume(self.report, self.root, fresh_snapshot),
                "stale-recheck-required",
            )

    def test_unknown_schema_algorithm_and_missing_fields_fail(self) -> None:
        for key, value in [
            ("report_schema_version", 99),
            ("check_schema_version", 99),
            ("snapshot_algorithm_version", 99),
        ]:
            changed = copy.deepcopy(self.report)
            changed[key] = value
            with self.assertRaises(ValueError):
                _ = consume(changed, self.root, self.snapshot)
        unchecked = copy.deepcopy(self.report)
        unchecked["assurance"] = "lifecycle-checking-for-supported-exit-flow"
        with self.assertRaises(ValueError):
            _ = consume(unchecked, self.root, self.snapshot)
        incomplete = copy.deepcopy(self.report)
        _ = incomplete.pop("coverage")
        with self.assertRaises(ValueError):
            _ = consume(incomplete, self.root, self.snapshot)

    def test_truncated_incomplete_and_failed_reports_never_pass(self) -> None:
        limited = copy.deepcopy(self.report)
        limited["details_truncated"] = True
        self.assertEqual(
            consume(limited, self.root, self.snapshot), "limited-recheck-required"
        )
        limited["details_truncated"] = False
        limited["total_diagnostics"] = 1
        limited["counts"] = {"TP3001": 1}
        self.assertEqual(
            consume(limited, self.root, self.snapshot), "limited-recheck-required"
        )
        incomplete = copy.deepcopy(self.report)
        incomplete["coverage"] = {
            "status": "incomplete",
            "reasons": ["unknown handoff"],
            "source_bytes": 5,
            "declarations": 0,
        }
        self.assertEqual(
            consume(incomplete, self.root, self.snapshot), "incomplete-supported-scope"
        )
        failure = read_object(Path("docs/schemas/fixtures/failure-v2.valid.json"))
        self.assertEqual(consume(failure, self.root, self.snapshot), "execution-failed")

    def test_tampered_snapshots_counts_and_escaping_paths_fail(self) -> None:
        altered = copy.deepcopy(self.report)
        altered["snapshot_id"] = "0" * 64
        with self.assertRaises(ValueError):
            _ = consume(altered, self.root, self.snapshot)
        altered = copy.deepcopy(self.report)
        altered["returned_diagnostics"] = 1
        with self.assertRaises(ValueError):
            _ = consume(altered, self.root, self.snapshot)
        for logical in ["../outside.py", "/absolute.py", "a\\b.py"]:
            altered = copy.deepcopy(self.report)
            altered["snapshot_manifest"] = [
                {
                    "logical_path": logical,
                    "sha256": hashlib.sha256(self.source).hexdigest(),
                }
            ]
            with self.assertRaises(ValueError):
                _ = consume(altered, self.root, self.snapshot)

    def test_diagnostic_locations_remain_in_current_selected_source(self) -> None:
        retained = read_object(
            Path(
                "docs/planning/phases/0.4/evidence/2026-11-09-candidate-c0ea48/onboarding/lifecycle-before.json"
            )
        )
        assert isinstance(retained["diagnostics"], list)
        diagnostic = copy.deepcopy(retained["diagnostics"][0])
        assert isinstance(diagnostic, dict)
        diagnostic["primary"] = {
            "file": "main.py",
            "line": 1,
            "column": 1,
            "end_line": 1,
            "end_column": 5,
        }
        diagnostic["related"] = []
        report = copy.deepcopy(self.report)
        report["diagnostics"] = [diagnostic]
        report["total_diagnostics"] = report["returned_diagnostics"] = report[
            "actionable_diagnostics"
        ] = 1
        report["counts"] = {"TP1001": 1}
        report["exit_code"] = 1
        report["policy_failures"] = ["TP1001 supported violation"]
        self.assertEqual(consume(report, self.root, self.snapshot), "policy-failed")
        for location in [
            {
                "file": "../outside.py",
                "line": 1,
                "column": 1,
                "end_line": 1,
                "end_column": 2,
            },
            {
                "file": "main.py",
                "line": 100,
                "column": 1,
                "end_line": 100,
                "end_column": 2,
            },
            {"file": "main.py", "line": 1, "column": 4, "end_line": 1, "end_column": 2},
        ]:
            diagnostic["primary"] = location
            with self.assertRaises(ValueError):
                _ = consume(report, self.root, self.snapshot)

    def test_byte_limits_and_missing_sources_fail_closed(self) -> None:
        path = self.root / "input.json"
        _ = path.write_bytes(b" " * 12)
        with self.assertRaises(ValueError):
            _ = read_object(path, maximum=11)
        (self.root / "main.py").unlink()
        self.assertEqual(
            consume(self.report, self.root, self.snapshot), "stale-recheck-required"
        )


if __name__ == "__main__":
    unittest.main()

from dataclasses import replace
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from check_typing_contracts import (
    Diagnostic,
    checker_environment,
    expected_diagnostics,
    parse_diagnostics,
    verify_output,
)


def mypy_output(
    diagnostics: list[Diagnostic], returncode: int = 1
) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(
        ["mypy"],
        returncode,
        "\n".join(
            json.dumps(
                {
                    "file": str(item.file),
                    "line": item.line,
                    "code": item.rule,
                    "severity": item.severity,
                    "message": item.message,
                }
            )
            for item in diagnostics
        ),
        "",
    )


class TypingContractQualificationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(__file__).resolve().parents[2]
        self.fixture = self.root / "tests/checkers/outcomes_invalid.py"
        self.expected = expected_diagnostics("mypy", [self.fixture])

    def test_every_required_rejection_must_be_present(self) -> None:
        self.assertEqual(len(self.expected), 6)
        verify_output(
            "mypy",
            mypy_output(self.expected),
            self.expected,
            self.root,
            expected_files=1,
        )
        for missing in range(len(self.expected)):
            with self.subTest(missing=self.expected[missing]):
                actual = self.expected[:missing] + self.expected[missing + 1 :]
                with self.assertRaisesRegex(ValueError, "missing="):
                    verify_output(
                        "mypy",
                        mypy_output(actual),
                        self.expected,
                        self.root,
                        expected_files=1,
                    )

    def test_wrong_location_category_severity_and_duplicates_fail(self) -> None:
        first = self.expected[0]
        for replacement in (
            replace(first, line=first.line + 1),
            replace(first, file=first.file.with_name("unrelated.py")),
            replace(first, rule="unrelated"),
            replace(first, severity="warning"),
        ):
            with self.subTest(replacement=replacement):
                with self.assertRaisesRegex(ValueError, "diagnostic mismatch"):
                    verify_output(
                        "mypy",
                        mypy_output([replacement, *self.expected[1:]]),
                        self.expected,
                        self.root,
                        expected_files=1,
                    )
        with self.assertRaisesRegex(ValueError, "unexpected="):
            verify_output(
                "mypy",
                mypy_output([*self.expected, first]),
                self.expected,
                self.root,
                expected_files=1,
            )

    def test_generic_category_cannot_hide_an_unrelated_error(self) -> None:
        actual = [replace(item, message="unrelated problem") for item in self.expected]
        with self.assertRaisesRegex(ValueError, "wrong rejection"):
            verify_output(
                "mypy", mypy_output(actual), self.expected, self.root, expected_files=1
            )

    def test_success_or_crash_cannot_qualify_negative_cases(self) -> None:
        for returncode in (0, 2, -9):
            with self.subTest(returncode=returncode):
                with self.assertRaisesRegex(ValueError, "failed to complete"):
                    verify_output(
                        "mypy",
                        mypy_output(self.expected, returncode),
                        self.expected,
                        self.root,
                        expected_files=1,
                    )
        result = mypy_output(self.expected)
        result.stderr = "checker failed after reporting diagnostics"
        with self.assertRaisesRegex(ValueError, "failed to complete"):
            verify_output("mypy", result, self.expected, self.root, expected_files=1)

    def test_valid_controls_reject_unexpected_diagnostics(self) -> None:
        verify_output("mypy", mypy_output([], 0), [], self.root, expected_files=1)
        with self.assertRaisesRegex(ValueError, "diagnostic mismatch"):
            verify_output(
                "mypy", mypy_output(self.expected, 0), [], self.root, expected_files=1
            )

    def test_removed_or_duplicate_case_marker_cannot_reduce_the_oracle(self) -> None:
        original = self.fixture.read_text(encoding="utf-8")
        for replacement in (
            original.replace("# typing-case: frozen-payload", "# removed"),
            original.replace(
                "typing-case: final-outcome", "typing-case: frozen-payload"
            ),
        ):
            with tempfile.TemporaryDirectory() as directory:
                fixture = Path(directory) / self.fixture.name
                fixture.write_text(replacement, encoding="utf-8")
                with self.assertRaises(ValueError):
                    expected_diagnostics("mypy", [fixture])

    def test_case_markers_follow_line_moves(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory) / self.fixture.name
            fixture.write_text(
                "\n" + self.fixture.read_text(encoding="utf-8"), encoding="utf-8"
            )
            shifted = expected_diagnostics("mypy", [fixture])
            self.assertEqual(
                [item.line for item in shifted],
                [item.line + 1 for item in self.expected],
            )

    def test_pyright_json_uses_one_based_lines_and_exact_rules(self) -> None:
        for checker in ("pyright", "basedpyright"):
            expected = expected_diagnostics(checker, [self.fixture])
            diagnostics = [
                {
                    "file": str(item.file),
                    "severity": item.severity,
                    "message": item.message,
                    "range": {"start": {"line": item.line - 1}},
                    "rule": item.rule,
                }
                for item in expected
            ]
            payload = {
                "generalDiagnostics": diagnostics,
                "summary": {
                    "filesAnalyzed": 1,
                    "errorCount": 6,
                    "warningCount": 0,
                    "informationCount": 0,
                },
            }
            result = subprocess.CompletedProcess([checker], 1, json.dumps(payload), "")
            verify_output(checker, result, expected, self.root, expected_files=1)
            self.assertEqual(
                parse_diagnostics(checker, result.stdout, self.root, expected_files=1),
                expected,
            )
            diagnostics.pop()
            result.stdout = json.dumps(payload)
            with self.assertRaisesRegex(ValueError, "summary is inconsistent"):
                verify_output(checker, result, expected, self.root, expected_files=1)

    def test_malformed_output_and_zero_files_do_not_qualify(self) -> None:
        for output in ("not JSON", "{}", "[]", '{"generalDiagnostics": []}'):
            with self.subTest(output=output):
                with self.assertRaises(ValueError):
                    parse_diagnostics("pyright", output, self.root, expected_files=1)
        empty = json.dumps(
            {
                "generalDiagnostics": [],
                "summary": {
                    "filesAnalyzed": 0,
                    "errorCount": 0,
                    "warningCount": 0,
                    "informationCount": 0,
                },
            }
        )
        with self.assertRaisesRegex(ValueError, "did not analyze any files"):
            parse_diagnostics("pyright", empty, self.root, expected_files=1)

    def test_installed_checks_cannot_inherit_source_import_paths(self) -> None:
        with patch.dict(os.environ, {"PYTHONPATH": "source", "MYPYPATH": "stubs"}):
            environment = checker_environment(self.root, installed=True)
            self.assertNotIn("PYTHONPATH", environment)
            self.assertNotIn("MYPYPATH", environment)
            source = checker_environment(self.root, installed=False)
            self.assertEqual(source["PYTHONPATH"], str(self.root / "python"))
            self.assertEqual(source["MYPYPATH"], str(self.root / "python"))

    def test_missing_positive_fixture_cannot_qualify_a_clean_result(self) -> None:
        for checker in ("pyright", "basedpyright"):
            payload = {
                "generalDiagnostics": [],
                "summary": {
                    "filesAnalyzed": 2,
                    "errorCount": 0,
                    "warningCount": 0,
                    "informationCount": 0,
                },
            }
            result = subprocess.CompletedProcess([checker], 0, json.dumps(payload), "")
            verify_output(checker, result, [], self.root, expected_files=2)
            payload["summary"]["filesAnalyzed"] = 1
            result.stdout = json.dumps(payload)
            with self.assertRaisesRegex(ValueError, "analyzed 1 files; expected 2"):
                verify_output(checker, result, [], self.root, expected_files=2)
            # Installed qualification intentionally requests only the outcome file.
            verify_output(checker, result, [], self.root, expected_files=1)


if __name__ == "__main__":
    unittest.main()

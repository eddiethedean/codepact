from pathlib import Path
import sys
import tempfile
import unittest

sys_path_root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(sys_path_root / "scripts"))

from check_phase03_validation import (  # noqa: E402
    EXPECTED_ROWS,
    validation_errors,
)


def write_matrix(
    root: Path, *, missing: str | None = None, stale: bool = False
) -> None:
    matrix_dir = root / "docs/planning/phases/0.3"
    matrix_dir.mkdir(parents=True)
    required = [
        f"| {identifier} | Scenario | Expected result | C01 |"
        for identifier in EXPECTED_ROWS
        if identifier != missing
    ]
    coverage = [
        f"| {identifier} | `{identifier.lower().replace('-', '_')}_case` | "
        "Remaining review |"
        for identifier in EXPECTED_ROWS
        if identifier != missing
    ]
    if stale:
        coverage[0] = coverage[0].replace("v03_01_case", "v03_01_missing_case")
    (matrix_dir / "validation.md").write_text(
        "# Fixture\n\n## Required matrix\n\n"
        + "\n".join(required)
        + "\n\n## Current automated coverage map\n\n"
        + "\n".join(coverage)
        + "\n\n## Other\n",
        encoding="utf-8",
    )
    source_dir = root / "crates/codepact/tests"
    source_dir.mkdir(parents=True)
    definitions = [
        f"#[test]\nfn {identifier.lower().replace('-', '_')}_case() {{}}"
        for identifier in EXPECTED_ROWS
        if identifier != missing
    ]
    (source_dir / "phase03.rs").write_text("\n".join(definitions), encoding="utf-8")


class Phase03ValidationTest(unittest.TestCase):
    def test_repository_matrix_rows_and_symbols_are_consistent(self) -> None:
        self.assertEqual(validation_errors(sys_path_root), [])

    def test_missing_matrix_row_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_matrix(root, missing="V03-12")
            errors = validation_errors(root)
            self.assertTrue(any("V03-12" in error for error in errors))

    def test_stale_test_reference_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_matrix(root, stale=True)
            errors = validation_errors(root)
            self.assertTrue(any("v03_01_missing_case" in error for error in errors))

    def test_ordinary_function_does_not_satisfy_a_test_reference(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_matrix(root, stale=True)
            source = root / "crates/codepact/tests/phase03.rs"
            with source.open("a", encoding="utf-8") as test_file:
                test_file.write("\nfn v03_01_missing_case() {}\n")
            errors = validation_errors(root)
            self.assertTrue(
                any("missing Rust tests" in error for error in errors), errors
            )


if __name__ == "__main__":
    unittest.main()

"""Keep the 0.3 validation matrix and its automated test references in sync."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import cast


EXPECTED_ROWS = tuple(f"V03-{number:02}" for number in range(1, 27))
CODE_ROOTS = (
    Path("crates/codepact/src"),
    Path("crates/codepact/tests"),
    Path("tests"),
)
RUST_TEST = re.compile(
    r"(?m)^\s*#\[test\]\s*(?:^\s*#\[[^\]]+\]\s*)*"
    r"^\s*(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("
)
RUST_FUNCTION = re.compile(
    r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?"
    r"fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]+>\s*)?\("
)


def table_section(text: str, heading: str) -> str:
    marker = f"## {heading}\n"
    start = text.find(marker)
    if start < 0:
        return ""
    start += len(marker)
    end = re.search(r"\n## ", text[start:])
    return text[start : start + end.start()] if end else text[start:]


def table_rows(section: str) -> tuple[list[str], list[str]]:
    identifiers: list[str] = []
    errors: list[str] = []
    for line_number, line in enumerate(section.splitlines(), start=1):
        if not line.strip().startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if not cells:
            continue
        match = re.match(r"(V03-\d{2})\b", cells[0])
        if match is None:
            continue
        identifier = match.group(1)
        if identifier in identifiers:
            errors.append(f"duplicate {identifier} row at table line {line_number}")
        identifiers.append(identifier)
        if len(cells) < 3 or not cells[1] or not cells[2]:
            errors.append(f"{identifier} row is missing required table content")
    return identifiers, errors


def validation_errors(root: Path) -> list[str]:
    matrix_path = root / "docs/planning/phases/0.3/validation.md"
    if not matrix_path.is_file():
        return [f"missing phase 0.3 validation matrix: {matrix_path.relative_to(root)}"]

    text = matrix_path.read_text(encoding="utf-8")
    errors: list[str] = []
    matrix_rows, matrix_errors = table_rows(table_section(text, "Required matrix"))
    coverage, coverage_errors = table_rows(
        table_section(text, "Current automated coverage map")
    )
    errors.extend(f"required matrix: {error}" for error in matrix_errors)
    errors.extend(f"coverage map: {error}" for error in coverage_errors)
    tables = (("required matrix", matrix_rows), ("coverage map", coverage))
    for label, identifiers in tables:
        missing = [
            identifier for identifier in EXPECTED_ROWS if identifier not in identifiers
        ]
        unexpected = [
            identifier for identifier in identifiers if identifier not in EXPECTED_ROWS
        ]
        if missing:
            errors.append(f"{label} is missing rows: {', '.join(missing)}")
        if unexpected:
            errors.append(f"{label} has unknown rows: {', '.join(unexpected)}")

    coverage_text = table_section(text, "Current automated coverage map")
    test_references = cast(
        list[str], re.findall(r"\bv03_[A-Za-z0-9_]+\b", coverage_text)
    )
    qualified_references = cast(
        list[str], re.findall(r"::([A-Za-z_][A-Za-z0-9_]*)", coverage_text)
    )
    references = set(test_references + qualified_references)
    source_files = [
        path
        for code_root in CODE_ROOTS
        for path in (root / code_root).rglob("*")
        if path.is_file() and path.suffix in {".rs", ".py"}
    ]
    source_text = "\n".join(
        path.read_text(encoding="utf-8", errors="replace") for path in source_files
    )
    rust_tests: set[str] = set()
    rust_functions: set[str] = set()
    for path in source_files:
        if path.suffix != ".rs":
            continue
        rust_text = path.read_text(encoding="utf-8", errors="replace")
        rust_functions.update(RUST_FUNCTION.findall(rust_text))
        # Tests can live in either integration-test files or a crate's `src`
        # tree. Qualified references in the matrix may name both.
        rust_tests.update(RUST_TEST.findall(rust_text))
    stale_tests = sorted(set(test_references) - rust_tests)
    if stale_tests:
        errors.append(
            "coverage map references missing Rust tests: " + ", ".join(stale_tests)
        )

    stale_qualified = sorted(set(qualified_references) - rust_functions)
    if stale_qualified:
        errors.append(
            "coverage map references missing Rust functions: "
            + ", ".join(stale_qualified)
        )

    stale = sorted(
        reference for reference in references if reference not in source_text
    )
    if stale:
        errors.append(
            "coverage map references missing source symbols: " + ", ".join(stale)
        )
    return errors


def main() -> int:
    root = Path.cwd()
    errors = validation_errors(root)
    if errors:
        for error in errors:
            print(f"phase 0.3 validation check failed: {error}", file=sys.stderr)
        return 1
    print("Phase 0.3 validation rows and source references are consistent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

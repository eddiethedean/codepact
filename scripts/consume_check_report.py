"""Reference CLI consumer: validate schema, snapshot and current source bytes.

This development tool requires the pinned jsonschema development dependency.
A trusted fresh snapshot ID is required because source hashes alone cannot
establish that configuration, baseline or model policy is still current.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
from typing import TypeAlias, cast

from jsonschema import Draft202012Validator
from jsonschema.protocols import Validator
from referencing import Registry, Resource
from referencing.jsonschema import Schema

JsonValue: TypeAlias = (
    None | bool | int | float | str | list["JsonValue"] | dict[str, "JsonValue"]
)
JsonObject = dict[str, JsonValue]
SCHEMAS = Path(__file__).resolve().parents[1] / "docs/schemas"


def editor_column(source_line: str, byte_column: int) -> int:
    """Convert a one-based UTF-8 byte column to a one-based code-point column."""
    encoded = source_line.encode("utf-8")
    if not 1 <= byte_column <= len(encoded) + 1:
        raise ValueError("source column is outside the current line")
    try:
        return len(encoded[: byte_column - 1].decode("utf-8")) + 1
    except UnicodeDecodeError as error:
        raise ValueError("source column splits a UTF-8 code point") from error


def read_object(path: Path, maximum: int = 32 * 1024 * 1024) -> JsonObject:
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    if len(raw) > maximum:
        raise ValueError("report exceeds its byte limit")
    value = cast(JsonValue, json.loads(raw))
    if not isinstance(value, dict):
        raise ValueError("expected a JSON object")
    return cast(JsonObject, value)


def validate_document(document: JsonObject, schema: str) -> None:
    resources: list[tuple[str, Resource[Schema]]] = []
    for path in sorted(SCHEMAS.glob("*.schema.json")):
        contents = read_object(path)
        identifier = contents.get("$id")
        if not isinstance(identifier, str):
            raise ValueError("schema has no identifier")
        resources.append((identifier, Resource.from_contents(contents)))
    registry = Registry[Schema]().with_resources(resources)
    validator = cast(
        Validator,
        Draft202012Validator(read_object(SCHEMAS / schema), registry=registry),
    )
    error = next(iter(validator.iter_errors(document)), None)
    if error is not None:
        raise ValueError(f"invalid CodePact document: {error.message}")


def consume(document: JsonObject, root: Path, expected_snapshot: str) -> str:
    """Return an explicit policy/scope result; never infer whole-program safety."""
    if document.get("execution_status") == "failed":
        validate_document(document, "failure-v2.schema.json")
        return "execution-failed"
    validate_document(document, "check-report-v2.schema.json")
    if document.get("snapshot_algorithm_version") != 1:
        raise ValueError("unsupported snapshot algorithm")
    semantic, policy, model = (
        document.get(key)
        for key in ("semantic_fingerprint", "policy_fingerprint", "model_fingerprint")
    )
    encoded = f"{semantic}\0{policy}\0{model}".encode()
    calculated = hashlib.sha256(encoded).hexdigest()
    if document.get("snapshot_id") != calculated:
        raise ValueError("snapshot identity does not match its components")
    if expected_snapshot != calculated:
        return "stale-recheck-required"
    manifest = document.get("snapshot_manifest")
    if not isinstance(manifest, list):
        raise ValueError("missing source manifest")
    source_lines: dict[str, list[bytes]] = {}
    paths: list[str] = []
    total_bytes = 0
    for item in manifest:
        if not isinstance(item, dict):
            raise ValueError("invalid manifest entry")
        entry = cast(JsonObject, item)
        logical = entry.get("logical_path")
        if not isinstance(logical, str):
            raise ValueError("invalid logical path")
        parts = PurePosixPath(logical)
        if parts.is_absolute() or ".." in parts.parts or "\\" in logical:
            raise ValueError("manifest path escapes the project")
        path = root
        for part in parts.parts:
            path = path / part
            if path.is_symlink():
                raise ValueError("manifest contains a symlink")
        try:
            with path.open("rb") as stream:
                raw = stream.read(1_000_001 - total_bytes)
        except OSError:
            return "stale-recheck-required"
        total_bytes += len(raw)
        if total_bytes > 1_000_000:
            return "stale-recheck-required"
        if hashlib.sha256(raw).hexdigest() != entry.get("sha256"):
            return "stale-recheck-required"
        source_lines[logical] = raw.split(b"\n")
        paths.append(logical)
    if len(set(paths)) != len(paths) or document.get("selected_files") != paths:
        raise ValueError("source manifest does not match selected files")
    diagnostics = document.get("diagnostics")
    if not isinstance(diagnostics, list):
        raise ValueError("missing diagnostics")
    for diagnostic in diagnostics:
        if not isinstance(diagnostic, dict):
            raise ValueError("invalid diagnostic")
        related = diagnostic.get("related")
        if not isinstance(related, list):
            raise ValueError("invalid related locations")
        for location in [diagnostic.get("primary"), *related]:
            if not isinstance(location, dict):
                raise ValueError("invalid source location")
            file = location.get("file")
            if not isinstance(file, str) or file not in source_lines:
                raise ValueError("diagnostic points outside the selected source scope")
            positions: list[tuple[int, int]] = []
            for line_key, column_key in [
                ("line", "column"),
                ("end_line", "end_column"),
            ]:
                line, column = location.get(line_key), location.get(column_key)
                if not isinstance(line, int) or not isinstance(column, int):
                    raise ValueError("invalid source coordinates")
                lines = source_lines[file]
                if (
                    not 1 <= line <= len(lines)
                    or not 1 <= column <= len(lines[line - 1]) + 1
                ):
                    raise ValueError("source location is outside the current file")
                encoded = lines[line - 1]
                if column <= len(encoded) and encoded[column - 1] & 0xC0 == 0x80:
                    raise ValueError("source location splits a UTF-8 code point")
                positions.append((line, column))
            if positions[1] < positions[0]:
                raise ValueError("source span end precedes its start")
    total = document.get("total_diagnostics")
    returned = document.get("returned_diagnostics")
    if (
        not isinstance(total, int)
        or not isinstance(returned, int)
        or returned != len(diagnostics)
        or total < returned
    ):
        raise ValueError("inconsistent diagnostic counts")
    failures = document.get("policy_failures")
    expected_exit = 1 if failures else 0
    if document.get("exit_code") != expected_exit:
        raise ValueError("policy failure reasons disagree with the exit code")
    counts = document.get("counts")
    if (
        not isinstance(counts, dict)
        or sum(value for value in counts.values() if isinstance(value, int)) != total
    ):
        raise ValueError("rule counts disagree with the full diagnostic count")
    if document.get("details_truncated") or total != returned:
        return "limited-recheck-required"
    coverage = document.get("coverage")
    if not isinstance(coverage, dict):
        raise ValueError("missing coverage")
    if coverage.get("status") != "complete-for-supported-subset":
        return "incomplete-supported-scope"
    return "policy-passed" if document.get("exit_code") == 0 else "policy-failed"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("report", type=Path)
    _ = parser.add_argument("--root", type=Path, required=True)
    _ = parser.add_argument("--expected-snapshot", required=True)
    args = parser.parse_args()
    # argparse's dynamic namespace is checked at this boundary.
    report = cast(Path, args.report)
    root = cast(Path, args.root)
    expected = cast(str, args.expected_snapshot)
    try:
        result = consume(read_object(report), root.resolve(strict=True), expected)
    except (ValueError, OSError) as error:
        print(f"invalid-or-unavailable-report: {error}")
        return 2
    print(result)
    return 0 if result == "policy-passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

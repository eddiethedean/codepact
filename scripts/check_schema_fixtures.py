"""Validate published JSON schemas and their version-boundary fixtures."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import TypeAlias, cast

from jsonschema import Draft202012Validator
from jsonschema.protocols import Validator
from referencing import Registry, Resource
from referencing.jsonschema import Schema, SchemaRegistry


SCHEMA_ROOT = Path("docs/schemas")
FIXTURE_ROOT = SCHEMA_ROOT / "fixtures"
JsonValue: TypeAlias = (
    None | bool | int | float | str | list["JsonValue"] | dict[str, "JsonValue"]
)
JsonObject = dict[str, JsonValue]


def read_object(path: Path) -> JsonObject:
    value = cast(JsonValue, json.loads(path.read_text(encoding="utf-8")))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return cast(JsonObject, value)


def cli_output(arguments: list[str], expected_exit: int = 0) -> str:
    command = [
        "cargo",
        "run",
        "--locked",
        "--quiet",
        "-p",
        "codepact",
        "--",
        *arguments,
    ]
    if binary := os.environ.get("CODEPACT_QUALIFICATION_BINARY"):
        command = [binary, *arguments]
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != expected_exit:
        raise RuntimeError(
            f"CLI command {arguments!r} exited {result.returncode}, "
            f"expected {expected_exit}: {result.stderr.strip()}"
        )
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument(
        "--fixtures-only",
        action="store_true",
        help="validate published schemas/fixtures without invoking the Rust CLI",
    )
    fixtures_only = cast(bool, parser.parse_args().fixtures_only)
    schema_paths = sorted(SCHEMA_ROOT.glob("*.schema.json"))
    if not schema_paths:
        print("no published JSON schemas were found", file=sys.stderr)
        return 1

    schemas: dict[str, JsonObject] = {}
    resources: list[tuple[str, Resource[Schema]]] = []
    for path in schema_paths:
        schema = read_object(path)
        Draft202012Validator.check_schema(schema)
        identifier = schema.get("$id")
        if not isinstance(identifier, str):
            raise ValueError(f"{path} has no string $id")
        schemas[path.name] = schema
        resources.append((identifier, Resource.from_contents(schema)))
    registry: SchemaRegistry = Registry[Schema]().with_resources(resources)

    checked = 0
    for filename, schema in schemas.items():
        stem = filename.removesuffix(".schema.json")
        valid_path = FIXTURE_ROOT / f"{stem}.valid.json"
        if not valid_path.is_file():
            raise ValueError(f"missing valid fixture: {valid_path}")
        validator = cast(Validator, Draft202012Validator(schema, registry=registry))
        valid = read_object(valid_path)
        errors = list(validator.iter_errors(valid))
        if errors:
            error = errors[0]
            raise ValueError(
                f"{valid_path} is invalid at "
                f"{list(error.absolute_path)}: {error.message}"
            )
        checked += 1

        for invalid_path in sorted(FIXTURE_ROOT.glob(f"{stem}.invalid*.json")):
            invalid = read_object(invalid_path)
            if not list(validator.iter_errors(invalid)):
                raise ValueError(
                    f"invalid fixture unexpectedly validates: {invalid_path}"
                )
            checked += 1

    if fixtures_only:
        print(f"Validated {len(schemas)} schemas and {checked} valid/invalid fixtures")
        return 0

    emitted_count = 0

    def validate_emitted(label: str, schema_name: str, output: str) -> None:
        nonlocal emitted_count
        schema = schemas.get(schema_name)
        if schema is None:
            raise ValueError(f"missing published schema: {schema_name}")
        document = cast(JsonValue, json.loads(output))
        validator = cast(Validator, Draft202012Validator(schema, registry=registry))
        errors = list(validator.iter_errors(document))
        if errors:
            error = errors[0]
            raise ValueError(
                f"current CLI {label} output is invalid at "
                f"{list(error.absolute_path)}: {error.message}"
            )
        emitted_count += 1

    validate_emitted(
        "capabilities",
        "capabilities-v1.schema.json",
        cli_output(["capabilities", "--json"]),
    )
    validate_emitted(
        "explanation",
        "explanation-v1.schema.json",
        cli_output(["explain", "TP1001", "--json"]),
    )

    with tempfile.TemporaryDirectory(prefix="codepact-schema-output-") as temp:
        root = Path(temp)
        project = root / "project"
        project.mkdir()
        source = project / "main.py"
        source.write_text("def main():\n    return 1\n", encoding="utf-8")
        check_args = [
            "check",
            "--root",
            str(project),
            "--no-config",
            "--json",
        ]
        validate_emitted(
            "completed check",
            "check-report-v2.schema.json",
            cli_output(check_args),
        )
        validate_emitted(
            "settings",
            "settings-v1.schema.json",
            cli_output(["check", "--root", str(project), "--show-settings", "--json"]),
        )
        (project / "invalid.toml").write_text(
            "[tool.codepact]\nconfig-version=999\n", encoding="utf-8"
        )
        validate_emitted(
            "failed check",
            "failure-v2.schema.json",
            cli_output(
                [
                    "check",
                    "--root",
                    str(project),
                    "--config",
                    str(project / "invalid.toml"),
                    "--json",
                ],
                expected_exit=2,
            ),
        )
        baseline_path = root / "baseline.json"
        baseline_summary = cli_output(
            [
                "baseline",
                "update",
                "--output",
                str(baseline_path),
                "--reason",
                "schema output validation",
                "--root",
                str(project),
                "--no-config",
                "--json",
            ]
        )
        validate_emitted(
            "baseline",
            "baseline-v1.schema.json",
            baseline_path.read_text(encoding="utf-8"),
        )
        validate_emitted(
            "baseline update",
            "baseline-update-v1.schema.json",
            baseline_summary,
        )
        validate_emitted(
            "inspection",
            "inspection-v0.schema.json",
            cli_output(["inspect-contracts", str(source)]),
        )

    print(
        f"Validated {len(schemas)} schemas, {checked} valid/invalid fixtures, "
        f"and {emitted_count} current CLI JSON outputs"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

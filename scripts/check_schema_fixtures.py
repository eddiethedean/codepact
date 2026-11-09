"""Validate published JSON schemas and their version-boundary fixtures."""

from __future__ import annotations

import json
import subprocess
import sys
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


def main() -> int:
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

    capability_schema = schemas.get("capabilities-v1.schema.json")
    if capability_schema is None:
        raise ValueError("missing published capabilities-v1 schema")
    command = [
        "cargo",
        "run",
        "--locked",
        "--quiet",
        "-p",
        "codepact",
        "--",
        "capabilities",
        "--json",
    ]
    emitted = subprocess.run(
        command,
        check=True,
        capture_output=True,
        text=True,
    )
    emitted_document = cast(JsonValue, json.loads(emitted.stdout))
    validator = cast(
        Validator, Draft202012Validator(capability_schema, registry=registry)
    )
    errors = list(validator.iter_errors(emitted_document))
    if errors:
        error = errors[0]
        raise ValueError(
            "the current CLI capabilities output is invalid at "
            f"{list(error.absolute_path)}: {error.message}"
        )
    checked += 1

    print(
        f"Validated {len(schemas)} schemas, {checked - 1} valid/invalid fixtures, "
        "and current CLI capabilities output"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

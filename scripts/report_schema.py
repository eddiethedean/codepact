"""Shared JSON types and schema reference registry for development tooling."""

from collections.abc import Iterable
from typing import TypeAlias

from referencing import Registry, Resource
from referencing.jsonschema import Schema, SchemaRegistry

JsonValue: TypeAlias = (
    None | bool | int | float | str | list["JsonValue"] | dict[str, "JsonValue"]
)
JsonObject = dict[str, JsonValue]


def schema_registry(schemas: Iterable[tuple[str, JsonObject]]) -> SchemaRegistry:
    resources: list[tuple[str, Resource[Schema]]] = []
    for filename, schema in schemas:
        identifier = schema.get("$id")
        if not isinstance(identifier, str):
            raise ValueError(f"{filename} has no string $id")
        resources.append((identifier, Resource.from_contents(schema)))
    return Registry[Schema]().with_resources(resources)

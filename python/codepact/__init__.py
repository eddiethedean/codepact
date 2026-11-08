"""Lightweight runtime declarations for CodePact.

Importing this module never loads the Rust inspector or executes analysis.
"""

from collections.abc import Callable
from typing import TypeVar, overload

from .contracts import Resource

T = TypeVar("T")


def _is_identifier(value: str) -> bool:
    try:
        return value.isidentifier()
    except AttributeError:
        return False


@overload
def resource(*, closed_by: str) -> Callable[[type[T]], type[T]]: ...


@overload
def resource(cls: type[T], *, closed_by: str) -> type[T]: ...


def resource(
    cls: type[T] | None = None, *, closed_by: str
) -> type[T] | Callable[[type[T]], type[T]]:
    """Attach a declared lifecycle contract while preserving class identity."""
    if not _is_identifier(closed_by):
        raise ValueError("closed_by must be a method name")

    def decorate(target: type[T]) -> type[T]:
        existing = target.__dict__.get("__codepact_resource__")
        contract = Resource(closed_by=closed_by)
        if existing is not None and existing != contract:
            raise ValueError("conflicting CodePact resource declarations")
        setattr(target, "__codepact_resource__", contract)
        return target

    return decorate(cls) if cls is not None else decorate


__all__ = ["resource", "Resource"]

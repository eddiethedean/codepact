"""Typed result and optional values for ordinary Python programs."""

from __future__ import annotations

import concurrent.futures
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Generic, ParamSpec, TypeAlias, TypeVar, cast, final

T_co = TypeVar("T_co", covariant=True)
E_co = TypeVar("E_co", covariant=True)
T = TypeVar("T")
E = TypeVar("E", bound=Exception)
P = ParamSpec("P")


@final
@dataclass(frozen=True, slots=True)
class Ok(Generic[T_co]):
    """A successful result carrying a value."""

    value: T_co


@final
@dataclass(frozen=True, slots=True)
class Err(Generic[E_co]):
    """A failed result carrying an exception or error value."""

    error: E_co


@final
@dataclass(frozen=True, slots=True)
class Some(Generic[T_co]):
    """A present optional value, including an explicit None."""

    value: T_co


@final
@dataclass(frozen=True, slots=True)
class Nothing:
    """An absent optional value."""


Result: TypeAlias = Ok[T_co] | Err[E_co]
Option: TypeAlias = Some[T_co] | Nothing


def from_nullable(value: T | None, /) -> Option[T]:
    """Wrap a nullable value without confusing falsey values with absence."""
    if value is None:
        return Nothing()
    return Some(value)


def to_nullable(value: Option[T], /) -> T | None:
    """Convert an optional value to None or its payload."""
    if isinstance(value, Some):
        return value.value
    return None


def _validate_exceptions(exceptions: object) -> None:
    if not isinstance(exceptions, tuple) or not exceptions:
        raise TypeError("exceptions must be a nonempty tuple of Exception classes")
    exception_values = cast(tuple[object, ...], exceptions)
    for exception in exception_values:
        if not isinstance(exception, type) or not issubclass(exception, Exception):
            raise TypeError("exceptions must contain only Exception classes")


def capture(
    function: Callable[P, T],
    exceptions: tuple[type[E], ...],
    /,
    *args: P.args,
    **kwargs: P.kwargs,
) -> Result[T, E]:
    """Call once, converting only explicitly selected exceptions into Err."""
    _validate_exceptions(exceptions)
    try:
        return Ok(function(*args, **kwargs))
    except Exception as error:
        if isinstance(
            error, (BaseExceptionGroup, concurrent.futures.CancelledError)
        ) or not isinstance(error, exceptions):
            raise
        return Err(error)


async def capture_async(
    function: Callable[P, Awaitable[T]],
    exceptions: tuple[type[E], ...],
    /,
    *args: P.args,
    **kwargs: P.kwargs,
) -> Result[T, E]:
    """Await once in the current task, converting selected exceptions to Err."""
    _validate_exceptions(exceptions)
    try:
        return Ok(await function(*args, **kwargs))
    except Exception as error:
        if isinstance(
            error, (BaseExceptionGroup, concurrent.futures.CancelledError)
        ) or not isinstance(error, exceptions):
            raise
        return Err(error)


__all__ = [
    "Ok",
    "Err",
    "Some",
    "Nothing",
    "Result",
    "Option",
    "from_nullable",
    "to_nullable",
    "capture",
    "capture_async",
]

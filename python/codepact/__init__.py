"""Lightweight runtime declarations for CodePact.

Importing this module never loads the Rust inspector or executes analysis.
"""

import keyword
from collections.abc import Callable, Iterable
from typing import ParamSpec, TypeVar, cast, overload

from .contracts import MustHandle, MustHandleOutcome, MustUseResult, Resource, State
from .outcomes import (
    Err,
    Nothing,
    Ok,
    Option,
    Result,
    Some,
    capture,
    capture_async,
    from_nullable,
    to_nullable,
)

T = TypeVar("T")
P = ParamSpec("P")
R = TypeVar("R")


def _is_identifier(value: str) -> bool:
    try:
        return value.isidentifier()
    except AttributeError:
        return False


def _is_state_name(value: str) -> bool:
    return _is_identifier(value) and not keyword.iskeyword(value)


def _validate_state_names(
    names: tuple[str, ...], *, invalid: str, duplicate: str
) -> None:
    if not names or any(not _is_state_name(name) for name in names):
        raise ValueError(invalid)
    if len(set(names)) != len(names):
        raise ValueError(duplicate)


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


def must_use(function: Callable[P, R]) -> Callable[P, R]:
    """Mark a synchronous function result as requiring immediate handling."""
    existing = getattr(function, "__codepact_must_use__", None)
    if existing is not None and existing != MustUseResult():
        raise ValueError("conflicting CodePact must-use declarations")
    setattr(function, "__codepact_must_use__", MustUseResult())
    return function


def discard(value: object) -> None:
    """Document intentional immediate abandonment of a value."""
    del value


def discard_outcome(
    value: Result[object, object] | Option[object], *, reason: str
) -> None:
    """Document intentional abandonment of an outcome and its reason."""
    if not isinstance(cast(object, reason), str) or not reason.strip():
        raise ValueError("discard_outcome requires a nonempty reason")
    del value


def state_machine(
    *, states: Iterable[str], initial: str
) -> Callable[[type[T]], type[T]]:
    """Declare a finite protocol state set and its initial state."""
    state_names = tuple(states)
    _validate_state_names(
        state_names,
        invalid="states must contain valid, nonempty state names",
        duplicate="states must be unique",
    )
    if initial not in state_names:
        raise ValueError("initial must name one of the declared states")

    def decorate(target: type[T]) -> type[T]:
        existing = target.__dict__.get("__codepact_state_machine__")
        declaration = (state_names, initial)
        if existing is not None and existing != declaration:
            raise ValueError("conflicting CodePact state-machine declarations")
        setattr(target, "__codepact_state_machine__", declaration)
        return target

    return decorate


def transition(
    *,
    from_state: str | tuple[str, ...],
    to_state: str,
    on_raise: str | None = None,
    on_cancel: str | None = None,
) -> Callable[[Callable[P, R]], Callable[P, R]]:
    """Declare protocol preconditions and completion states for a method."""
    from_states = (from_state,) if isinstance(from_state, str) else tuple(from_state)
    _validate_state_names(
        from_states,
        invalid="from_state must contain valid state names",
        duplicate="from_state values must be unique",
    )
    if not _is_state_name(to_state):
        raise ValueError("to_state must be a valid state name")
    if on_raise is not None and not _is_state_name(on_raise):
        raise ValueError("on_raise must be a valid state name")
    if on_cancel is not None and not _is_state_name(on_cancel):
        raise ValueError("on_cancel must be a valid state name")

    def decorate(function: Callable[P, R]) -> Callable[P, R]:
        existing = getattr(function, "__codepact_transition__", None)
        declaration = (from_states, to_state, on_raise, on_cancel)
        if existing is not None and existing != declaration:
            raise ValueError("conflicting CodePact transition declarations")
        setattr(function, "__codepact_transition__", declaration)
        return function

    return decorate


def move(value: T) -> T:
    """Mark an explicit ownership transfer and return the original object."""
    return value


__all__ = [
    "resource",
    "Resource",
    "state_machine",
    "transition",
    "State",
    "must_use",
    "discard",
    "discard_outcome",
    "move",
    "MustUseResult",
    "MustHandle",
    "MustHandleOutcome",
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

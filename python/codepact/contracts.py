"""Typing metadata understood by the CodePact source inspector."""

from dataclasses import dataclass
from typing import Annotated, Generic, TypeAlias, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")
S = TypeVar("S")


@dataclass(frozen=True, slots=True)
class Resource:
    """Declared lifecycle metadata; it does not enforce runtime behavior."""

    closed_by: str


@dataclass(frozen=True, slots=True)
class SharedBorrow:
    """Intent to access a value through a shared borrow."""


@dataclass(frozen=True, slots=True)
class OwnedAccess:
    """Intent to access or transfer an owned handle."""


@dataclass(frozen=True, slots=True)
class ExclusiveBorrow:
    """Intent to access a value through an exclusive borrow."""


@dataclass(frozen=True, slots=True)
class StateAccess(Generic[S]):
    """The finite protocol states required or produced by an operation."""


@dataclass(frozen=True, slots=True)
class MustUseResult:
    """The declared result should be handled by its immediate caller."""


@dataclass(frozen=True, slots=True)
class MustHandleOutcome:
    """The declared outcome requires acknowledgment after delivery."""


MustUse: TypeAlias = Annotated[T, MustUseResult()]
MustHandle: TypeAlias = Annotated[T, MustHandleOutcome()]


Owned: TypeAlias = Annotated[T, OwnedAccess()]
Borrowed: TypeAlias = Annotated[T, SharedBorrow()]
BorrowedMut: TypeAlias = Annotated[T, ExclusiveBorrow()]
State = TypeAliasType("State", Annotated[T, StateAccess[S]], type_params=(T, S))

__all__ = [
    "Resource",
    "SharedBorrow",
    "OwnedAccess",
    "ExclusiveBorrow",
    "StateAccess",
    "MustUseResult",
    "MustHandleOutcome",
    "MustUse",
    "MustHandle",
    "Owned",
    "Borrowed",
    "BorrowedMut",
    "State",
]

"""Typing metadata understood by the CodePact source inspector."""

from dataclasses import dataclass
from typing import Annotated, TypeAlias, TypeVar

T = TypeVar("T")


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


Owned: TypeAlias = Annotated[T, OwnedAccess()]
Borrowed: TypeAlias = Annotated[T, SharedBorrow()]
BorrowedMut: TypeAlias = Annotated[T, ExclusiveBorrow()]

__all__ = [
    "Resource",
    "SharedBorrow",
    "OwnedAccess",
    "ExclusiveBorrow",
    "Owned",
    "Borrowed",
    "BorrowedMut",
]

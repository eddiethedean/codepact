"""Explicit Rust-inspired annotation spellings for ordinary Python values."""

from .contracts import Borrowed, BorrowedMut, MustUse, Owned, State

__all__ = ["Owned", "Borrowed", "BorrowedMut", "MustUse", "State"]

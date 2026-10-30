"""Explicit Rust-inspired annotation spellings for ordinary Python values."""

from .contracts import Borrowed, BorrowedMut, Owned

__all__ = ["Owned", "Borrowed", "BorrowedMut"]

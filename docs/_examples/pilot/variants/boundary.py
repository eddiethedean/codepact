from __future__ import annotations

from codepact.rust import Borrowed
from store import ExportSession


def export_receipt(session: Borrowed[ExportSession]) -> None:
    session.fetch_receipt()

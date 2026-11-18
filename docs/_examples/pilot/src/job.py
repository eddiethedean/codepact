from codepact.rust import Borrowed
from store import ExportSession


def export_receipt(session: Borrowed[ExportSession]) -> None:
    session.fetch_receipt()


def run() -> None:
    session = ExportSession()
    session.close()
    export_receipt(session)

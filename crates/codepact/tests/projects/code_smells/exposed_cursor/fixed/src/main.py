from codepact.rust import Borrowed

from cursor import Cursor


def export_handles(view: Borrowed[Cursor]) -> list[object]:
    return [view.read()]


def read_snapshot(view: Borrowed[Cursor]) -> str:
    return view.read()

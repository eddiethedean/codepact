from collections.abc import Callable

from codepact.rust import Borrowed

from cursor import Cursor


def export_handles(view: Borrowed[Cursor]) -> list[object]:
    view.read()
    return [view]


def make_reader(view: Borrowed[Cursor]) -> Callable[[], str]:
    return lambda: view.read()

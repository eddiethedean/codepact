from codepact.rust import Borrowed

from receipt import Receipt

cached: Receipt | None = None


def record(view: Borrowed[Receipt]) -> None:
    global cached
    view.read()
    cached = view

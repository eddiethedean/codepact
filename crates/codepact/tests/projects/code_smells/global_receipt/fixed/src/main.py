from codepact.rust import Borrowed

from receipt import Receipt


def record(view: Borrowed[Receipt]) -> None:
    view.read()

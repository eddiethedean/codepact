from collections.abc import Callable

from codepact import resource
from codepact.rust import BorrowedMut


@resource(closed_by="close")
class Buffer:
    def write(self) -> None:
        pass

    def close(self) -> None:
        pass


def edit(
    view: BorrowedMut[Buffer],
    callback: Callable[[BorrowedMut[Buffer]], None],
) -> None:
    callback(view)
    view.write()


def write_view(view: BorrowedMut[Buffer]) -> None:
    view.write()


def run() -> None:
    item = Buffer()
    edit(item, write_view)
    item.close()

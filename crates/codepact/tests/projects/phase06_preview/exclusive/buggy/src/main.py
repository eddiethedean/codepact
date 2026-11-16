from codepact import resource
from codepact.rust import Borrowed, BorrowedMut


@resource(closed_by="close")
class Buffer:
    def read(self) -> None:
        pass

    def write(self) -> None:
        pass

    def close(self) -> None:
        pass


def edit(view: BorrowedMut[Buffer], alias: Borrowed[Buffer]) -> None:
    view.write()


def run() -> None:
    item = Buffer()
    alias = item
    edit(item, alias)
    item.close()

from codepact import resource
from codepact.rust import BorrowedMut


@resource(closed_by="close")
class Buffer:
    def write(self) -> None:
        pass

    def close(self) -> None:
        pass


def edit(view: BorrowedMut[Buffer]) -> None:
    view.write()


def run() -> None:
    item = Buffer()
    edit(item)
    item.close()

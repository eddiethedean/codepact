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


def edit(view: BorrowedMut[Buffer], observer: Borrowed[Buffer]) -> None:
    view.write()
    observer.read()


def run() -> None:
    item = Buffer()
    other = Buffer()
    edit(item, other)
    item.close()
    other.close()

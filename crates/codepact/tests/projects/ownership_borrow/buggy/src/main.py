from codepact import resource
from codepact.rust import Borrowed


@resource(closed_by="close")
class Reader:
    def read(self) -> str:
        return "record"

    def close(self) -> None:
        pass


def first_record(view: Borrowed[Reader]) -> str:
    saved = [view]
    return saved[0].read()

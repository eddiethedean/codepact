from codepact import resource
from codepact.rust import Borrowed, Owned


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def read(conn: Borrowed[Connection]) -> str:
    return "ready"


def consume(conn: Owned[Connection]) -> None:
    conn.close()

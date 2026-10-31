from typepact import resource
from typepact.rust import Borrowed, Owned


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def read(conn: Borrowed[Connection]) -> str:
    return "ready"


def consume(conn: Owned[Connection]) -> str:
    conn.close()
    return "closed"

"""Small lifecycle example for `typepact check --root examples`."""

from typepact import resource
from typepact.rust import Borrowed


@resource(closed_by="close")
class Connection:
    def status(self) -> str:
        return "ready"

    def close(self) -> None:
        pass


def inspect_connection(conn: Borrowed[Connection]) -> str:
    return conn.status()


conn = Connection()
inspect_connection(conn)
conn.close()
inspect_connection(conn)  # TP1001 at this call; close is the related location.

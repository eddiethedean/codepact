from codepact import resource
from codepact.rust import Borrowed


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

from client import read
from models import Connection


def run() -> None:
    conn = Connection()
    conn.close()
    read(conn)

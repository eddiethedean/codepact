from models import Connection
from typepact.rust import Borrowed


def read(conn: Borrowed[Connection]) -> str:
    return "ready"

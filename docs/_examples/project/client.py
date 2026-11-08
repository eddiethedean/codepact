from models import Connection
from codepact.rust import Borrowed


def read(conn: Borrowed[Connection]) -> str:
    return "ready"

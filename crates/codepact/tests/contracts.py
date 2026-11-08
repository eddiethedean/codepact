"""Paired declarations used by the CodePact 0.1 inspector."""
from typing import Annotated

from codepact import resource
from codepact.contracts import Resource, SharedBorrow
from codepact.rust import Borrowed


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


ConnectionResource = Annotated[Connection, Resource(closed_by="close")]


def inspect_pythonic(conn: Borrowed[Connection]) -> str:
    return "ready"


def inspect_explicit(conn: Annotated[Connection, SharedBorrow()]) -> str:
    return "ready"


def inspect_resource_view(conn: ConnectionResource) -> str:
    return "ready"

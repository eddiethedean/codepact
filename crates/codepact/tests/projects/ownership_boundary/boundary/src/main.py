from asyncio import create_task
from collections.abc import Callable

from codepact import resource
from codepact.rust import Borrowed


@resource(closed_by="close")
class Reader:
    def read(self) -> str:
        return "record"

    def close(self) -> None:
        pass


async def read(view: Borrowed[Reader]) -> None:
    view.read()


async def run(hook: Callable[[Reader], None]) -> None:
    reader = Reader()
    hook(reader)
    create_task(read(reader))

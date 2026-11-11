from asyncio import TaskGroup

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


async def run() -> None:
    reader = Reader()
    try:
        async with TaskGroup() as group:
            group.create_task(read(reader))
            reader.close()
    except Exception:
        raise

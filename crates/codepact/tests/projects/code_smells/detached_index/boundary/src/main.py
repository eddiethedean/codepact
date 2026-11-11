from asyncio import create_task

from codepact import resource
from codepact.rust import Borrowed


@resource(closed_by="close")
class Index:
    def read(self) -> None:
        pass

    def close(self) -> None:
        pass


async def scan(index: Borrowed[Index]) -> None:
    index.read()


async def refresh(index: Borrowed[Index]) -> None:
    create_task(scan(index))

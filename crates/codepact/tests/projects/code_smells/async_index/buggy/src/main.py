from asyncio import TaskGroup

from codepact.rust import Borrowed

from index import Index


async def scan(view: Borrowed[Index]) -> None:
    view.read()


async def cancel_scan() -> None:
    index = Index()
    async with TaskGroup() as group:
        task = group.create_task(scan(index))
        task.cancel()
        index.close()


async def build_index() -> None:
    index = Index()
    async with TaskGroup() as outer:
        outer.create_task(scan(index))
        async with TaskGroup() as inner:
            inner.create_task(scan(index))
        index.close()

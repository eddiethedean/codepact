from asyncio import TaskGroup


async def worker() -> None:
    pass


async def run() -> None:
    async with TaskGroup() as outer:
        async with TaskGroup() as inner:
            inner.create_task(worker())
        outer.create_task(worker())

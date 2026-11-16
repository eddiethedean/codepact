from asyncio import TaskGroup


async def worker() -> None:
    pass


async def run() -> None:
    async with TaskGroup() as group:
        group.create_task(worker())
        group.create_task(worker())

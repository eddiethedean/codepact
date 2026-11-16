from asyncio import TaskGroup
from collections.abc import Callable


async def worker() -> None:
    pass


async def run(factory: Callable[..., object]) -> None:
    async with TaskGroup() as group:
        group.set_task_factory(factory)  # type: ignore[attr-defined]
        task = group.create_task(worker())
    task.cancel()

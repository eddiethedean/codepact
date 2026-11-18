from asyncio import Queue


async def consume() -> None:
    queue = Queue()
    await queue.get()

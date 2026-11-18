from asyncio import Queue


async def consume() -> None:
    queue = Queue()
    queue.put_nowait("receipt")
    await queue.get()
    queue.task_done()
    await queue.join()

from asyncio import Lock
from codepact import resource


@resource(closed_by="close")
class Counter:
    def increment(self) -> None:
        pass

    def close(self) -> None:
        pass


async def update() -> None:
    lock = Lock()
    counter = Counter()
    try:
        async with lock:
            counter.increment()
    finally:
        counter.close()

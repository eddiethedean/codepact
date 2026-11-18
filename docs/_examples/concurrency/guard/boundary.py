from asyncio import Lock
from codepact import resource


@resource(closed_by="close")
class Counter:
    def increment(self) -> None:
        pass

    def close(self) -> None:
        pass


async def update(select_other: bool) -> None:
    lock = Lock()
    counter = Counter()
    other = Counter()
    alias = counter
    if select_other:
        alias = other
    try:
        async with lock:
            alias.increment()
    finally:
        counter.close()
        other.close()

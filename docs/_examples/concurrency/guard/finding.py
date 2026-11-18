from asyncio import Lock
from codepact import resource


@resource(closed_by="close")
class Counter:
    def increment(self) -> None:
        pass

    def close(self) -> None:
        pass


async def update() -> None:
    lock = Lock()  # noqa: F841 - the configured lock is deliberately unheld
    counter = Counter()
    try:
        counter.increment()
    finally:
        counter.close()

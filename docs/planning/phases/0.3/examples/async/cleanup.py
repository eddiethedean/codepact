from contextlib import aclosing

from codepact import resource


@resource(closed_by="aclose")
class AsyncConnection:
    async def aclose(self):
        pass


async def direct_cleanup() -> None:
    connection = AsyncConnection()
    await connection.aclose()


async def context_cleanup() -> None:
    connection = AsyncConnection()
    async with aclosing(connection):
        pass

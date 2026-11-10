import asyncio
from codepact import resource
from codepact.rust import Owned


@resource(closed_by="close")
class Channel:
    def close(self) -> None:
        pass


async def finish(channel: Owned[Channel]) -> None:
    channel.close()


def launch() -> None:
    channel = Channel()
    pending = finish(channel)
    asyncio.create_task(pending)

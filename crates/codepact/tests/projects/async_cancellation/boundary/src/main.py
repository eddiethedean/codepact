from codepact import resource


@resource(closed_by="aclose")
class Channel:
    async def aclose(self) -> None:
        pass


async def stop() -> None:
    channel = Channel()
    await channel.aclose()

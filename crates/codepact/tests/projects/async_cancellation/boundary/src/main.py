from codepact import resource

@resource(closed_by="aclose")
class Channel:
    async def aclose(self):
        pass

async def stop():
    channel = Channel()
    await channel.aclose()

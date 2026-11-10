import asyncio
from codepact import resource
from codepact.rust import Owned

@resource(closed_by="close")
class Channel:
    def close(self):
        pass

async def finish(channel: Owned[Channel]):
    channel.close()

def launch():
    channel = Channel()
    pending = finish(channel)
    asyncio.create_task(pending)

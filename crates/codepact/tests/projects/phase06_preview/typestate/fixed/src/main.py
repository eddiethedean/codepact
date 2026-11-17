from facade import Channel


def run() -> None:
    channel = Channel()
    try:
        channel.disconnect()
    except RuntimeError:
        channel.send()

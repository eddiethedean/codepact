from typing import Literal

from codepact import state_machine
from codepact.rust import State


@state_machine(states=("ready", "closed"), initial="ready")
class Channel:
    def send(self: State["Channel", Literal["ready"]]) -> None:
        pass


def run() -> None:
    channel = Channel()
    channel.send()

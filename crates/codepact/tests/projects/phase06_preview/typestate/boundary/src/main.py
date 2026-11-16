from typing import Literal, Protocol

from codepact import state_machine
from codepact.rust import State


@state_machine(states=("ready", "closed"), initial="ready")
class Channel:
    def send(self: State["Channel", Literal["ready"]]) -> None:
        pass


class Effect(Protocol):
    def __call__(self, channel: Channel) -> None: ...


def run(unknown_effect: Effect) -> None:
    channel = Channel()
    unknown_effect(channel)
    channel.send()

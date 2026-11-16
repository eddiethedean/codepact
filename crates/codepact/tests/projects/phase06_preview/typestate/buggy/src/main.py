from typing import Literal

from codepact import state_machine, transition
from codepact.rust import State


@state_machine(states=("ready", "closed"), initial="ready")
class Channel:
    @transition(from_state="ready", to_state="closed", on_raise="closed")
    def disconnect(self) -> None:
        raise RuntimeError()

    def send(self: State["Channel", Literal["ready"]]) -> None:
        pass


def run() -> None:
    channel = Channel()
    try:
        channel.disconnect()
    except RuntimeError:
        channel.send()

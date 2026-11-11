from typing import Annotated

from codepact import move
from codepact.contracts import OwnedAccess

from buffer import Buffer, finish


def hand_off(buffer: Annotated[Buffer, OwnedAccess()]) -> None:
    finish(move(buffer))

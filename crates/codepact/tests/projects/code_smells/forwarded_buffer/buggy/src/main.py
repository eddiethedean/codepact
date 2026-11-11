from typing import Annotated

from codepact.contracts import OwnedAccess

from buffer import Buffer, finish


def hand_off(buffer: Annotated[Buffer, OwnedAccess()]) -> None:
    finish(buffer)

from typing import Annotated

from codepact import must_use
from codepact.contracts import MustUseResult
from codepact.rust import MustUse


@must_use
def confirm_address() -> bool:
    return False


def reserve_slot() -> MustUse[bool]:
    return False


def can_dispatch() -> Annotated[bool, MustUseResult()]:
    return False

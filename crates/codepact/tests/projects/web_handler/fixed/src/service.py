from typing import Annotated

from codepact import resource
from codepact.contracts import MustUseResult


@resource(closed_by="release")
class Session:
    def query(self) -> int:
        return 200

    def release(self) -> None:
        pass


def authorization_decision() -> Annotated[bool, MustUseResult()]:
    return True

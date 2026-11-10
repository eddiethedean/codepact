from codepact import resource

@resource(closed_by="release")
class Session:
    def query(self):
        return 200

    def release(self):
        pass

from typing import Annotated
from codepact.contracts import MustUseResult

def authorization_decision() -> Annotated[bool, MustUseResult()]:
    return True

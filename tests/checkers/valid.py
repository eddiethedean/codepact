from typing import Annotated, Literal, assert_type

from codepact import discard, move, must_use, state_machine, transition
from codepact.contracts import Borrowed, MustUse, Owned, StateAccess
from codepact.rust import Borrowed as ExplicitBorrowed
from codepact.rust import MustUse as ExplicitMustUse
from codepact.rust import State


class Connection:
    pass


@state_machine(states=("ready", "closed"), initial="ready")
class Channel:
    @transition(from_state="ready", to_state="closed")
    def close(self) -> None:
        pass

    def send(self: State["Channel", Literal["ready"]]) -> None:
        pass


@must_use
def fetch(name: str) -> Connection:
    return Connection()


def fetch_explicit() -> MustUse[Connection]:
    return Connection()


borrowed: Borrowed[Connection] = Connection()
owned: Owned[Connection] = Connection()
explicit: ExplicitBorrowed[Connection] = Connection()
assert_type(borrowed, Connection)
assert_type(owned, Connection)
assert_type(explicit, Connection)
assert_type(fetch("primary"), Connection)
assert_type(fetch_explicit(), Connection)
assert_type(discard(fetch("discard")), None)
must_use_alias: ExplicitMustUse[Connection] = Connection()
assert_type(must_use_alias, Connection)
assert_type(move(owned), Connection)

state_alias: State[Channel, Literal["ready"]] = Channel()
explicit_state: Annotated[Channel, StateAccess[Literal["ready"]]] = Channel()
assert_type(state_alias, Channel)
assert_type(explicit_state, Channel)

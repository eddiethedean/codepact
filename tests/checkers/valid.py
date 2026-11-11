from typing import assert_type

from codepact import discard, move, must_use
from codepact.contracts import Borrowed, MustUse, Owned
from codepact.rust import Borrowed as ExplicitBorrowed
from codepact.rust import MustUse as ExplicitMustUse


class Connection:
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

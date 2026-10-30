from typing import assert_type

from typepact.contracts import Borrowed, Owned
from typepact.rust import Borrowed as ExplicitBorrowed


class Connection:
    pass


borrowed: Borrowed[Connection] = Connection()
owned: Owned[Connection] = Connection()
explicit: ExplicitBorrowed[Connection] = Connection()
assert_type(borrowed, Connection)
assert_type(owned, Connection)
assert_type(explicit, Connection)

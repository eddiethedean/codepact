# Declaring resource contracts

CodePact's Python helpers provide declarations that ordinary Python type checkers can understand. They preserve the class and underlying parameter type. The inspector reads their syntax and converts recognized declarations into contract facts.

## Declare a resource lifecycle

Use `@resource` with a literal `closed_by` method name:

```python
from codepact import resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        ...
```

At runtime, the decorator attaches a `Resource(closed_by="close")` value to the class as `__codepact_resource__` and returns the same class. It checks that the name is a valid Python identifier. This metadata does not inspect or invoke `close`.

The source inspector recognizes the decorator only when it resolves to `codepact.resource` and the argument is a literal string. Dynamic expressions such as `@resource(closed_by=DEFAULT_CLOSE)` are outside the supported subset.

## Declare access intent

Import one of the aliases from `codepact.rust`:

```python
from codepact.rust import Borrowed, BorrowedMut, Owned


def inspect(conn: Borrowed[Connection]) -> str:
    ...


def consume(conn: Owned[Connection]) -> str:
    ...


def update(conn: BorrowedMut[Connection]) -> str:
    return "updated"
```

| Alias | Metadata | Inspector access value | Intent |
| --- | --- | --- | --- |
| `Borrowed[T]` | `SharedBorrow()` | `shared` | Shared access |
| `Owned[T]` | `OwnedAccess()` | `owned` | Owned access or transfer |
| `BorrowedMut[T]` | `ExclusiveBorrow()` | `exclusive` | Exclusive access |

These aliases are `typing.Annotated` aliases. The underlying Python type remains `T`; the access marker is metadata for CodePact. Type checkers do not treat these spellings as Rust ownership or borrowing rules.

The metadata classes and aliases are also exported from `codepact.contracts`. Submodule imports work too: `from codepact import rust as r` allows `r.Borrowed[Connection]`, and `from codepact import contracts as ct` allows `ct.Resource(closed_by="close")` in `Annotated`. Rebinding either imported name removes that API identity.

## Mark a must-use return (0.4 candidate)

The local 0.4 CLI candidate recognizes synchronous function and method results
declared in any of these equivalent forms:

```python
from typing import Annotated
from codepact import discard, must_use
from codepact.contracts import MustUseResult
from codepact.rust import MustUse

@must_use
def fetch() -> int: ...

def fetch_typed() -> MustUse[int]: ...
def fetch_verbose() -> Annotated[int, MustUseResult()]: ...

fetch()                 # TP3001 in a check
value = fetch()         # immediate handling, no later-use proof
discard(fetch())        # explicit intentional abandonment
```

`must_use` is a ParamSpec-typed identity decorator: it attaches a marker and
returns the same callable without wrapping it. `MustUse[T]` is an
`Annotated[T, MustUseResult()]` alias. `discard(value)` evaluates and returns
`None`; it does not close, transfer, suppress exceptions, or import the
analyzer. These helper APIs are typing metadata/markers, not runtime resource
semantics. A user-defined lookalike has no CodePact meaning.

The source checker accepts only resolved synchronous return contracts.
Assignment, return/forwarding and direct argument passing count as immediate
handling, not eventual consumption. Async/dynamic consumption and unsupported
nested expressions remain visible coverage boundaries. Misplaced known
metadata is a declaration error. See the [rule catalog](rules.md#tp3001) and
[adoption workflow](adopting.md); the 0.4 candidate has not been pilot-qualified.

## Use `Annotated` directly

You can spell the metadata explicitly with `typing.Annotated`:

```python
from typing import Annotated

from codepact.contracts import Resource, SharedBorrow


ConnectionView = Annotated[
    Connection,
    Resource(closed_by="close"),
    SharedBorrow(),
]


def inspect(conn: ConnectionView) -> str:
    ...
```

Metadata on an alias describes that alias. It does not mutate the underlying class or add lifecycle metadata to all references to the class. The inspector combines compatible class and annotation metadata. Conflicting access or lifecycle declarations produce an error diagnostic and incomplete coverage.

## What the declarations mean

The resource helper decorator has a small runtime effect: it stores metadata on the class. Parameter and return annotations remain ordinary Python typing metadata. The analyzer does not verify method bodies, ownership transfer, exclusivity, cleanup, or use-after-close.

All inspected facts are marked `declared-unverified`. CodePact reports what the source declares; it does not prove that the implementation honors it. See [output](output.md) and [current limitations](limitations.md).

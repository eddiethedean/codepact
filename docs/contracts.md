# Declaring resource contracts

TypePact's Python helpers provide declarations that ordinary Python type checkers can understand. They preserve the class and underlying parameter type. The inspector reads their syntax and converts recognized declarations into contract facts.

## Declare a resource lifecycle

Use `@resource` with a literal `closed_by` method name:

```python
from typepact import resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        ...
```

At runtime, the decorator attaches a `Resource(closed_by="close")` value to the class as `__typepact_resource__` and returns the same class. It checks that the name is a valid Python identifier. This metadata does not inspect or invoke `close`.

The source inspector recognizes the decorator only when it resolves to `typepact.resource` and the argument is a literal string. Dynamic expressions such as `@resource(closed_by=DEFAULT_CLOSE)` are outside the supported subset.

## Declare access intent

Import one of the aliases from `typepact.rust`:

```python
from typepact.rust import Borrowed, BorrowedMut, Owned


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

These aliases are `typing.Annotated` aliases. The underlying Python type remains `T`; the access marker is metadata for TypePact. Type checkers do not treat these spellings as Rust ownership or borrowing rules.

The metadata classes and aliases are also exported from `typepact.contracts`.

## Use `Annotated` directly

You can spell the metadata explicitly with `typing.Annotated`:

```python
from typing import Annotated

from typepact.contracts import Resource, SharedBorrow


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

The helper decorator has a small runtime effect: it stores metadata on the class. Parameter and return annotations remain ordinary Python typing metadata. The analyzer does not verify method bodies, ownership transfer, exclusivity, cleanup, or use-after-close in 0.1.

All inspected facts are marked `declared-unverified`. TypePact reports what the source declares; it does not prove that the implementation honors it. See [output](output.md) and [current limitations](limitations.md).

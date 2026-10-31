# Quick start

This example declares that a `Connection` is closed by `close`, then describes how two functions intend to access it.

Save the code as `connection.py`:

```python
from typepact import resource
from typepact.rust import Borrowed, Owned


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def read(conn: Borrowed[Connection]) -> str:
    return "ready"


def consume(conn: Owned[Connection]) -> str:
    conn.close()
    return "closed"
```

Inspect the declarations:

```sh
uv run typepact inspect-contracts connection.py
```

The JSON result contains a resource contract for `Connection` and access contracts for `read.conn` and `consume.conn`. The first parameter is shared; the second is owned. Both inherit the class lifecycle metadata.

The source is parsed without importing `connection.py`. TypePact records the declared method name, but this release does not check whether `close()` is called on every path or whether the method actually releases a resource.

## Inspect multiple modules

Pass all source files that should participate in direct import resolution. Use `--root` to derive package-qualified module names:

```sh
uv run typepact inspect-contracts --root src src/models.py src/client.py
```

The root is used to name modules; TypePact does not discover files or follow imports automatically. For the full option and input behavior, see the [CLI reference](cli.md). For the declarations understood by 0.1, see [supported syntax](supported-syntax.md).

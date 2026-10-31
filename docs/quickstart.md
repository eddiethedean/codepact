# Quick start

This example declares that a `Connection` is closed by `close`, then describes how two functions intend to access it.

From a Python project, install the helpers and command with `uv add typepact==0.1.0`. See [installation](install.md) for standalone CLI use and platform requirements.

Save the code as `connection.py`:

```{literalinclude} _examples/connection.py
:language: python
```

Inspect the declarations:

```sh
uv run typepact inspect-contracts connection.py
```

A successful run exits with status `0`, has an empty `diagnostics` list, and reports `coverage.status` as `complete-for-supported-subset`.

The JSON result contains a resource contract for `Connection` and access contracts for `read.conn` and `consume.conn`. The first parameter is shared; the second is owned. Both inherit the class lifecycle metadata.

The source is parsed without importing `connection.py`. TypePact records the declared method name, but this release does not check whether `close()` is called on every path or whether the method actually releases a resource.

## Read the result

| Declaration | Lifecycle | Access |
| --- | --- | --- |
| `Connection` | Closed by `close` | `unspecified` |
| `read.conn` | Closed by `close` | `shared` |
| `consume.conn` | Closed by `close` | `owned` |

All three facts refer to `connection.Connection`. The `source` field locates each declaration, and `provenance` records how its metadata was declared. See the [full output](output.md) for the corresponding JSON.

## Inspect multiple modules

Pass all source files that should participate in direct import resolution. Use `--root` to derive package-qualified module names:

```sh
uv run typepact inspect-contracts --root src src/models.py src/client.py
```

The root is used to name modules; TypePact does not discover files or follow imports automatically. For the full option and input behavior, see the [CLI reference](cli.md). For the declarations understood by 0.1, see [supported syntax](supported-syntax.md).

Continue with the [multi-file tutorial](project-guide.md) to resolve declarations across modules, or [automation](automation.md) to check an explicit source set in CI.

## Try lifecycle checking next

The source checkout includes an unreleased `typepact check` preview for direct
construction, local aliases, normal synchronous calls, and `if`/`else` joins.
The published 0.1.0 packages used above provide inspection only.

Follow the [lifecycle tutorial](lifecycle.md) to build the preview, reproduce a
use after close, read its related close location, and make the example pass.
Possible findings and coverage warnings can accompany exit `0`; the command does
not prove cleanup on every exit or general ownership safety.

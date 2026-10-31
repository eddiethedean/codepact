# Inspect a project with multiple files

TypePact can carry resource metadata from a class in one supplied module to an annotation in another. This tutorial uses absolute imports and explicit inputs, which are supported in 0.1.

## Create the source files

Start with this directory layout:

```text
my-project/
├── pyproject.toml
└── src/
    ├── models.py
    └── client.py
```

Save the resource declaration as `src/models.py`:

```{literalinclude} _examples/project/models.py
:language: python
```

Save its consumer as `src/client.py`:

```{literalinclude} _examples/project/client.py
:language: python
```

## Supply both modules

From `my-project`, install TypePact and inspect the two files:

```sh
uv add typepact==0.1.0
uv run typepact inspect-contracts --root src src/models.py src/client.py
```

The root makes the files' module names `models` and `client`. TypePact resolves `from models import Connection` using the supplied `models.py`, then associates `read.conn` with `models.Connection` and its `close` lifecycle.

| Declaration | Base symbol | Lifecycle | Access |
| --- | --- | --- | --- |
| `Connection` | `models.Connection` | `close` | `unspecified` |
| `read.conn` | `models.Connection` | `close` | `shared` |

The result has no diagnostics and complete coverage for this supported subset. Input order does not change import resolution.

## Map package paths to imports

If the files instead live at `src/my_app/models.py` and `src/my_app/client.py`, keep `--root src` and use `from my_app.models import Connection`. Pass the relocated paths explicitly. The normalized resource symbol becomes `my_app.models.Connection`.

Choose the directory above your top-level package as the root. File arguments are resolved from the current working directory, so `--root src` still requires arguments such as `src/my_app/models.py`.

## Interpret missing inputs

Supplying only `client.py` leaves the imported class unresolved. The inspector reports incomplete coverage instead of importing your application or searching installed packages. Add the provider file to the invocation when its declaration belongs to the supported subset.

Relative imports, re-export chains, and import cycles remain outside 0.1. See [limitations](limitations.md) before expanding the input list to an entire application, and [automation](automation.md) to use a chosen source set in CI.

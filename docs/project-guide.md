# Check a project with multiple files

Continue in the [quick-start project](quickstart.md), with the repaired
`src/connection.py`. Add the three files below to the same `src/` directory.

| File | Role |
| --- | --- |
| {download}`src/models.py <_examples/project/models.py>` | Declares a connection and its closing method. |
| {download}`src/client.py <_examples/project/client.py>` | Declares a read that borrows the connection. |
| {download}`src/app.py <_examples/project/app.py>` | Calls the borrowed read after closing the connection. |

```{literalinclude} _examples/project/app.py
:language: python
:linenos:
```

## Check the project and repair the call

Run the checker from the project directory.

<!-- smoke: project-finding -->
```sh
uv run --locked codepact check --root src --mode strict
```

Expect `TP1001` at `app.py:8:5`, a related close at `app.py:7:5`, and
exit `1`. The checker resolves `Connection` from `models.py` and the
borrowed parameter from `client.py`. Move `read(conn)` before
`conn.close()` and rerun the same command. Expect exit `0`.

## Save the project policy

Add the following table to the project's `pyproject.toml`.

```{literalinclude} _examples/project/codepact.toml
:language: toml
```

Show the resolved policy from the project directory.

<!-- smoke: project-settings -->
```sh
uv run --locked codepact check --show-settings --json
```

The resolved settings show `src` as the effective root, Python 3.11 as the
source target, and strict mode. [Run the same check in CI](automation.md).

## Map package paths to imports

For `src/my_app/models.py`, use an absolute import such as
`from my_app.models import Connection` and keep `src` as the root.
CodePact gives that file the module name `my_app.models`. Check that the
provider is under the root and matches the include/exclude policy.

Start with modules that use direct absolute imports and recognized
annotations. Relative imports, postponed or string annotations, and dynamic
dispatch can leave coverage incomplete. Read `coverage.reasons` when a
provider or call fails to resolve. [Supported syntax](supported-syntax.md) ·
[Configuration lookup](cli.md#project-configuration-and-scope)

## Inspect just the declarations

Run declaration inspection from the project directory.

<!-- smoke: project-inspection -->
```sh
uv run --locked codepact inspect-contracts --root src src/models.py src/client.py
```

The command emits schema 0 JSON and exits `0`. It shows the closing method
on `models.Connection` and shared access on `read.conn`. Supplying only
`src/client.py` leaves its resource unresolved. File arguments are relative
to the working directory and inspection reads only the named files.

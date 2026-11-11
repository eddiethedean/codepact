# Quick start

Run a check with the published CodePact 0.5.0 CLI, read a definite
use-after-close finding, and repair the example.

## Install in your project

In an existing uv project, install the helper package and its matching CLI.

```sh
uv add codepact==0.5.0
uv run --locked codepact --version
mkdir -p demo
```

The version command prints `codepact 0.5.0`.
[Standalone CLI, pip, and Rust installs](install.md)

## Declare a resource

Save the [complete example](https://github.com/eddiethedean/codepact/blob/main/examples/lifecycle.py)
as `demo/connection.py`.

```{literalinclude} ../examples/lifecycle.py
:language: python
:linenos:
:emphasize-lines: 22,23
```

`@resource` declares the close operation. `Borrowed[Connection]` gives
`inspect_connection` shared access to an open connection. The underlying
Python type remains `Connection` for mypy and Pyright.
[Contract declarations](contracts.md)

## Run your first check

```sh
uv run --locked codepact check --root demo
```

```text
connection.py:23:1: error TP1001: Borrowed call requires an open resource
  certainty: definite
  trust: declared-unverified
  repair: Keep the use before close, or guard it with a lifecycle check.
  related: connection.py:22:1
1 diagnostics; 1 actionable; 0 suppressed; 0 rule-disabled; 1 returned
```

The primary location is the call on line 23. The related location points to
the close on line 22. The check exits `1`. Delete the call after the close and
rerun the same command. The repaired example exits `0` with complete coverage
for the supported subset.

The close method is a stub. CodePact checks the declared transition, while
the implementation is responsible for runtime cleanup.
[Lifecycle, aliases, and branches](lifecycle.md)

```{tip}
Use a narrow root for early checks. `check` discovers Python files below the
root. Supplying a filename adds an input to discovery.
```

## Inspect the contract facts

```sh
uv run --locked codepact inspect-contracts demo/connection.py
```

The inspector emits schema 0 JSON with two contract views for this example.

| Declaration | Base symbol | Lifecycle | Access |
| --- | --- | --- | --- |
| `Connection` | `connection.Connection` | Closed by `close` | `unspecified` |
| `inspect_connection.conn` | `connection.Connection` | Closed by `close` | `shared` |

The inspector returns those facts and exits `1` because the example contains
module-level executable statements outside declaration inspection. Read
`coverage.reasons` to see those boundaries. Use `check` for diagnostics that
depend on control flow and lifecycle state.
[Output reference](output.md)

## Continue with a project

- [Check multiple modules](project-guide.md) to include a resource provider
  and its consumer.
- [Set a project policy](adopting.md) to select source roots and decide how
  findings affect the exit status.
- [Capture reports in CI](ci-reporting.md) to preserve evidence on both
  passing and failing runs.

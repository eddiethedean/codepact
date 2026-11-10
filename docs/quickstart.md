# Quick start

Run a first check using CodePact 0.4.0, then inspect the contracts behind its
result. Continue with project policy, baselines, and must-use checks in the
[adoption guide](adopting.md).

## Install in your project

In an existing uv project:

```sh
uv add codepact==0.4.0
uv run --locked codepact --version
mkdir -p demo
```

The helper package installs the matching CLI. The command prints
`codepact 0.4.0`. See [installation](install.md) for standalone CLI and source builds.

## Declare a resource

Save this complete example as `demo/connection.py`:

```{literalinclude} _examples/connection.py
:language: python
```

`@resource` declares the close operation. `Borrowed[Connection]` describes a
shared view; `Owned[Connection]` assigns cleanup responsibility to the consumer.
The ordinary Python type remains `Connection` for mypy and Pyright.

## Run your first check

```sh
uv run --locked codepact check --root demo --json
```

This example has no lifecycle violations and complete coverage for the
supported subset; it exits `0`. CodePact reads the file without importing it.
The empty `close` method is a stub: the declared transition is modeled, while
actual runtime cleanup remains the implementation’s responsibility.

Use a narrow root: `check` discovers Python files below `demo`. Supplying a
filename adds an input and does not limit discovery to that file.

## Inspect the contract facts

```sh
uv run --locked codepact inspect-contracts demo/connection.py
```

Inspection emits schema 0 JSON with these views:

| Declaration | Base symbol | Lifecycle | Access |
| --- | --- | --- | --- |
| `Connection` | `connection.Connection` | Closed by `close` | `unspecified` |
| `read.conn` | `connection.Connection` | Closed by `close` | `shared` |
| `consume.conn` | `connection.Connection` | Closed by `close` | `owned` |

The result has an empty `diagnostics` list and
`coverage.status: complete-for-supported-subset`. Each view includes its source
span and declaration provenance. Inspection describes declarations; `check`
analyzes supported flow. See the [output reference](output.md).

The inspector also rejects inconsistent metadata. A view that declares
`Resource(closed_by="release")` for this class conflicts with its `close`
lifecycle and produces `TP4002` with both source locations. See the
[complete conflicting-declaration example](rules.md#example-conflicting-lifecycle-metadata).

## Continue with a failing example

Follow the [lifecycle tutorial](lifecycle.md) to reproduce a definite use after
close, read the diagnostic, and make the example pass. Use the
[multi-file tutorial](project-guide.md) when resource providers and consumers
live in different modules, or [automation](automation.md) to check them in CI.

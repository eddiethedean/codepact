# Check a resource lifecycle

The historical TypePact 0.2.0 release introduced its `typepact check` command
for a bounded subset of normal synchronous Python execution. CodePact 0.3.0
publishes the renamed `codepact check` command and adds cleanup obligations
across supported exits.

## Install and run

Install the matching helper and CLI packages in your project:

```sh
uv add codepact==0.3.0
```

The analyzer reads Python source as text; it does not import the example.

## Reproduce a use after close

Create a `demo` directory in the checkout and save this example as
`demo/connection.py`:

```{literalinclude} ../examples/lifecycle.py
:language: python
```

Run the checker from the checkout root:

```sh
uv run codepact check --root demo
```

The first `inspect_connection(conn)` runs while the connection is open. After
`conn.close()`, the final call requires a resource that is already closed. The
checker reports:

```text
connection.py:23:1: error TP1001: use of a resource that may be closed
  related: connection.py:22:1
```

This example intentionally exits `1`. Remove the final call and run the command
again: the supported example exits `0` without diagnostics and has complete
coverage. The empty `close` body illustrates a declared model: `closed_by="close"`
supplies the normal transition to closed. It does not prove runtime cleanup.

## Understand aliases and branches

An exact alias refers to the same modeled object. Closing `alias` below also
invalidates `conn`:

```python
conn = Connection()
alias = conn
alias.close()
conn.status()  # Definite TP1001.
```

If a supported branch closes a resource on only one path, a later use can produce
a **possible** TP1001. Separate constructions have separate state. Unknown
effects and unsupported expressions can prevent the checker from establishing
which state reaches a use.

A `Borrowed[Connection]` parameter describes shared access. Closing that parameter
directly, through a local alias, or through a supported helper produces TP2001.
It does not provide a general lifetime or escape proof.

## Read the whole report

| Result | Command exit | Action |
| --- | --- | --- |
| Definite lifecycle violation or declaration error | `1` | Review the primary operation and related locations |
| Possible violation | `0` unless an error also exists | Inspect the branch or alias alternatives |
| Unknown effect or unsupported region | `0` unless an error also exists | Read the coverage reasons and determine what was left unchecked |
| Invalid invocation or file input | `2` | Correct the command or input paths |

Readable output is the default. For the provisional structured report:

```sh
uv run codepact check --root demo --json
```

Read both `diagnostics` and `coverage`. A successful exit can include possible
findings or incomplete analysis. The historical TypePact 0.2.0 checker used
`assurance: "lifecycle-checking-for-supported-normal-flow"`; CodePact 0.3.0
uses `lifecycle-checking-for-supported-exit-flow`. Declaration provenance
remains `declared-unverified`. CodePact 0.3.0 bounds loop fixed-point analysis
to 16 passes, finite tuple loops to 16 items, and pending coroutine records to
128 per flow state. See the
[output reference](output.md).

## Choose a narrow project root

`check` scans `.py` files below its root. An explicit filename adds an input; it
does not restrict discovery to that file. Keep unrelated examples, generated
files, and unsupported code outside the selected root. The default root is the
current directory. The scan skips symlinks and common build and environment
directories; see the [CLI reference](cli.md).

These limits describe the historical TypePact 0.2.0 checker. Its released
behavior did not model loops, exception handlers, `finally`, context managers,
async suspension, fields and containers, or returned resource aliases
precisely. CodePact 0.3.0 extends bounded exit flow and cleanup;
exact finite tuple loops,
resolved local close bodies, and directly awaited coroutine calls are included
in the released subset. Its remaining boundaries are listed in the [0.3
specification](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/specification.md).
Recursive calls still recover to an unknown summary. A clean result does not
establish leak freedom, cleanup on every exit, ownership safety, or correctness
of declared resource models. Read the [limitations](limitations.md) and
[supported syntax](supported-syntax.md) before using released results in a workflow.

Phase 0.2 qualification and publication evidence are in the
[phase plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.2-lifecycle.md)
and [release record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.2.0-evidence.md).

# Check a resource lifecycle

Use CodePact 0.5.1 to reproduce a use after close, read its evidence, and
make the example pass. [Configure check policy](adopting.md)

## Install and run

Install the matching helper and CLI packages in your project:

```sh
uv add codepact==0.5.1
```

The analyzer reads Python source as text; it does not import the example.

## Reproduce a use after close

Create a `src` directory if needed and save this example as
`src/connection.py`:

```{literalinclude} ../examples/lifecycle.py
:language: python
```

Run the checker from your project root:

```sh
uv run --locked codepact check --root src
```

The first `inspect_connection(conn)` runs while the connection is open. After
`conn.close()`, the final call requires a resource that is already closed. The
checker reports:

```text
connection.py:23:1: error TP1001: Borrowed call requires an open resource
  certainty: definite
  trust: declared-unverified
  repair: Keep the use before close, or guard it with a lifecycle check.
  related: connection.py:22:1
1 diagnostics; 1 actionable; 0 suppressed; 0 rule-disabled; 1 returned
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

The following exits describe the default advisory policy. Strict mode
additionally fails on unsuppressed possible violations and incomplete
coverage. [Policy and exits](cli.md#policy-and-exits)

| Result | Command exit | Action |
| --- | --- | --- |
| Definite lifecycle violation or declaration error | `1` | Review the primary operation and related locations |
| Possible violation | `0` unless an error also exists | Inspect the branch or alias alternatives |
| Unknown effect or unsupported region | `0` unless an error also exists | Read the coverage reasons and determine what was left unchecked |
| Invalid invocation or file input | `2` | Correct the command or input paths |

Readable output is the default. For the provisional structured report:

```sh
uv run --locked codepact check --root src --json
```

Read both `diagnostics` and `coverage`. A successful exit can include possible
findings or incomplete analysis. The default CLI report uses checker
schema 2 and the assurance value
`lifecycle-and-must-use-checking-for-supported-exit-flow`. Declaration
provenance remains `declared-unverified`. [Output reference](output.md)

## Choose a narrow project root

`check` scans `.py` files below its root. An explicit filename adds an input; it
does not restrict discovery to that file. Keep unrelated examples, generated
files, and unsupported code outside the selected root. The default root is the
current directory. The scan skips symlinks and common build and environment
directories; see the [CLI reference](cli.md).

The 0.3 checker also models bounded cleanup duties across exceptional exits,
`finally`, loops, supported context managers, and direct coroutine awaits.
Continue with the [cleanup tutorial](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/cleanup-tutorial.md)
for `TP1002` and ownership handoffs. Recursive calls, dynamic dispatch, arbitrary
containers, and unsupported concurrency can leave coverage incomplete. Read
[limitations](limitations.md) and [supported syntax](supported-syntax.md) before
using results in a required build gate.

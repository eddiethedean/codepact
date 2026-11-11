# Bugs CodePact catches before runtime

The examples below demonstrate the published default checking scope. The 0.5
ownership examples cover moves and outstanding task borrows separately.
[Experimental ownership](ownership.md) · [Synthetic corpus](synthetic-projects.md)

CodePact checks lifecycle and result-use contracts that ordinary Python type
checking and linting do not generally model. Its annotations preserve the
underlying Python type: `Borrowed[T]` and `Owned[T]` are both still `T` to a
conventional type checker. Likewise, a `@must_use` marker is metadata, not a
Python type error. CodePact reads those declarations together with supported
function bodies to find state and contract mistakes.

The excerpts below come from the synthetic corpus. The corpus is deliberately
kept clean under this repository's Ruff rules and strict Pyright checks, so
these examples isolate the contract errors instead of relying on ordinary
unused-variable, syntax, or annotation warnings. Other tools may catch related
problems when configured with specialized rules or project-specific models.
The snippets describe possible runtime failure modes for real resources; the
fixture methods are small stubs and are not executed by the analyzer.

## Use after close: TP1001

`CsvFile` declares `close` as its closing operation. This function reads the
same object after that operation:

```{literalinclude} ../crates/codepact/tests/projects/csv_importer/buggy/src/main.py
:language: python
:start-at: def preview_closed
:end-before: def return_closed_input
```

CodePact reports a definite TP1001 and relates the read to the close. A real
file, socket, cursor, or client object may reject that operation or return an
invalid result after release. The repaired variant reads first and then closes
the resource in
[`csv_importer/fixed`](https://github.com/eddiethedean/codepact/tree/v0.5.0/crates/codepact/tests/projects/csv_importer/fixed).

## Missing cleanup on an exit: TP1002

Cleanup duties apply to supported function exits, including exceptional exits.
Here a writer is used and then the function raises before calling `finish`:

```{literalinclude} ../crates/codepact/tests/projects/report_exporter/buggy/src/main.py
:language: python
:start-at: def interrupted_export
:end-before: def write_after_finalizing
```

CodePact reports the outstanding duty at the exceptional exit. If the writer
wraps a file, transaction, lock, or remote job, skipping its cleanup can leak
capacity or leave partial work behind. A normal type check verifies the call
and exception types; it does not ordinarily require cleanup on every path. The
repaired fixture puts cleanup in `finally` and is linked from
[`report_exporter/fixed`](https://github.com/eddiethedean/codepact/tree/v0.5.0/crates/codepact/tests/projects/report_exporter/fixed).

The same rule covers supported async function bodies before their first
suspension. This worker acknowledges a link but exits without closing it:

```{literalinclude} ../crates/codepact/tests/projects/async_worker/buggy/src/main.py
:language: python
:start-at: async def forget_link
:end-before: async def acknowledge_closed_link
```

This finding concerns the coroutine body when it runs. Scheduling and
cancellation have separate limits described under
[async and other analysis boundaries](#async-and-other-analysis-boundaries).

## Invalid ownership handoff: TP1003

`Borrowed[Token]` and `Owned[Token]` both retain `Token` as their underlying
Python type. The annotation difference communicates a CodePact contract, but
ordinary assignment compatibility does not enforce the transfer:

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/main.py
:language: python
:start-at: def steal_borrowed_token
:end-before: def ignored_configuration
```

The function promises to return an owned token while returning a caller-owned
borrow. A caller could then release an object it does not own or keep using an
object whose owner has released it. CodePact reports TP1003 at the handoff.
The repaired fixture consumes the borrow without claiming ownership in
[`plugin_runner/fixed`](https://github.com/eddiethedean/codepact/tree/v0.5.0/crates/codepact/tests/projects/plugin_runner/fixed).

## Closing a borrowed resource: TP2001

A handler receives a borrowed session, so its caller retains the cleanup duty.
This body releases the session anyway:

```{literalinclude} ../crates/codepact/tests/projects/web_handler/buggy/src/main.py
:language: python
:start-at: def close_caller_session
:end-before: def lose_connection
```

CodePact reports TP2001 even though `session.release()` is a valid method call
and the declared Python type is compatible. In a real request path, the caller
may try to use the session after the handler has released it. The repaired
handler queries the session and leaves cleanup to its owner; see
[`web_handler/fixed`](https://github.com/eddiethedean/codepact/tree/v0.5.0/crates/codepact/tests/projects/web_handler/fixed).

## Discarding a required result: TP3001

The provider marks configuration as must-use, while the caller throws away the
returned decision:

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/provider.py
:language: python
:start-at: @must_use
```

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/main.py
:language: python
:start-at: def ignored_configuration
:end-before: def use_after_imported_close
```

The first declaration gives the result-use contract; the second is the
unhandled call. TP3001 reports the dropped result, which could otherwise mean
that a caller silently ignores a required configuration choice, validation
decision, or operation status. Ruff and Pyright do not ordinarily treat this
metadata as an obligation. The published 0.4.0 CLI includes TP3001.
[Release availability](availability.md)

## Following an imported cleanup helper

The plugin example also checks an effect across named re-exports. The caller
imports `close_token` from `bridge.py`; that helper is itself imported from
`provider.py` and releases its argument:

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/main.py
:language: python
:start-at: def use_after_imported_close
:end-before: def valid_plugin
```

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/bridge.py
:language: python
```

```{literalinclude} ../crates/codepact/tests/projects/plugin_runner/buggy/src/provider.py
:language: python
:start-at: def close_token
:end-before: @must_use
```

CodePact follows this supported call chain and reports TP1001 on the later
`inspect`. A type checker can verify that `close_token` accepts a `Token`, but
that signature alone does not say that it releases the token. The test corpus
also checks the repaired multi-module variant and input-order parity.

## Async and other analysis boundaries

CodePact distinguishes a modeled bug from a construct it cannot safely
resolve. For example, task scheduling transfers work to an event loop:

```{literalinclude} ../crates/codepact/tests/projects/async_handoff/boundary/src/main.py
:language: python
:start-at: def launch
```

The analyzer reports an unknown boundary and marks coverage incomplete. It does
not claim a definite leak or treat an advisory clean status as proof. The
synthetic controls also cover cancellation during async cleanup, dynamically
loaded plugins, and repeated close calls without an idempotency model. Read the
coverage reasons and the [supported syntax](supported-syntax.md) before relying
on a result in code that crosses one of these boundaries.

## What these examples establish

The corpus independently specifies 33 planted findings, repaired counterparts,
and boundary cases. All 20 source variants are checked with the repository's
Ruff rules and strict Pyright, then the analyzer is tested on Python source
targets 3.11–3.13. This demonstrates that CodePact finds the listed semantic
contracts in controlled inputs. It does not estimate real-project precision or
prove that an external resource declaration matches runtime behavior. See the
[corpus and test commands](synthetic-projects.md) and the separate
[synthetic preview qualification and retained field history](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.4/synthetic-preview-protocol.md).

# Rust API reference

The `typepact` crate exposes the same inspector used by the command-line program. It can be installed as a Rust library or used to build the CLI.

## Install

Add the crate to a Rust application:

```sh
cargo add typepact@0.1.0
```

Or install the command from crates.io:

```sh
cargo install typepact --version 0.1.0 --locked
```

Source builds require Rust 1.96 or newer. Python CLI wheels do not require a Rust toolchain at installation time.

## One-shot inspection

Create `SourceFile` values with a display path, a logical module name, and the source text. Pass all files that should participate in import resolution to `inspect`:

```rust
use typepact::{inspect, SourceFile};

fn main() {
    let source = r#"
from typepact import resource
from typepact.rust import Borrowed

@resource(closed_by="close")
class Connection:
    def close(self) -> None: ...

def read(conn: Borrowed[Connection]) -> str: ...
"#;

    let file = SourceFile::new("connection.py", "connection", source);
    let result = inspect(&[file], "3.11");
    assert_eq!(result.assurance, "contract-inspection-only");
}
```

`inspect` returns an `Inspection` with contract facts, diagnostics, and coverage. It does not import or execute the Python source. The analyzer accepts source targets 3.11, 3.12, and 3.13.

## Lifecycle checking preview

The current unreleased source also exposes `check(files, target)`, `AnalysisSession::check`, and cancellable/revision-tagged `AnalysisSession::check_cancellable`. They return the same `Inspection` shape with lifecycle findings and `assurance` set to `lifecycle-checking-for-supported-normal-flow`. This path tracks supported local constructions, aliases, branches and synchronous direct calls. Incomplete coverage and diagnostic certainty remain part of the result; a clean report is not a general resource-safety proof.

## Incremental analysis session

Use `AnalysisSession` when inputs change repeatedly. It stores source in a Salsa database, tracks input revisions, and invalidates computations from changed files while retaining reusable work for unchanged files.

```rust
use typepact::{AnalysisSession, SourceFile};

fn inspect_edits() {
    let mut session = AnalysisSession::default();
    let source = SourceFile::new(
        "connection.py",
        "connection",
        "class Connection: ...\n",
    );
    let result = session.inspect_cancellable(&[source], "3.11");
    assert!(result.is_ok());
}
```

`inspect_cancellable` returns a `SessionInspection` containing the revision and result. Each call synchronizes the session inputs to the supplied `SourceFile` list, removing omitted paths and applying changed source before computing the result. For an editor workflow, pass the current set of project files on each call so Salsa can reuse computations for unchanged inputs.

The session also provides `set_file`, `remove_file`, `reset`, `files`, `parse_summary`, and `parse_summary_for`. `set_file` reports file-count or source-size violations as `SessionInputError`. `CancellationToken` can request cancellation from another thread; cancellation publishes no partial result, and the session can be used again after the query stops.

The default one-shot function and incremental session share the same analysis pipeline and input bounds. See the [architecture page](architecture.md) for the computation model.

## Session operations

| Operation | Behavior |
| --- | --- |
| `inspect(files, target)` | Synchronizes the full input snapshot and returns `Inspection` |
| `check(files, target)` | Synchronizes the snapshot and adds bounded lifecycle diagnostics |
| `check_cancellable(files, target)` | Returns checked `SessionInspection` with revision; cancellation returns `AnalysisCancelled` |
| `inspect_cancellable(files, target)` | Returns `Result<SessionInspection, AnalysisCancelled>` with a revision on success |
| `set_file(path, module, source)` | Updates one stored input; returns `SessionInputError` if its source or file budgets would be exceeded |
| `remove_file(path)` | Removes a stored input; removing an absent path is a no-op |
| `reset()` | Clears inputs and memoized state and advances the revision |
| `parse_summary(path)` | Returns a stored file's parse summary using the default 3.11 target |
| `parse_summary_for(path, target)` | Returns a stored file's parse summary using the selected target |

`set_file` is useful when working with stored parse summaries. It does not replace the full snapshot argument to inspection: passing an empty file list to either inspection method removes all stored inputs. Keep your source snapshot in the host application and pass it on every inspection.

## Cancellation and results

Obtain `session.cancellation_token()` before starting the inspection, then give the token to the thread responsible for cancelling work. Calling `cancel()` requests cancellation of the associated query. Use `inspect_cancellable` to distinguish cancellation from a completed analysis: it returns `Err(AnalysisCancelled)` without publishing partial output. The convenience `inspect` method represents cancellation as an incomplete report with `TP9002`.

An `Ok` return does not establish successful declaration analysis. Check the returned `inspection.coverage.status` and diagnostics, since syntax errors, unresolved declarations, and work limits can be represented by a completed report. `SessionInspection.revision` describes the source/settings revision, not the JSON schema version.

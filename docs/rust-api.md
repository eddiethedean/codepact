# Rust API reference

The `typepact` crate exposes the same inspector and lifecycle checker used by the command-line program. It can be installed as a Rust library or used to build the CLI.

## Install

Add the crate to a Rust application:

```sh
cargo add typepact@0.2.0
```

Or install the command from crates.io:

```sh
cargo install typepact --version 0.2.0 --locked
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

## Lifecycle checking

The published 0.2.0 crate exposes `check(files, target)`, `AnalysisSession::check`, and cancellable/revision-tagged `AnalysisSession::check_cancellable`. They return the same `Inspection` shape with lifecycle findings and `assurance` set to `lifecycle-checking-for-supported-normal-flow`. This path tracks supported local constructions, aliases, branches, synchronous calls, and imported helpers through acyclic named re-exports. Incomplete coverage and diagnostic certainty remain part of the result; a clean report is not a general resource-safety proof.

The host supplies all
`SourceFile` values for checking: filesystem discovery belongs to the CLI adapter,
not to `check`. Consult the [lifecycle tutorial](lifecycle.md) for modeled behavior
and the [output reference](output.md) for certainty and coverage interpretation.

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

Lifecycle body checks use tracked per-file queries with their supplied import
dependencies. An edit to an unrelated file can reuse unaffected body checks;
editing an imported effect refreshes that module and its callers. The 0.2
validation matrix records query-execution evidence for both cases; see the
[release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md) for qualification results.

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

## Public types and limits

The crate root exports the following result and input types. The 0.x API is
provisional; the [`typepact` 0.2.0 rustdoc](https://docs.rs/typepact/0.2.0/typepact/)
documents the published crate.

| Type or function | Purpose |
| --- | --- |
| `SourceFile { path, module, source }` | One source snapshot item; `SourceFile::new(path, module, source)` accepts values convertible to `String` |
| `inspect(files, python_version) -> Inspection` | One-shot declaration inspection |
| `check(files, python_version) -> Inspection` | Lifecycle checking for the supported subset |
| `AnalysisSession` | Reusable database for full-snapshot updates, inspection, and checking |
| `SessionInspection { revision, inspection }` | Result plus the session revision used for analysis |
| `AnalysisCancelled` | Error returned by cancellable methods when no complete current result is available |
| `SessionInputError` | `FileLimitExceeded`, `SourceLimitExceeded`, or `TotalSourceLimitExceeded` from `set_file` |
| `CancellationToken` | `cancel()` requests cancellation; `is_cancelled()` reports its state |
| `Span`, `Provenance`, `Lifecycle`, `Contract`, `Diagnostic`, `Coverage`, `Inspection` | Public result model types; fields are public and serializable |
| `is_python_identifier(value, python_version) -> bool` | Checks an identifier against the selected analyzer target's Unicode rules |

The crate root also exports `SCHEMA_VERSION` and these defaults:
`DEFAULT_MAX_SOURCE_BYTES` (1,000,000 bytes per file and request),
`DEFAULT_MAX_DECLARATIONS` (10,000 contracts),
`DEFAULT_MAX_INPUT_FILES` (10,000 files),
`DEFAULT_MAX_AST_DECLARATIONS` (20,000 statements per file),
`DEFAULT_MAX_TOTAL_AST_DECLARATIONS` (50,000 statements per request),
`DEFAULT_MAX_IMPORT_EDGES` (50,000 edges), and
`DEFAULT_MAX_SESSION_MUTATIONS` (1,000 changes before session storage resets).

`Inspection` carries `schema_version`, `assurance`, `contracts`, `diagnostics`,
and `coverage`. Contract records contain the normalized base symbol, display
name, optional lifecycle, access intent, source span, and provenance. Diagnostic
records contain a code, severity, message, primary and related spans, and
certainty. Span lines and columns are one-based; columns count UTF-8 bytes.
String-valued access, certainty, severity, assurance, and coverage status fields
are schema values, so consumers should handle unknown values defensively.

The public `db` module also exposes lower-level Salsa types: `Db`, `Database`,
`FileInput`, `ParseSummary`, and `parse_file`. `FileInput` represents the path,
logical module, and source text stored in the database. `ParseSummary` exposes
`ok` and an optional `error`; its converted declarations and flow statements are
internal. `files()` yields the session's current `FileInput` handles. These types
provide access to the current database integration and are not needed for
ordinary one-shot or session use. Treat them as provisional during 0.x; consult
the source rustdoc for their exact signatures.

## Complete error and status handling

`inspect` and `check` return an `Inspection` even when the analyzer found syntax,
declaration, or coverage problems. A returned value is not synonymous with a
clean analysis. Check `inspection.coverage.status` and inspect diagnostics.

`inspect_cancellable` and `check_cancellable` return
`Result<SessionInspection, AnalysisCancelled>`. `set_file` returns
`Result<(), SessionInputError>`. `remove_file`, `reset`, and the convenience
`inspect`/`check` methods do not return `Result`. The convenience `inspect`
represents cancellation as an incomplete report containing `TP9002`; use the
cancellable method when the host must distinguish cancellation from a completed
analysis.

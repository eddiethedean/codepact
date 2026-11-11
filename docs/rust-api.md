# Rust API reference

The published `codepact` crate exposes the same inspector and lifecycle checker used by the command-line program. CodePact 0.4.0 is available on crates.io.

## Install

Add the released crate to a Cargo project with `cargo add codepact@0.4.0`. To
build the workspace crate from source, run this from the repository root:

```sh
cargo build --locked -p codepact
```

Run the CLI from the checkout:

```sh
cargo run --locked -p codepact -- --version
```

Source builds require Rust 1.96 or newer. Python CLI wheels do not require a Rust toolchain at installation time.

## One-shot inspection

Create `SourceFile` values with a display path, a logical module name, and the source text. Pass all files that should participate in import resolution to `inspect`:

```rust
use codepact::{inspect, SourceFile};

fn main() {
    let source = r#"
from codepact import resource
from codepact.rust import Borrowed

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

The historical TypePact 0.2.0 crate exposed `check(files, target)`, `AnalysisSession::check`, and cancellable/revision-tagged `AnalysisSession::check_cancellable`. CodePact 0.3.0 retains those methods and adds cleanup-aware checking. Incomplete coverage and diagnostic certainty remain part of the result; a clean report is not a general resource-safety proof.

The host supplies all
`SourceFile` values for checking: filesystem discovery belongs to the CLI adapter,
not to `check`. Consult the [lifecycle tutorial](lifecycle.md) for modeled behavior
and the [output reference](output.md) for certainty and coverage interpretation.

CodePact 0.3.0 extends checking to modeled return, raise, loop and
cleanup exits and uses the provisional assurance string
`lifecycle-checking-for-supported-exit-flow`. Its qualified behavior and
release evidence are recorded in the [0.3.0 release record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md).

The crate also exposes `check_report(files, target)` and
`AnalysisSession::check_report`, returning `CheckResult` with
`check_schema_version: 1`. Its checker diagnostics add completion kind, trust,
coverage reason and repair guidance while declaration inspection remains schema
version 0. The result includes the selected Python target and versioned built-in
model catalog. The [`codepact` 0.3.0 rustdoc](https://docs.rs/codepact/0.3.0/codepact/)
documents the published API. `AnalysisSession::check_report_cancellable` returns a
revision-tagged `SessionCheckResult`. `check` continues returning `Inspection`
for source compatibility. The 0.x API remains provisional and may change in a
future release.

CodePact 0.4.0 adds `check_with_must_use(files, target)` and
`AnalysisSession::check_with_must_use` (plus a cancellable session method) for
the synchronous TP3001 result-use rule. These return `Inspection` so the
existing `check` and `check_report` APIs retain their released rule set.
`check` returns `Inspection` (schema 0), and `check_report` returns
`CheckResult` (schema 1). A caller that needs the CLI adoption policy can wrap canonical
results with the public `codepact::adoption` resolver and report builder. This
preview API is covered by synthetic controls; real-world usefulness is unqualified.

The default adoption policy includes TP3001, so its canonical input must come
from `check_with_must_use` using the same source files and target version.
`build_report` and `write_baseline` reject a lifecycle-only result when must-use
is selected, and reject cancelled or inspection-only results. A legacy lifecycle
result remains usable when TP3001 is explicitly disabled. The report's
`assurance` identifies the analyzed scope; it is part of snapshot identity.

## Incremental analysis session

Use `AnalysisSession` when inputs change repeatedly. It stores source in a Salsa database, tracks input revisions, and invalidates computations from changed files while retaining reusable work for unchanged files.

```rust
use codepact::{AnalysisSession, SourceFile};

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
[release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.2.0-evidence.md) for qualification results.

## Session operations

| Operation | Behavior |
| --- | --- |
| `inspect(files, target)` | Synchronizes the full input snapshot and returns `Inspection` |
| `check(files, target)` | Synchronizes the snapshot and adds bounded lifecycle diagnostics |
| `check_with_must_use(files, target)` | Lifecycle check plus synchronous TP3001 diagnostics |
| `check_cancellable(files, target)` | Returns checked `SessionInspection` with revision; cancellation returns `AnalysisCancelled` |
| `check_with_must_use_cancellable(files, target)` | Cancellable check including synchronous TP3001 |
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
provisional. [CodePact 0.4.0 rustdoc](https://docs.rs/codepact/0.4.0/codepact/) ·
[Historical TypePact 0.2.0 rustdoc](https://docs.rs/typepact/0.2.0/typepact/)

| Type or function | Purpose |
| --- | --- |
| `SourceFile { path, module, source }` | One source snapshot item; `SourceFile::new(path, module, source)` accepts values convertible to `String` |
| `inspect(files, python_version) -> Inspection` | One-shot declaration inspection |
| `check(files, python_version) -> Inspection` | Lifecycle checking for the supported subset |
| `check_report(files, python_version) -> CheckResult` | 0.3.0 checker result with its own schema version and cleanup metadata |
| `AnalysisSession` | Reusable database for full-snapshot updates, inspection, and checking |
| `AnalysisSession::check_report(files, target)` | Versioned check result for the current full input snapshot |
| `SessionInspection { revision, inspection }` | Result plus the session revision used for analysis |
| `SessionCheckResult { revision, result }` | Versioned check result plus the session revision used for analysis |
| `AnalysisSession::check_report_cancellable(files, target)` | Cancellable, revision-tagged checker result |
| `AnalysisCancelled` | Error returned by cancellable methods when no complete current result is available |
| `SessionInputError` | `FileLimitExceeded`, `SourceLimitExceeded`, or `TotalSourceLimitExceeded` from `set_file` |
| `CancellationToken` | `cancel()` requests cancellation; `is_cancelled()` reports its state |
| `Span`, `Provenance`, `Lifecycle`, `Contract`, `Diagnostic`, `Coverage`, `Inspection` | Inspection result model types; fields are public and serializable |
| `CheckResult`, `CheckDiagnostic`, `CheckModelMetadata` | 0.3.0 versioned checker result model |
| `is_python_identifier(value, python_version) -> bool` | Checks an identifier against the selected analyzer target's Unicode rules |

The crate root also exports `SCHEMA_VERSION`, `CHECK_SCHEMA_VERSION`,
`CHECK_MODEL_CATALOG_VERSION`, and these defaults:
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


## Experimental ownership in the 0.5 candidate

`codepact::check_with_ownership` and
`AnalysisSession::check_with_ownership_cancellable` return structured check
results for lifecycle, must-use, and the selected ownership scope. Pass
`Ownership::Enabled` for local and resolved interprocedural ownership checks,
or `Ownership::AssumeDefaultTaskFactory` for the pinned TaskGroup model with
its explicit runtime assumption. `Ownership::Disabled` selects the 0.4
adoption scope. Scope changes advance the reusable session revision and are
part of the flow-query inputs. [Experimental ownership guide](ownership.md)

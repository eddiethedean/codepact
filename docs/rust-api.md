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

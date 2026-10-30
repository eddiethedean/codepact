# Contributing to TypePact

TypePact 0.1 is under active development. Start with the [planning index](docs/planning/README.md), the [inspector guide](docs/inspector.md), and the [phase 0.1 repair plan](docs/planning/phases/0.1/remediation.md).

The Rust analyzer requires Rust 1.96 or newer. Install the pinned dependency-checking tools once with:

~~~sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
~~~

Install the locked Python environment and Rust dependency auditors, then run
the same full gate used by CI:

~~~sh
uv sync --locked
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
~~~

The Python package supports Python 3.11 and newer. Its CI matrix runs package
checks on Python 3.11 through 3.14. To run the strict package typing and lint
checks locally, use the focused targets:

~~~sh
uv run --locked make check-python check-python-package
~~~

The full `make check` target also runs Rust formatting, compilation, Clippy,
tests, MSRV validation, packaged-crate smoke checks, dependency audits, and the
inspector side-effect guard. CI runs the Python targets on Python 3.11 through
3.14 and runs the Rust and dependency checks once per workflow.

The uv workspace has a pure Python typepact package built by Hatchling and a typepact-cli package built by maturin. typepact pins typepact-cli to the same version. Package installation checks must prove that installing typepact resolves its CLI dependency and that standalone typepact-cli works, in clean environments outside the checkout. CLI source builds require Rust 1.96+; helper source builds remain pure Python. Cargo archive checks should run from the extracted `typepact` crate, outside the checkout. Analyzer changes should add fixtures for supported behavior and explicit incomplete-coverage boundaries. Never execute inspected Python files as part of analysis.

Before changing public semantics, document the use case, canonical contract, compatibility impact and both interface expressions. Keep unsupported constructs visible rather than silently treating them as resolved.

See the [release plan](docs/planning/releases.md) for versioning, distribution and candidate evidence requirements.

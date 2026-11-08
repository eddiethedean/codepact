# Contributing to CodePact

Version 0.1.0 is published. The current source also contains a lifecycle
checking preview for the upcoming 0.2.0 release; its qualification gates remain
open. Start with the [development guide](docs/development.md) for setup and the
[0.2 phase plan](docs/planning/phases/0.2-lifecycle.md) for current scope.

## Set up the repository

Use Python 3.11 or newer, Rust 1.96 or newer, `uv`, Make, and a linker. The full
dependency audit also needs the pinned `cargo-audit` and `cargo-deny` tools.

~~~sh
uv sync --locked
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
~~~

The `make check` target runs Rust formatting, compilation, Clippy and tests;
Python typing/lint/package checks; dependency audits; MSRV and archive checks;
the inspector's no-execution guard; and the strict Sphinx build. GitHub Actions
runs the Python checks on Python 3.11–3.14 and the Rust and dependency checks on
Linux. Use focused targets while iterating; see the
[development guide](docs/development.md#focused-checks).

## Find a good first change

The repository has no published list of beginner issues. A contained first
contribution is to clarify an existing guide, add a minimal supported or
boundary example, or improve a diagnostic explanation. The source lives in
`crates/codepact/src`, Python helper declarations live in `python/codepact`,
fixture tests live in `crates/codepact/tests`, and user docs live in `docs`.

For analyzer work, describe the source pattern, add a positive or negative
fixture, and add an explicit incomplete-coverage case when behavior is unknown.
The [diagnostic catalog](docs/rules.md) and [supported syntax](docs/supported-syntax.md)
are the reference for expected behavior. Never execute inspected Python files
as part of analysis.

## Submit a change

1. For a behavior or API change, open an issue or design discussion with a
   minimal source example and expected result.
2. Make one focused change and update the relevant public guide, limitations,
   and release notes.
3. Run the focused check for the affected component, then run
   `uv run --locked make check` when practical.
4. In the pull request, describe the behavior, compatibility impact, evidence,
   and any remaining uncertainty.

Public APIs, diagnostic codes, configuration keys, and semantic changes need a
recorded design decision. Keep unsupported constructs visible rather than
silently treating them as resolved. Do not add dependencies or copied code
without provenance and license review.

## Release work

Release versions must match across the Python helper, CLI, and Rust crate. The
tag workflow builds and validates the exact artifact set before publishing.
See the [release plan](docs/planning/releases.md) and the
[0.2.0 release checklist](docs/planning/release-0.2.0-checklist.md). Do not
publish a release from an unreviewed candidate or claim qualification from a
passing unrelated CI run.

The accountable maintainer may review their own changes, validate all release
evidence and approve publication alone. Additional reviewers are optional;
no independent-review waiver is required. See the
[maintainer authority policy](docs/planning/release-gates.md#maintainer-authority).

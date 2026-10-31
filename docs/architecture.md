# Architecture

TypePact separates Python declarations, source parsing, and semantic inspection:

1. The `typepact` Python package offers `resource` and `Annotated` metadata. Importing it does not load or launch the analyzer.
2. The `typepact-cli` package distributes the native Rust `typepact` command.
3. The Rust library parses supplied source text, resolves the supported declaration relationships, and emits contracts, diagnostics, and coverage.

## Source is not executed

The inspector reads UTF-8 source files as text. It does not import the analyzed project, evaluate annotations, invoke decorators, or run build hooks. A declaration therefore contributes only the static facts that TypePact can resolve from its syntax and the other files supplied in the request.

## Parser and semantic queries

The frontend uses pinned Ruff Python parser and AST components to convert each file into typed module declarations and source ranges. The semantic layer resolves TypePact names, class/alias identities, direct supplied-module imports, lifecycle metadata, and access metadata. Constructs outside the supported set produce incomplete coverage.

The library uses Salsa to track parsing, module inspection, and export summaries. An `AnalysisSession` can reuse unaffected work when files change. Consumers depend on semantic export summaries, so a provider body edit that does not change a relevant export need not invalidate dependent module results. Session cancellation prevents a stale or partial result from being published.

## Shared engine

The one-shot Rust `inspect` function, `AnalysisSession`, and CLI use the same query pipeline. The CLI adds file loading, logical module naming, JSON serialization, and process exit status. This keeps command output and library results consistent.

See the [Rust API](rust-api.md) for use of the one-shot and incremental interfaces, and the [inspector guide](inspector.md) for implementation details.

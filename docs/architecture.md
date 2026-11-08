# Architecture

CodePact separates Python declarations, source parsing, declaration inspection,
and lifecycle checking:

1. The `codepact` Python package offers `resource` and `Annotated` metadata. Importing it does not load or launch the analyzer.
2. The `codepact-cli` package distributes the native Rust `codepact` command.
3. The Rust library parses supplied source text, resolves supported declaration relationships, and emits contracts, diagnostics, and coverage.
4. The 0.2.0 checker runs a bounded flow pass over module statements and synchronous bodies to produce lifecycle findings.

## Source is not executed

The analyzer reads UTF-8 source files as text. It does not import the analyzed project, evaluate annotations, invoke decorators, or run build hooks. A declaration therefore contributes only the static facts that CodePact can resolve from its syntax and the other files supplied in the request.

## Parser and semantic queries

The frontend uses pinned Ruff Python parser and AST components to convert each file into typed module declarations and source ranges. The semantic layer resolves CodePact names, class/alias identities, direct supplied-module imports, lifecycle metadata, and access metadata. Constructs outside the supported set produce incomplete coverage.

The library uses Salsa to track parsing, module inspection, and export summaries. An `AnalysisSession` can reuse unaffected work when files change. Consumers depend on semantic export summaries, so a provider body edit that does not change a relevant export need not invalidate dependent module results. Session cancellation prevents a stale or partial result from being published.

## Lifecycle flow

The checking path starts from declaration inspection and parsed body statements.
Its flow state separates local names from modeled resource objects, tracks open,
closed, and unknown state, and joins supported normal branches. Supported calls
apply body effects to passed objects; recursive calls recover to unknown summaries.

The current implementation uses a project checking query. Phase 0.2 qualified
its invalidation and reuse behavior; the inspection query cache alone does not
provide that guarantee. See the
[phase plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.2-lifecycle.md).

## Shared engine

The one-shot Rust entry points, `AnalysisSession`, and CLI share the analyzer.
The CLI adds file loading, logical module naming, project-root scanning for
`check`, text or JSON rendering, and process exit status. Rust callers provide
their own explicit source snapshot. Inspection and checking carry different
assurance strings so consumers can identify which analysis produced a result.

See the [Rust API](rust-api.md) for use of the one-shot and incremental interfaces, and the [inspector guide](inspector.md) for implementation details.

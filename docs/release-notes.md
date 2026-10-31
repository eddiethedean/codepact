# Release notes

## Upcoming 0.2.0 — source preview, not yet released

The current source checkout adds `typepact check` for bounded normal synchronous lifecycle
analysis. Version 0.1.0 remains the latest published release and provides contract inspection;
0.2.0 qualification gates remain pending in the
[phase plan](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.2-lifecycle.md).

- Local constructions, aliases, straight-line calls, returns, and `if`/`else` joins.
- TP1001 use-after-close findings with certainty and related close locations.
- TP2001 close-through-Borrowed findings in supported bodies.
- Project-root scanning, readable output, provisional JSON, and Rust checking entry points.
- Unknown summaries for recursive calls and visible coverage boundaries.

Recent fixes preserve resource metadata across calls, retain uncertain alias
alternatives, propagate supported method argument effects, evaluate nested
constructor calls, and keep literal arguments from weakening known resource state.
Checking also removes inspection-only boundaries for supported module statements.

Start with the [lifecycle tutorial](lifecycle.md). Possible findings and incomplete
coverage alone can return exit `0`; read the report before accepting a result.
Ownership enforcement and cleanup on every exit remain outside this preview.

## 0.1.0 — published

TypePact 0.1.0 introduces resource contract inspection for Python, with pure Python declaration helpers and a native Rust CLI distributed through `typepact-cli`. The helper depends on the exact matching CLI version.

### Included capabilities

- Identity-preserving `resource` declarations and `Annotated` lifecycle/access metadata.
- `Borrowed`, `Owned`, and `BorrowedMut` access intent, using direct symbol imports or `from typepact import rust`/`contracts` submodule imports.
- Literal `None` parameter and return annotations as ordinary non-resource types.
- Captured assignment aliases, rebinding-aware declaration identities, and direct absolute imports between explicitly supplied files.
- Schema-v0 JSON with contract facts, UTF-8 byte spans, provenance, diagnostics, and explicit coverage status.
- A shared Ruff/Salsa engine, reusable Rust analysis sessions, revision-tagged results, cancellation, and bounded work.

### Coverage boundaries

The inspector reports incomplete coverage for nested resource contracts in unsupported container annotation forms and eager class-body global declarations that can mutate module bindings. Deferred method-body global declarations do not invalidate the module merely by appearing in source.

String/future annotations, relative imports, re-export chains, inheritance, arbitrary decorators, and control-flow analysis remain outside the supported subset. See [limitations](limitations.md). All facts are `declared-unverified`; lifecycle flow and ownership enforcement are planned for later phases.

### Compatibility and upgrading

The helpers require Python 3.11+; quality CI covers CPython 3.11–3.14. Analyzer source targets are 3.11–3.13. The release matrix builds eight native CLI wheels and a universal pure helper wheel, plus both source archives. CLI source builds require Rust 1.96+ and a linker. See [installation](install.md) for platform minimums.

This is the initial release, so there is no earlier public version to migrate from. Keep `typepact` and `typepact-cli` versions aligned and review schema/diagnostic changes during the provisional 0.x series.

Maintainer qualification is tracked in the [candidate evidence record](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.1.0-evidence.md).

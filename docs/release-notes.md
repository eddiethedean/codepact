# Release notes

## 0.1.0

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

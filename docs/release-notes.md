# Release notes

## 0.3.0 — candidate, not released

This unreleased candidate extends lifecycle checking with bounded cleanup
obligations. Release qualification is pending; this section describes candidate
behavior and must not be read as a published support claim. See the [phase
plan](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3-cleanup.md), [supported subset and
limitations](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/specification.md), [validation
matrix](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/validation.md), [candidate evidence record](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.3.0-evidence.md), and [candidate tutorial](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/cleanup-tutorial.md).

### Candidate behavior

- TP1002 reports owned local resources left outstanding on modeled function
  exits. Direct `Owned[T]` returns and resolved owning helpers transfer the
  cleanup duty at their supported boundary; aliases share one duty.
- `try`/`except`/`else`/`finally`, bounded `while`/`for` loops, early exits,
  reviewed context cleanup, and cleanup failures retain path-specific
  uncertainty. Built-in exception handlers use a bounded ancestry table;
  custom exception relationships remain explicit boundaries unless exact
  identity is known.
- Source-only models recognize exact `contextlib.closing` and
  `contextlib.aclosing` identities for selected Python targets 3.11–3.13.
- Bounded direct coroutine awaits include suspension and cancellation outcomes.
  Possible cleanup interruption and incomplete coverage remain visible.
  Resolved coroutine cancellation preserves unrelated local duties while
  weakening captured or execution-affected resources.
- A separate versioned check-result schema carries completion kind, certainty,
  trust, repair guidance, model metadata, and coverage information.

### Compatibility and limitations

The checker does not prove unique ownership, move validity, borrow lifetimes,
task completion, race freedom, or runtime leak freedom. Task scheduling,
TaskGroup joins, arbitrary awaitables, generators, general callbacks and
container/field ownership, process/thread handoffs, and unsupported managers
remain explicit boundaries. Exhausted work budgets weaken affected state and
report incomplete coverage. Resource declarations remain static contracts, not
proof that an external close operation succeeds at runtime.

TP1002 is a new default diagnostic in the candidate and can make existing
`typepact check` commands exit with status 1 for definite outstanding duties.
Possible findings and boundary-only reports continue to return 0; malformed or
unrecoverable input returns 2. The candidate has not yet passed the 0.3
qualification gates, and no package version has been published for this
behavior.

## 0.2.0 — published 2026-11-01

TypePact 0.2.0 adds `typepact check` for bounded normal synchronous lifecycle
analysis. It is available through the matching `typepact` and `typepact-cli`
Python packages on PyPI and the `typepact` crate on crates.io. The qualified scope
and limitations are in the [phase plan](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.2-lifecycle.md)
and [release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md).

- Local constructions, aliases, straight-line calls, returns, and `if`/`else` joins.
- TP1001 use-after-close findings with certainty and related close locations.
- TP2001 close-through-Borrowed findings in supported bodies.
- Project-root scanning, acyclic named re-exports, readable output, provisional JSON, and Rust checking entry points.
- Unknown summaries for recursive calls and visible coverage boundaries.

Recent fixes preserve resource metadata across calls, retain uncertain alias
alternatives, propagate supported method argument effects, evaluate nested
constructor calls, and keep literal arguments from weakening known resource state.
Checking also removes inspection-only boundaries for supported module statements.
Unknown aliases remain unknown when copied, possibly unbound aliases produce a
coverage boundary, and alias/object/flow limits weaken state instead of
preserving an exact proof. File-scoped lifecycle queries now reuse unaffected
checks after unrelated edits while refreshing callers when an imported effect
changes. Named re-exported helpers carry source origins through the call so
diagnostics point back to the defining module.

Start with the [lifecycle tutorial](lifecycle.md). Possible findings and incomplete
coverage alone can return exit `0`; read the report before accepting a result.
Ownership enforcement and cleanup on every exit remain outside the supported
0.2.0 subset.

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

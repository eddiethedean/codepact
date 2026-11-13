# Release notes

## 0.5.1 (unreleased)

- Remove obsolete version numbers from TP9001 coverage messages for
  unsupported declarations and annotations. Diagnostic codes, analysis
  coverage, and exit policy keep their existing behavior.
  [Issue #2](https://github.com/eddiethedean/codepact/issues/2)
- `codepact --version` and `codepact -V` report an output error and exit with
  status 2 if stdout is a closed pipe.
  [Issue #11](https://github.com/eddiethedean/codepact/issues/11)
- Rust adoption reports keep the same policy fingerprint and snapshot ID
  when callers reorder source files containing inline suppressions.
  [Issue #9](https://github.com/eddiethedean/codepact/issues/9)
- Lifecycle checks follow the reachable branch of `if True` and `if False`.
  A definite use-after-close in reachable code fails the default advisory check.
  [Issue #5](https://github.com/eddiethedean/codepact/issues/5)
- Baseline updates scan each source file once for inline suppressions, speeding
  up generation when a file contains many findings.
  [Issue #8](https://github.com/eddiethedean/codepact/issues/8)
- Baseline matching includes the exact engine version. Review and regenerate
  a 0.5.0 baseline when upgrading to 0.5.1.
  [Baseline workflow](adopting.md#record-the-initial-findings)
- Lifecycle source locations now handle bare carriage returns consistently.
  Adoption report identities include module identity, and inline suppression
  indexing no longer depends on source-file order.
- Malformed non-UTF-8 command-line arguments now return a structured CLI
  error instead of panicking. Empty output-format values are rejected.
- Ownership analysis delays uncertain container-alias boundaries until an
  alias is read, while still reporting possibly unbound aliases and avoiding
  false reports for lambda captures.
- The CLI reports source and declaration boundaries without stale version
  wording. Existing diagnostic codes and exit policy are unchanged.

## 0.5.0 — experimental ownership

Published on November 10, 2026, CodePact 0.5.0 adds opt-in explicit transfers,
lexical shared borrows, and
structured task lifetimes for a bounded CPython `asyncio.TaskGroup` model. It
includes the typed identity helper `codepact.move` and experimental rules
TP2002–TP2006. The existing lifecycle and must-use policy remains the default.
Install matching 0.5.0 helpers and CLI. Review baseline scope before enabling
the new checks. Synthetic examples qualify the supported scope.
[Installation](install.md) · [Experimental ownership](ownership.md) ·
[Migration](upgrading.md#upgrade-from-04-to-05) ·
[Release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.5.0-evidence.md)

## 0.4.0 — synthetic preview

Published on November 10, 2026, CodePact 0.4.0 adds adoption tooling to the
lifecycle checker as a synthetic preview. Real-world usefulness remains
unqualified. The tag-triggered workflow passed all 25 jobs, and the 11 PyPI
files and crates.io archive passed registry verification.
[Release ledger](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.4.0-evidence.md) ·
[PyPI helper](https://pypi.org/project/codepact/0.4.0/) ·
[PyPI CLI](https://pypi.org/project/codepact-cli/0.4.0/) ·
[Rust crate](https://crates.io/crates/codepact/0.4.0)

- Validated project settings, rule selection, severity overrides, effective
  setting origins, and advisory/strict check policy.
- Explicit baseline creation and replacement with stable finding matching,
  plus line suppressions that require exact eligible codes and a reason.
- Baseline fingerprint algorithm 5 uses canonical Python tokens and resolved
  resource and callable identities. It recognizes equivalent whitespace and
  multiline calls. Changed cleanup declarations and imported resource
  identities reopen findings. Regenerate algorithm 3/4 baselines explicitly.
- Synchronous must-use contracts through `@must_use`, `MustUse[T]`, and
  `MustUseResult`, with TP3001 for supported bare discarded calls.
- CLI check schema 2, text/JSON/concise output, bounded detail reporting,
  snapshot identity, capabilities, static rule explanations, and published
  schemas with valid/invalid fixtures.
- Existing declaration inspection and Rust checking APIs retain their released
  result contracts. Mandatory errors and incomplete coverage remain visible
  when eligible findings are suppressed.

Review interface changes before rolling out project policy. [Upgrade guide](upgrading.md) · [Adoption guide](adopting.md)

## 0.3.0 — published 2026-11-08

CodePact 0.3.0 extends lifecycle checking with bounded cleanup obligations.
Its qualification and publication evidence are linked below. See the [phase
plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3-cleanup.md), [supported subset and
limitations](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/specification.md), [validation
matrix](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/validation.md), [release evidence record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md), and [tutorial](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/cleanup-tutorial.md).

This is the first CodePact release. The Python distributions and
import, CLI distribution and executable, Rust crate and import, and GitHub
repository use the `codepact` name. The previous 0.1.0 and 0.2.0 TypePact
artifacts remain documented under their original names; compatibility aliases
are not provided.

### Included behavior

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
- CodePact preserves distinct identities for repeated factory results,
  executes only the specifically awaited coroutine, clears exception targets
  after their handlers, and routes failures from `raise ... from ...` causes
  through the correct exception path.
- Open module-scope resources and resource values stored in `global`
  bindings receive explicit lifetime/escape boundaries. Module
  completion is not treated as process shutdown or automatic cleanup; ordinary
  scalar global assignments remain precise.

### Compatibility and limitations

The checker does not prove unique ownership, move validity, borrow lifetimes,
task completion, race freedom, or runtime leak freedom. Task scheduling,
TaskGroup joins, arbitrary awaitables, generators, general callbacks and
container/field ownership, process/thread handoffs, and unsupported managers
remain explicit boundaries. Exhausted work budgets weaken affected state and
report incomplete coverage. Resource declarations remain static contracts, not
proof that an external close operation succeeds at runtime.

TP1002 is a new default diagnostic in 0.3.0 and can make existing
`codepact check` commands exit with status 1 for definite outstanding duties.
Possible findings and boundary-only reports continue to return 0; malformed or
unrecoverable input returns 2. See the linked release evidence for the passed
qualification gates and the exact published artifact hashes.

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

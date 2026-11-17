# Phase 0.7 technical qualification evidence

This record consolidates the implementation evidence for the 0.7.0 release
candidate. The implementation revision is
`be8691f08a48e43abac726ecd648b827f7cb3f1a`. It adds qualification tests only
after `8108f9458b730636eb3e5f192ac9b8801f7a5ddb`; the runtime and analyzer
implementation is unchanged from the retained performance and provider-wheel
captures. The evidence-recording commit may add documentation only.
Qualification does not supply maintainer review or release approval. Common
and phase-specific gate statuses remain Pending until the accountable
maintainer reviews and records them in the
[release evidence record](../../../../releases/0.7.0-evidence.md).

## Candidate and environment

- Comparison base: published `v0.6.0`, commit
  `2d638e761a26211e5c98f1792731103dd9856994`.
- Local host: macOS 26.5.2 arm64, CPython 3.11.14, Rust 1.96.0.
- Hosted Python matrix: CPython 3.11–3.14. The 3.11.14 local run skips five
  concurrency conformance cases pinned to CPython 3.13.16; hosted runs execute
  those cases on their pinned runtime.
- Raw installed-provider, query invalidation, adapter invalidation, hint
  invalidation, capture-boundary, and performance results are stored beside
  this file as JSON. The invalidation and capture-boundary records identify
  the implementation revision above.

## Evidence map

| Validation row | Retained implementation evidence |
| --- | --- |
| V07-01 | Runtime representation, identity, frozen payload fields, equality, hash behavior, and positional/keyword matching: [`test_outcomes.py`](../../../../../tests/python/test_outcomes.py). Cross-namespace identities are checked by the same test. |
| V07-02 | Positive and rejected covariant assignments, inferred payload types, `isinstance` and match narrowing, mutation and subclass rejection: [`outcomes_valid.py`](../../../../../tests/checkers/outcomes_valid.py), [`outcomes_invalid.py`](../../../../../tests/checkers/outcomes_invalid.py), and installed-wheel checker replay in [`check_python_package.py`](../../../../../scripts/check_python_package.py). |
| V07-03 | Falsey/null conversions, payload identity, absence, and `Some(None)` loss: `test_nullable_conversion_preserves_falsey_values_and_records_loss` in [`test_outcomes.py`](../../../../../tests/python/test_outcomes.py). |
| V07-04–05 | Sync/async capture, selected subclasses, exactly-once invocation, argument forwarding, invalid declarations, exception identity, groups, process-control exceptions and cancellation at task start, await, and cleanup: [`test_outcomes.py`](../../../../../tests/python/test_outcomes.py). `test_sync_capture_wraps_an_awaitable_without_running_or_joining_it` confirms a synchronous capture leaves a returned coroutine pending. The async adapter analyzer fixture retains its unresolved-effect boundary and an independent violation in [`capture-boundary.json`](capture-boundary.json), backed by `phase07_async_capture_keeps_unresolved_effects_and_independent_resource_findings` in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). Existing controlled join, pre-start cancellation and pending-coroutine controls are in [`phase03.rs`](../../../../../crates/codepact/tests/phase03.rs) and [`phase05.rs`](../../../../../crates/codepact/tests/phase05.rs). |
| V07-06–07 | Result and Option exhaustiveness, joins, guarded and fallback arms, unreachable arms, constructor shadowing, open and unsupported scrutinees, incomplete-coverage policy, contradictory annotations, and JSON/text parity: `phase07_*` tests in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). |
| V07-08 | Resource payload boundaries across `Ok`, `Err`, and `Some`, including assignment, extraction, return, forwarding, and discard, with ordinary payload controls: `phase07_outcome_*` tests in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). Existing ownership and concurrency suites remain part of the complete workspace run. |
| V07-09 | Helper-only, CLI-extra, standalone CLI, wheel and source routes, exact extra pin, migration replay, and installed typing checks: [`check_python_package.py`](../../../../../scripts/check_python_package.py), repeated in the hosted Quality matrix. Native wheel builds and target smokes are in the non-publishing Release workflow. |
| V07-10 | A built provider wheel installed outside its checkout, supported declaration forms and re-exports, version and baseline behavior, disabled-rule and shadow controls, unresolved-provider coverage, and a no-execution sentinel: [`installed-provider.json`](installed-provider.json). CLI/Rust result parity and provider selection order are covered in `phase07_provider_*` tests in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). |
| V07-11 | Uniform `Owned[T]` handoff guidance, borrowed/closed/mixed/unresolved/finally controls, closing and forwarding hints, related spans, and advisory/strict exit policy: `phase07_owned_return_*` and `phase07_review_hints_*` tests in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). |
| V07-12 | Fresh/reused result parity, affected query counts, unrelated query reuse, cancellation recovery, and provider, adapter, cancellation, and hint evidence edits: [`query-invalidation.json`](query-invalidation.json), [`adapter-invalidation.json`](adapter-invalidation.json), and [`hint-invalidation.json`](hint-invalidation.json); the async cancellation-summary edit is covered by `async_cancelled_outcome_edits_refresh_only_affected_flow_and_match_fresh` in [`db.rs`](../../../../../crates/codepact/src/db.rs). Provider root precedence is checked with both root orders by `phase07_provider_selection_skips_namespace_portions_and_unrelated_missing_records` in [`cli.rs`](../../../../../crates/codepact/tests/cli.rs). Root order is resolved before the Rust session receives provider sources, so the session evidence covers replacement of the resolved provider summary rather than mutating a root list inside one session. |
| V07-13 | Optimized comparison with the 0.6.0 baseline and added finite-match/provider workloads, including timings, process memory, input sizes, discovered files, scopes, diagnostics, and coverage: [`performance.md`](performance.md) and [`performance.json`](performance.json). |

## Local check results

- `make check-rust` passed on implementation revision `be8691f`, including
  formatting, check, Clippy with warnings denied, and the complete Rust unit
  and integration suites.
- `UV_CACHE_DIR=/private/tmp/typepact-uv-cache uv run --locked make check-python`
  passed on CPython 3.11.14: 66 runtime/release tests (five version-pinned
  skips), Basedpyright, Mypy, Pyright, Ruff, and 63 synthetic static-tool
  project variants.
- The installed-provider wheel fixture passed locally with a real wheel built
  by setuptools, installed into the repository virtual environment, and
  analyzed by the release CLI. Its exact output is `installed-provider.json`.
- Schema and documentation checks passed directly: 12 schemas, 17
  valid/invalid fixtures, 23 CLI JSON outputs, relative-link checks across 131
  documentation files, and a strict Sphinx HTML build. The `make check-docs`
  wrapper could not fetch its
  pinned documentation dependency in this network-restricted local
  environment; hosted Quality repeats the documentation build and replay.
- The complete local package wrapper could not be replayed after its latest
  helper change because the local interpreter lacks the Rust build backend and
  external package-index DNS is unavailable. The hosted Quality run performs
  the full Python-version package, upgrade, and installed-provider checks.

## Hosted checks

The exact-evidence-candidate hosted Quality and non-publishing Release run IDs,
results, and artifact digests are recorded in the
[release evidence record](../../../../releases/0.7.0-evidence.md). Quality
must pass the Rust suite, dependency audit, all four Python-version jobs, and
documentation build/replay. Release must pass matching-version checks, all
eight native CLI wheel builds and target smokes, source archive builds,
license/inventory and metadata checks, isolated installs, and final artifact
validation. The branch Release workflow does not publish artifacts. Registry
publication, maintainer review, and tag approval remain outside this technical
qualification record.


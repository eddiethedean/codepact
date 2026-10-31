# Testing, performance and quality plan

This is the verification strategy for the published inspector and the unreleased lifecycle preview. Local Rust and CLI fixtures cover core flow, resolution, incremental reuse, cancellation, budgets and conservative boundaries. The V02 matrix is mapped to automation below, while full candidate evidence and review remain pending.

## Correctness layers

| Layer | Required evidence |
| --- | --- |
| Parser and resolver | Ruff AST/ranges, syntax versions, recovery, scopes, binding events, captured aliases, imports and cycles |
| Salsa queries | Fresh/reused parity; no-op and unrelated-query reuse; exports, missing modules, settings, revisions and budget invalidation |
| Contract lowering | Canonical snapshots from both APIs, conflicts and alias resolution |
| Flow engine | Transfer/join unit cases; loops, exceptions, cleanup and returns |
| Diagnostics | Golden outputs with codes, spans, related sites and certainty |
| Interoperability | Pythonic producer / explicit consumer and reverse combinations |
| Runtime API | Identity, signatures, pattern matching, conversion and exception behavior |
| Packaging | Clean install, import, CLI, wheel contents and typing markers |
| Compatibility | Pinned checker and framework fixtures per published matrix |

## Essential regression corpus

Cover use before/after close; alias close; conditional close; close raising; idempotent close; early return; cleanup in finally; context exit failure; resource escape; unknown call; move followed by exception; double move; overlapping borrows; returned closure; await/yield boundaries; guarded match; shadowed API names; malicious annotation expressions.

Every reported bug should add the smallest relevant fixture and, where meaningful, its cross-interface equivalent.

## Properties and differential checks

Test that equivalent syntax lowers to equivalent semantic facts, renaming locals preserves results, diagnostic order is deterministic, and cached versus fresh analysis agrees. Adding an unknown path must not strengthen certainty. Use generated small CFGs to test joins and termination.

Ordinary checkers validate base typing compatibility, not TypePact ownership semantics. Differential results are evidence to investigate, not a substitute oracle for new rules.

## Performance

Create a versioned corpus with small modules, large dependency graphs, alias-heavy functions and recursive summaries. Record hardware, OS, analyzer revision, corpus revision, cold/warm cache state, elapsed time, peak memory and percentile incremental latency.

Proposed engineering budget after baseline: flag over 10% median regression on repeated controlled runs and investigate before release. Set absolute latency targets only after M1 measurements. Benchmark runtime import separately from analyzer execution and optional runtime enforcement.

## CI and release gates

Pull requests run formatting, static checks, targeted unit/integration tests, documentation links and compatibility fixtures for changed surfaces. Scheduled/release jobs run the full platform matrix, fuzzing budgets, wheel smoke checks and benchmarks.

Fuzz parser and IR inputs with bounded resources; failures must produce diagnostics rather than crashes. Enforce no target-code execution using fixtures with side effects that would be visible if imported.

A release requires no unresolved critical soundness regression in claimed scope, no unexplained interoperability mismatch, and a reviewed compatibility report. Publish known failures and unsupported cases explicitly.

## Analyzer migration qualification

0.1 requires [V01–V24](phases/0.1/validation.md). Preserve regression cases for multiline declarations, quote-like string/comment contents, defaults/body calls, every annotation position, metadata conflicts, imported lookalikes, cross-file isolation, captured aliases and repeated class names. A valid 24,000-alias source below the byte limit must terminate without stack overflow; actual limits must be enforced before traversal.

Compare fresh and incrementally updated sessions after each edit, including an unchanged request, unrelated-file edit, exported-contract change, body-only edit, moved source spans, missing import creation, deletion, settings/target/model change and cancelled request. Use Salsa execution events to establish reuse separately from parity. Never weaken an assertion to unrelated names to make a fixture pass.

Record cold-session, warm no-op, local-edit and dependency-edit measurements separately. Disk-cache and editor cancellation stress tests expand this corpus in 0.8; basic in-memory query correctness is required from 0.1.

The current source suite has 31 Rust unit tests, 22 lifecycle CLI integration tests, 60 semantic fixtures and 8 Python tests. It covers provider body and model edits, missing-provider addition/removal, moved spans, no-op reuse, session revisions and cancellation recovery. The clean Cargo archive and installed Python wheel/sdist checks now also run the lifecycle example, assert one definite TP1001 with a related close location, and verify inspected source is not executed. These are implementation regressions; they do not replace the exact-candidate V02 evidence, compatibility review or release decision.

## Python helper and CLI distributions

Packaging checks require a pure helper wheel declaring the exact native typepact-cli dependency, eight CLI platform wheels and separate helper/CLI source archives. The local make check gate builds both packages through their sdists and installs all four local artifacts into fresh uv environments outside the checkout, then verifies CLI version/help, inspection, lifecycle checking and source non-execution. Release CI builds all eight targets and validates inventory/metadata. Full candidate platform qualification still requires recorded evidence.

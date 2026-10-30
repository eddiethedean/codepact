# Testing, performance and quality plan

This is the future implementation test strategy. The planning commit does not implement or execute these suites.

## Correctness layers

| Layer | Required evidence |
| --- | --- |
| Parser and resolver | Syntax versions, recovery, scopes, shadowing, imports and cycles |
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

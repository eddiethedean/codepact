# Phase 0.8 implementation evidence

The 0.8.0 implementation is in the working tree based on `245d64332f78b11f311384e491d0640dfef0fe09`. The comparison base is the published `v0.7.0` at `1b46d613ebdc2eba8e250f579144e336b7be7286`. The release checklist needs an exact candidate revision, the full distribution matrix and maintainer acceptance. [Phase gates](../0.8-reviewed-concurrency-foundations.md)

## Runtime and analysis

- Local checks passed for all 670 Rust tests, Rust 1.96.0 compilation, Clippy and dependency policy. Python checks covered the unit suite, typing oracles and 63 synthetic projects. The documentation build validated 16 schemas against 25 fixtures and 25 emitted CLI reports.
- The controlled conformance runner exercises the exact CPython builds. The Linux x86_64 runs cover 12 probes each on 3.11.15 and 3.12.13, and 33 on 3.13.16. The 16 Phase 0.8 probes cover file cleanup, scheduling rejection, eager execution, task cancellation and shielding, and explicit-spawn Process and Pool behavior. Each receipt includes interpreter identity, source digests and executed test IDs. [Conformance registry](../../../../tests/conformance/runtime-models.json)
- Phase 0.8 Rust regressions exercise the file pack, task identity and shielded timeout, Process and Pool shutdown, provider aliases and artifact edits, lock and model updates, and argument failure on both sides of an evaluated move. The Salsa regression counts provider and flow query executions across an unchanged check and a provider upgrade. [Rust fixtures](../../../../crates/codepact/tests/phase08.rs)
- The provider artifact replay uses the installed `codepact` executable. It builds separate provider and caller wheels, compares inline and manifest diagnostics and coverage, verifies the archive and its `RECORD`, and checks that publisher and caller execution sentinels stay absent. The replay also checks an unmarked wheel with matching, mismatching and ambiguous lock entries. [Artifact workflow](../../../../scripts/check_phase08_artifacts.py)
- Local distribution checks passed on macOS ARM64 for clean helper-only, standalone CLI, exact CLI extra and source-archive installs. The local Cargo archive was unpacked, tested, installed and smoke-checked. The distribution matrix for the exact release candidate remains pending.

The per-run receipts are written under `target/phase08-linux` and, when `CODEPACT_PHASE08_ARTIFACT_EVIDENCE` is set, to the requested file for the installed-package check. The artifact receipt includes the installed CLI and wheel hashes, rejected inputs and selected lock and model digests.

## Comparison and outcome decisions

The version 1 comparison descriptor pins engine and rule semantics, finding identity, source target, preview settings, catalog, normalization and preservation-policy revisions. Each snapshot includes the source digests, runtime assumptions, provider and model identity, and reached scope limits. A source edit can preserve the descriptor. Lost provider applicability or caller scope, exhausted required evidence and incomparable semantics preclude a preservation pass. [CR01 comparison design](../../change-review.md) · [Evidence schema](../../../schemas/foundations-evidence-v1.schema.json)

For OH01, a supported marked return delivers one outcome duty when the call returns successfully. Delivery forwards the existing pending identity or renews one outward duty. A raise before return delivers none. Unmarked outcomes and immediate `MustUse` retain their released behavior. The first declaration position is the outer return type of a callable with a resolved CodePact `Result` or `Option` and ordinary payloads. [OH01 outcome design](../../persistent-outcomes.md)

## Performance method

Build the optimized `benchmark_phase06` example from `v0.7.0` and this working tree and build `benchmark_phase08` from this tree. `scripts/capture_phase08_performance.py` alternates the baseline and implementation for the inherited state, exclusive and executor workloads. The manifest workload checks a 64-function caller, an unchanged session, a provider upgrade with unchanged caller bytes and an independent file edit. Each check after an edit is verified against a fresh session. The capture records process peak RSS, raw timing samples and benchmark executable digests. [Benchmark capture](../../../../scripts/capture_phase08_performance.py)

On macOS ARM64 with Rust 1.96.0, the first capture measured an unchanged exclusive check at 10.71 µs for 0.7 and 11.75 µs for 0.8. Inherited cold checks improved by 1.0–2.0% by paired median. Peak process RSS increased by 0.67–1.70 MiB. The installed-manifest case has the following local medians.

| Cold | Unchanged | Provider upgrade | Independent edit | Peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 4.27 ms | 0.28 ms | 3.85 ms | 0.13 ms | 9.73 MiB |

The 20-pair exclusive replay measured a median increase of 1.00 µs on the unchanged check, with a median paired increase of 14.9%. The implementation was slower in 15 pairs. Raw results are in `target/phase08-performance.json` and `target/phase08-exclusive-replay.json`.

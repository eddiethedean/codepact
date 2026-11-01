# 0.3 candidate performance record

Status: **Measured; maintainer acceptance and release qualification are pending.**
This record measures implementation commit
[`6adb344776d26a9204702a86128c534dcf9a4963`](https://github.com/eddiethedean/typepact/commit/6adb344776d26a9204702a86128c534dcf9a4963)
on 2026-11-01. No performance gate is marked passed by these measurements.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64 (Darwin 25.5.0).
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.11.15 for package tooling.
- Analyzer target: Python 3.11.
- Session timing: seven cold sessions and seven warm/edit scenarios; report medians and ranges. Timings are in-process `AnalysisSession` calls and exclude process startup.
- The lifecycle benchmark checks N distinct resources across two source files. A provider helper closes each resource; the benchmark verifies the expected diagnostic count after each update.
- The phase-specific benchmark measures N generated sync functions, each with two resources and nested `try`/`except`/`else`/`finally`, `while`, and two `contextlib.closing` managers; a second corpus measures N async functions with direct awaited close and cancellation uncertainty. Each run verifies its diagnostics and coverage.
- Generator SHA256: `benchmark_lifecycle.rs` `c16de0ff9518f38b9fd53e3928f21d4d6db2ca1a002f3f475243a831c8835cff`; `benchmark_phase03.rs` `ceb6e42df4f1a7f69093282c30a3c4c547c1bc2ced959ea7e05538ea0ae76739`.

## Cleanup-path workloads

Milliseconds, seven repeats. N is the number of generated functions; cold is a new session and warm is a repeated no-op check on the same session.

| N | Sync cold | Sync warm | Sync source bytes | Async cold | Async warm | Async source bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.351 | 0.004 | 523 | 0.112 | 0.004 | 310 |
| 16 | 1.493 | 0.004 | 4,594 | 0.634 | 0.016 | 1,576 |
| 64 | 12.855 | 0.004 | 17,650 | 4.725 | 0.055 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.296 (0.273–1.903) | 0.009 (0.009–0.011) | 0.133 (0.118–0.311) | 0.115 (0.112–0.136) | 0.103 (0.100–0.112) |
| 250 | 13.265 (13.040–14.388) | 0.397 (0.393–0.414) | 11.120 (11.027–11.252) | 12.378 (12.344–12.760) | 10.596 (10.345–10.769) |
| 1,000 | 150.341 (148.181–152.759) | 1.544 (1.535–1.567) | 128.242 (126.345–131.542) | 132.037 (130.806–135.641) | 126.260 (124.116–129.867) |

The 1,000-resource source corpus is 78,891 bytes. Maximum resident set size was 39,960,576 bytes (about 38.1 MiB). With the diagnostics dropped and the analysis session retained, the measured RSS increase was 20,048 KiB (about 19.6 MiB).

Compared with the 0.2 accepted baseline, the 1,000-resource cold median is 2.40× higher; unrelated body and location edits are about 3.08× higher, and imported-effect edits are about 2.81× higher. Warm no-op remains similar (1.544 ms vs 1.442 ms). The same candidate is about 64% faster cold and 66–68% faster on edits than base commit [`6a29c07b7d36e48841a976c79d4b482747be52bd`](https://github.com/eddiethedean/typepact/commit/6a29c07b7d36e48841a976c79d4b482747be52bd), which measured 414.084 ms cold at 1,000 resources. The improvement over the phase base does not remove the regression against the released 0.2 baseline.

The phase plan defines no absolute latency threshold. The maintainer must decide whether this regression is acceptable or whether further optimization is required before release qualification. No independent reviewer is needed.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 3.313 ms median (2.699–450.163), 1,000 classes 83.266 ms (82.540–86.169), helper import 22.334 ms (21.436–57.617), maximum child RSS 18.59 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

Query tests assert reuse independently of timing: `unchanged_session_check_reuses_the_check_query`, `unrelated_edit_reuses_other_file_lifecycle_checks`, `imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, `moving_an_imported_close_refreshes_the_related_lifecycle_span`, and `v03_22_incremental_body_edit_matches_a_fresh_check`.

## Budgets and reproduction

| Dimension | Candidate bound | Exhaustion behavior |
| --- | ---: | --- |
| Loop fixed-point passes | 16 | Widen affected state and retain incomplete coverage |
| Static `for` tuple items | 16 | Preserve possible remaining iterations and report a boundary |
| Pending coroutine records | 128 | Weaken captured objects, lose exact aliases and report a boundary |
| Nested control-flow depth | 64 | Weaken affected resources and report a boundary |
| Possible alias targets | 64 | Drop exact targets, weaken candidates and report a boundary |
| Tracked resource objects per body | 4,096 | Do not create an exact place; report a boundary |
| Nested call/effect evaluation | 8 | Weaken affected resources and report a boundary |
| Flow transfers per request | 100,000 | Stop bounded work with visible incomplete coverage |

Reproduce from the repository root:

```sh
cargo build --release --locked -p typepact --bins --examples
target/release/examples/benchmark_phase03 1
target/release/examples/benchmark_phase03 16
target/release/examples/benchmark_phase03 64
target/release/examples/benchmark_lifecycle 1
target/release/examples/benchmark_lifecycle 250
target/release/examples/benchmark_lifecycle 1000
/usr/bin/time -l target/release/examples/benchmark_lifecycle 1000
TYPEPACT_RETAINED_MEMORY_PROBE=1 target/release/examples/benchmark_lifecycle 1000
uv run --locked python scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000
```

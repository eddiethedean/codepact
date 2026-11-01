# 0.3 candidate performance record

Status: **Measured; maintainer acceptance and release qualification are pending.**
This record measures implementation commit
[`85e0c7d2dfe5aea3f078f4bf560131a6738e7491`](https://github.com/eddiethedean/typepact/commit/85e0c7d2dfe5aea3f078f4bf560131a6738e7491)
on 2026-11-01. No performance gate is marked passed by these measurements.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64 (Darwin 25.5.0).
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.11.15 for package tooling.
- Analyzer target: Python 3.11.
- Session timing: seven cold sessions and seven warm/edit scenarios; report medians and ranges. Timings are in-process `AnalysisSession` calls and exclude process startup.
- The lifecycle benchmark checks N distinct resources across two source files. A provider helper closes each resource; the benchmark verifies the expected diagnostic count after each update.
- The phase-specific benchmark measures N generated sync functions, each with two resources and nested `try`/`except`/`else`/`finally`, `while`, and two module-qualified `contextlib.closing` managers; a second corpus measures N async functions with direct awaited close and cancellation uncertainty. Each run verifies its diagnostics and coverage.
- Generator SHA256: `benchmark_lifecycle.rs` `c16de0ff9518f38b9fd53e3928f21d4d6db2ca1a002f3f475243a831c8835cff`; `benchmark_phase03.rs` `b070dbb22294f4199421a3812558d0cb4afe84bec13f0202b856b8f9b230af7c`.

## Cleanup-path workloads

Milliseconds, seven repeats. N is the number of generated functions; cold is a new session and warm is a repeated no-op check on the same session.

| N | Sync cold | Sync warm | Sync source bytes | Async cold | Async warm | Async source bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.427 | 0.008 | 525 | 0.253 | 0.010 | 310 |
| 16 | 2.041 | 0.005 | 4,716 | 0.750 | 0.019 | 1,576 |
| 64 | 12.860 | 0.004 | 18,156 | 4.762 | 0.055 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.307 (0.273–2.052) | 0.005 (0.005–0.013) | 0.129 (0.112–0.181) | 0.115 (0.108–0.127) | 0.100 (0.098–0.107) |
| 250 | 13.124 (12.861–19.237) | 0.396 (0.393–0.407) | 11.043 (10.689–11.166) | 12.666 (12.508–13.057) | 10.829 (10.756–10.875) |
| 1,000 | 150.425 (147.786–160.226) | 1.552 (1.530–1.582) | 128.200 (126.623–128.328) | 133.420 (133.237–134.020) | 126.979 (125.813–128.036) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 39,780,352 bytes (37.94 MiB). A retained-session probe, with diagnostics dropped, measured an RSS increase of 19,488 KiB (19.03 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the 0.2 accepted baseline, the 1,000-resource cold median is 2.40× higher; unrelated body and location edits are 3.08× and 3.10× higher, and imported-effect edits are 2.84× higher. Warm no-op remains similar (1.552 ms vs 1.442 ms). Cold and edit timings are within 0.4% of the previous 0.3 candidate; warm no-op is 1.6% higher. The candidate remains about 64% faster cold and 66–68% faster on edits than base commit [`6a29c07b7d36e48841a976c79d4b482747be52bd`](https://github.com/eddiethedean/typepact/commit/6a29c07b7d36e48841a976c79d4b482747be52bd), which measured 414.084 ms cold at 1,000 resources. The improvement over the phase base does not remove the regression against the released 0.2 baseline.

The phase plan defines no absolute latency threshold. The maintainer must decide whether this regression is acceptable or whether further optimization is required before release qualification. No independent reviewer is needed.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 2.938 ms median (2.600–454.391), 1,000 classes 84.383 ms (83.697–85.226), helper import 24.171 ms (23.905–25.316), maximum child RSS 20.42 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

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

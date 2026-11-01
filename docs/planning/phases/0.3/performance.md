# 0.3 candidate performance record

Status: **Measured; maintainer acceptance and release qualification are pending.**
This record measures implementation commit
[`0eb94cf142093c4aa82c657ed8523150c38247a0`](https://github.com/eddiethedean/typepact/commit/0eb94cf142093c4aa82c657ed8523150c38247a0)
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
| 1 | 0.376 | 0.008 | 525 | 0.262 | 0.010 | 310 |
| 16 | 2.131 | 0.005 | 4,716 | 0.782 | 0.019 | 1,576 |
| 64 | 13.061 | 0.004 | 18,156 | 4.784 | 0.055 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.294 (0.259–1.800) | 0.009 (0.009–0.012) | 0.253 (0.247–0.299) | 0.113 (0.101–0.174) | 0.097 (0.091–0.103) |
| 250 | 12.960 (12.464–13.958) | 0.382 (0.381–0.392) | 10.719 (10.605–10.852) | 12.384 (12.326–12.448) | 10.614 (10.594–10.720) |
| 1,000 | 150.896 (150.433–151.783) | 1.528 (1.524–1.568) | 128.256 (127.504–128.651) | 132.921 (131.438–134.595) | 127.215 (126.445–127.941) |

The 1,000-resource source corpus is 78,891 bytes. Three `/usr/bin/time -l` samples measured maximum resident set between 40,075,264 and 51,838,976 bytes (about 38.2–49.4 MiB). Two retained-session probes, with diagnostics dropped, measured RSS increases of 19,584–19,648 KiB (about 19.1–19.2 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the 0.2 accepted baseline, the 1,000-resource cold median is 2.41× higher; unrelated body and location edits are 3.08× and 3.11× higher, and imported-effect edits are 2.82× higher. Warm no-op remains similar (1.528 ms vs 1.442 ms). These measurements are within about 1% of the previous 0.3 candidate for the 1,000-resource timing scenarios. The candidate remains about 64% faster cold and 66–68% faster on edits than base commit [`6a29c07b7d36e48841a976c79d4b482747be52bd`](https://github.com/eddiethedean/typepact/commit/6a29c07b7d36e48841a976c79d4b482747be52bd), which measured 414.084 ms cold at 1,000 resources. The improvement over the phase base does not remove the regression against the released 0.2 baseline.

The phase plan defines no absolute latency threshold. The maintainer must decide whether this regression is acceptable or whether further optimization is required before release qualification. No independent reviewer is needed.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 2.964 ms median (2.544–339.179), 1,000 classes 83.908 ms (82.219–85.196), helper import 21.193 ms (20.177–21.959), maximum child RSS 18.45 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

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

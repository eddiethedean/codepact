# 0.3 candidate performance record

Status: **Measured on the current phase 0.3 implementation candidate; maintainer performance acceptance and release qualification are pending.**
Measurements were collected on 2026-11-01 after the exception-routing and cancellation-state fixes documented in the [validation matrix](validation.md). The benchmark source and workloads were unchanged. The current values below replace the earlier candidate measurements; they do not pass a performance or qualification gate.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64 (Darwin 25.5.0).
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.14.3 for inspector/package tooling.
- Analyzer target: Python 3.11.
- Session timing: seven cold sessions and seven warm/edit scenarios; report medians and ranges. Timings are in-process `AnalysisSession` calls and exclude process startup.
- The lifecycle benchmark checks N distinct resources across two source files. A provider helper closes each resource; the benchmark verifies the expected diagnostic count after each update.
- The phase-specific benchmark measures N generated sync functions, each with two resources and nested `try`/`except`/`else`/`finally`, `while`, and two module-qualified `contextlib.closing` managers; a second corpus measures N async functions with direct awaited close and cancellation uncertainty. Each run verifies its diagnostics and coverage.
- Generator SHA256: `benchmark_lifecycle.rs` `c16de0ff9518f38b9fd53e3928f21d4d6db2ca1a002f3f475243a831c8835cff`; `benchmark_phase03.rs` `b070dbb22294f4199421a3812558d0cb4afe84bec13f0202b856b8f9b230af7c`.

## Cleanup-path workloads

Milliseconds, seven repeats. N is the number of generated functions; cold is a new session and warm is a repeated no-op check on the same session.

| N | Sync cold | Sync warm | Sync source bytes | Async cold | Async warm | Async source bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.362 | 0.008 | 525 | 0.257 | 0.005 | 310 |
| 16 | 1.601 | 0.004 | 4,716 | 0.638 | 0.017 | 1,576 |
| 64 | 13.377 | 0.004 | 18,156 | 5.065 | 0.062 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.299 (0.275–1.988) | 0.010 (0.009–0.017) | 0.267 (0.134–0.287) | 0.115 (0.111–0.148) | 0.105 (0.101–0.113) |
| 250 | 13.037 (12.942–13.958) | 0.364 (0.362–0.373) | 10.991 (10.951–11.106) | 12.601 (12.563–12.774) | 11.058 (10.917–11.163) |
| 1,000 | 150.509 (149.464–152.534) | 1.458 (1.447–1.501) | 128.478 (127.004–130.606) | 134.056 (129.320–135.526) | 125.995 (123.736–129.068) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 49,152,000 bytes (46.88 MiB). A retained-session probe, with diagnostics dropped, measured baseline RSS of 1,952 KiB and retained RSS of 22,000 KiB, an increase of 20,048 KiB (19.58 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the accepted 0.2 baseline, the 1,000-resource cold median is 2.40× higher; unrelated body, imported-effect and location edits are 3.09×, 2.85× and 3.08× higher. Warm no-op is 1.01× the 0.2 time (1.458 ms vs 1.442 ms). At 250 resources, cold time is 1.51× and the edit scenarios are 1.77×–1.99× higher. This regression remains material and requires the maintainer's explicit acceptance or further optimization before release qualification.

The phase plan defines no absolute latency threshold. The maintainer must decide whether this regression is acceptable or whether further optimization is required before release qualification. No independent reviewer is needed.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 2.782 ms median (2.650–441.385), 1,000 classes 84.480 ms (83.157–86.170), helper import 23.753 ms (23.636–26.305), maximum child RSS 20.39 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

Query tests assert reuse independently of timing: `unchanged_session_check_reuses_the_check_query`, `unrelated_edit_reuses_other_file_lifecycle_checks`, `imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, `moving_an_imported_close_refreshes_the_related_lifecycle_span`, `resource_model_edits_invalidate_reused_cleanup_queries`, `reviewed_exceptional_close_outcomes_flow_through_session_results`, and `v03_22_incremental_body_edit_matches_a_fresh_check`.

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

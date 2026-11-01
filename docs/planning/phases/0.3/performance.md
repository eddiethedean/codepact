# 0.3 candidate performance record

Status: **Measured on the optimized phase 0.3 implementation candidate; maintainer performance review and release qualification are pending.**
Measurements were collected on 2026-11-01 after optimizing resolved-call state transfer and source-span lookup. The benchmark source and workloads were unchanged. The values below replace the earlier candidate measurements; they do not pass a qualification gate.

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
| 1 | 0.350 | 0.008 | 525 | 0.255 | 0.010 | 310 |
| 16 | 1.464 | 0.004 | 4,716 | 0.628 | 0.016 | 1,576 |
| 64 | 12.354 | 0.004 | 18,156 | 4.656 | 0.060 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.327 (0.285–2.148) | 0.010 (0.009–0.017) | 0.133 (0.116–0.312) | 0.117 (0.115–0.256) | 0.104 (0.102–0.108) |
| 250 | 6.099 (6.064–6.669) | 0.365 (0.363–0.374) | 5.334 (5.286–5.473) | 6.971 (6.948–7.002) | 5.137 (5.121–5.216) |
| 1,000 | 29.221 (28.905–30.889) | 1.436 (1.426–1.466) | 26.216 (26.107–26.571) | 33.273 (33.078–33.753) | 25.701 (25.540–25.983) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 39,796,736 bytes (37.96 MiB). A retained-session probe, with diagnostics dropped, measured baseline RSS of 1,952 KiB and retained RSS of 22,416 KiB, an increase of 20,464 KiB (19.98 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the accepted 0.2 baseline, the 1,000-resource cold median is 0.47×; unrelated body, imported-effect and location edits are 0.63×, 0.71× and 0.63×. Warm no-op is effectively unchanged at 1.00× (1.436 ms vs 1.442 ms). At 250 resources, cold time is 0.71×, warm is 1.01× and edit scenarios range from 0.92× to 0.98×. For one resource the absolute medians are small (0.10–0.33 ms), but cold and unrelated-body samples are noisy; the maintainer should consider that small-corpus spread in the G4 review. The phase plan defines no absolute latency threshold.

Resolved-call transfer now visits only caller-reachable keys in the callee result instead of cloning every caller resource for every helper call. Scoped newline indexes remove repeated source-prefix scans on sources at least 1 KiB; small sources retain the direct scan. These changes reduced the 1,000-resource cold median from 150.5 ms to 29.2 ms and 1,000-resource edit medians from 126–134 ms to 25.7–33.3 ms. The maintainer may review and accept this evidence alone; no independent reviewer is required.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 2.926 ms median (2.774–511.143), 1,000 classes 49.640 ms (47.796–50.208), helper import 24.128 ms (23.868–25.202), maximum child RSS 20.36 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

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

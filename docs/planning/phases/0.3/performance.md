# 0.3 candidate performance record

Status: **Measured on the optimized phase 0.3 implementation candidate; maintainer performance review and release qualification are pending.**
Measurements were recollected on 2026-11-01 after bounding context-manager exit partitions and enforcing the object/duty cap for resource parameters. The benchmark source and workloads were unchanged from the run after optimizing resolved-call state transfer and source-span lookup. The values below replace the earlier candidate measurements; they do not pass a qualification gate.

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
| 1 | 0.404 | 0.004 | 525 | 0.114 | 0.005 | 310 |
| 16 | 1.475 | 0.004 | 4,716 | 0.639 | 0.016 | 1,576 |
| 64 | 12.298 | 0.004 | 18,156 | 4.700 | 0.060 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.329 (0.271–2.051) | 0.009 (0.009–0.011) | 0.133 (0.115–0.289) | 0.116 (0.114–0.144) | 0.106 (0.101–0.107) |
| 250 | 6.117 (6.095–6.643) | 0.375 (0.368–0.389) | 5.393 (5.349–5.479) | 7.061 (6.873–7.162) | 5.262 (5.186–5.279) |
| 1,000 | 28.822 (28.654–30.807) | 1.413 (1.411–1.429) | 25.899 (25.752–26.232) | 33.755 (33.526–33.932) | 26.223 (26.111–26.651) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 39,616,512 bytes (37.78 MiB). A retained-session probe, with diagnostics dropped, measured baseline RSS of 1,936 KiB and retained RSS of 21,488 KiB, an increase of 19,552 KiB (19.09 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the accepted 0.2 baseline, the 1,000-resource cold median is 0.46×; unrelated body, imported-effect and location edits are 0.62×, 0.72× and 0.64×. Warm no-op is effectively unchanged at 0.98× (1.413 ms vs 1.442 ms). At 250 resources, cold time is 0.71×, warm is 1.04× and edit scenarios range from 0.95× to 0.99×. For one resource the absolute medians are small, but cold samples are noisy; the maintainer should consider that small-corpus spread in the G4 review. The phase plan defines no absolute latency threshold.

Resolved-call transfer now visits only caller-reachable keys in the callee result instead of cloning every caller resource for every helper call. Scoped newline indexes remove repeated source-prefix scans on sources at least 1 KiB; small sources retain the direct scan. These changes reduced the 1,000-resource cold median from 150.5 ms to 28.8 ms and 1,000-resource edit medians from 126–134 ms to 25.9–33.8 ms. The maintainer may review and accept this evidence alone; no independent reviewer is required.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/codepact --repeats 7 --classes 1000`: small CLI 3.000 ms median (2.624–491.295), 1,000 classes 48.737 ms (47.450–54.047), helper import 23.893 ms (23.653–25.993), maximum child RSS 20.44 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

Query tests assert reuse independently of timing: `unchanged_session_check_reuses_the_check_query`, `unrelated_edit_reuses_other_file_lifecycle_checks`, `imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, `moving_an_imported_close_refreshes_the_related_lifecycle_span`, `resource_model_edits_invalidate_reused_cleanup_queries`, `reviewed_exceptional_close_outcomes_flow_through_session_results`, and `v03_22_incremental_body_edit_matches_a_fresh_check`.

## Budgets and reproduction

| Dimension | Candidate bound | Exhaustion behavior |
| --- | ---: | --- |
| Loop fixed-point passes | 16 | Widen affected state and retain incomplete coverage |
| Static `for` tuple items | 16 | Preserve possible remaining iterations and report a boundary |
| Exit partitions per context-manager unwind | 256 | Coalesce excess outcomes by completion, weaken tracked resources and report a boundary |
| Pending coroutine records | 128 | Weaken captured objects, lose exact aliases and report a boundary |
| Nested control-flow depth | 64 | Weaken affected resources and report a boundary |
| Possible alias targets | 64 | Drop exact targets, weaken candidates and report a boundary |
| Tracked resource objects and cleanup duties per body | 4,096 | Do not create an exact place or duty; report a boundary |
| Nested call/effect evaluation | 8 | Weaken affected resources and report a boundary |
| Flow transfers per request | 100,000 | Stop bounded work with visible incomplete coverage |

Reproduce from the repository root:

```sh
cargo build --release --locked -p codepact --bins --examples
target/release/examples/benchmark_phase03 1
target/release/examples/benchmark_phase03 16
target/release/examples/benchmark_phase03 64
target/release/examples/benchmark_lifecycle 1
target/release/examples/benchmark_lifecycle 250
target/release/examples/benchmark_lifecycle 1000
/usr/bin/time -l target/release/examples/benchmark_lifecycle 1000
CODEPACT_RETAINED_MEMORY_PROBE=1 target/release/examples/benchmark_lifecycle 1000
uv run --locked python scripts/benchmark_inspector.py --binary target/release/codepact --repeats 7 --classes 1000
```

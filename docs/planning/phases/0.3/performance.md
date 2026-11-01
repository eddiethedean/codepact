# 0.3 candidate performance record

Status: **Measured for the current candidate; maintainer acceptance and release qualification are pending.**
This record measures candidate commit
[`c97a7b58a6cd34e41816f61bdb8d1aa8180c34b1`](https://github.com/eddiethedean/typepact/commit/c97a7b58a6cd34e41816f61bdb8d1aa8180c34b1)
on 2026-11-01. No performance gate is marked passed by these measurements.

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
| 1 | 0.416 | 0.008 | 525 | 0.239 | 0.010 | 310 |
| 16 | 2.081 | 0.005 | 4,716 | 0.799 | 0.020 | 1,576 |
| 64 | 12.412 | 0.004 | 18,156 | 4.769 | 0.054 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

## Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.389 (0.259–2.175) | 0.010 (0.009–0.014) | 0.247 (0.243–0.280) | 0.251 (0.242–0.297) | 0.219 (0.217–0.227) |
| 250 | 12.620 (12.311–17.628) | 0.356 (0.355–0.369) | 10.586 (10.561–10.645) | 12.136 (11.827–12.275) | 10.475 (10.455–10.595) |
| 1,000 | 144.656 (143.602–147.377) | 1.386 (1.380–1.441) | 124.099 (123.238–125.453) | 129.563 (128.829–130.065) | 122.982 (122.545–123.818) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 49,184,768 bytes (46.91 MiB). A retained-session probe, with diagnostics dropped, measured baseline RSS of 1,968 KiB and retained RSS of 22,176 KiB, an increase of 20,208 KiB (19.73 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the 0.2 accepted baseline, the 1,000-resource cold median is 2.31× higher; unrelated body, imported-effect and location edits are 2.98×, 2.75× and 3.00× higher. Warm no-op is 0.961× the 0.2 time (1.386 ms vs 1.442 ms). At 250 resources, cold time is 1.46× and the edit scenarios are 1.70×–1.88× higher. Compared with the prior measured 0.3 candidate at [`85e0c7d2dfe5aea3f078f4bf560131a6738e7491`](https://github.com/eddiethedean/typepact/commit/85e0c7d2dfe5aea3f078f4bf560131a6738e7491), the current 1,000-resource cold/edit medians are about 3%–4% lower and warm no-op is 10.7% lower. The candidate remains about 65% faster cold and 67%–69% faster on edits than base commit [`6a29c07b7d36e48841a976c79d4b482747be52bd`](https://github.com/eddiethedean/typepact/commit/6a29c07b7d36e48841a976c79d4b482747be52bd), which measured 414.084 ms cold at 1,000 resources. The improvement over the phase base does not remove the regression against the released 0.2 baseline.

The phase plan defines no absolute latency threshold. The maintainer must decide whether this regression is acceptable or whether further optimization is required before release qualification. No independent reviewer is needed.

## Inspector and query reuse

The inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000`: small CLI 2.751 ms median (2.573–469.877), 1,000 classes 84.601 ms (83.183–85.014), helper import 23.869 ms (22.938–32.537), maximum child RSS 20.39 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking.

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

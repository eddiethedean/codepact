# 0.3 performance record

Status: **Measurements collected and accepted for G4 by the accountable maintainer on 2026-11-08. CodePact 0.3.0 is published.** The historical tables are retained for comparison. Measurements below use the released analyzer implementation, including the four correctness fixes described later. The release qualification and publication are recorded in the release evidence.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64 (Darwin 25.5.0).
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.14.3 for inspector/package tooling.
- Analyzer target: Python 3.11.
- Session timing: seven cold sessions and seven warm/edit scenarios; report medians and ranges. Timings are in-process `AnalysisSession` calls and exclude process startup.
- The lifecycle benchmark checks N distinct resources across two source files. A provider helper closes each resource; the benchmark verifies the expected diagnostic count after each update.
- The phase-specific benchmark measures N generated sync functions, each with two resources and nested `try`/`except`/`else`/`finally`, `while`, and two module-qualified `contextlib.closing` managers; a second corpus measures N async functions with direct awaited close and cancellation uncertainty. Each run verifies its diagnostics and coverage.
- Generator SHA256: `benchmark_lifecycle.rs` `f27f5610b228742cb672ee5c8546117f4fd5e8ed4e5bedce24ed34c3881979ae`; `benchmark_phase03.rs` `bfb061c911826463c64618d4731e7d01e9636ab8139f278ed6131f4f1a58174d`.

## Fixed-candidate measurements

The optimized executables were built with `cargo build --release --locked -p codepact --bins --examples` from the candidate tree at `f13848a6f83a1d294da69ee171a2644047500ed8`. No Rust source changed between that revision and the measurement run. Lifecycle and phase-specific results below report seven-run medians; lifecycle ranges show minimum and maximum. Every workload asserted its expected diagnostics and coverage.

### Cleanup-path workloads

Milliseconds; N is the number of generated functions. Cold is a new session and warm is a repeated no-op check on the same session.

| N | Sync cold | Sync warm | Sync source bytes | Async cold | Async warm | Async source bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.344 | 0.008 | 525 | 0.246 | 0.010 | 310 |
| 16 | 1.806 | 0.004 | 4,716 | 0.812 | 0.018 | 1,576 |
| 64 | 11.707 | 0.004 | 18,156 | 4.552 | 0.055 | 5,656 |

### Lifecycle comparison

Milliseconds, median (minimum–maximum), using the same 1/250/1,000-resource corpus as the earlier baseline:

| Resources | Source bytes | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 294 | 0.376 (0.260–3.143) | 0.010 (0.010–0.014) | 0.258 (0.250–0.323) | 0.261 (0.258–0.326) | 0.228 (0.194–0.267) |
| 250 | 19,641 | 6.896 (6.128–8.518) | 0.352 (0.349–0.363) | 5.284 (5.237–5.373) | 7.403 (7.305–7.806) | 5.192 (4.953–5.347) |
| 1,000 | 78,891 | 29.382 (28.233–30.058) | 1.417 (1.413–1.474) | 26.114 (25.776–26.697) | 34.687 (34.434–34.921) | 25.292 (25.003–26.059) |

A separate `/usr/bin/time -l` sample of the 1,000-resource process reported maximum resident set of 47,005,696 bytes (44.83 MiB). The retained-session probe measured 1,952 KiB baseline RSS and 21,568 KiB after analysis with diagnostics dropped, an increase of 19,616 KiB (19.16 MiB). These process-level readings are coarse and are not allocation counts or memory limits.

The current `scripts/benchmark_inspector.py --binary target/release/codepact --repeats 7 --classes 1000` run used CPython 3.14.3. Small CLI median was 5.304 ms (4.249–558.192), the 1,000-class CLI median was 46.885 ms (46.466–59.848), helper import was 23.159 ms (22.949–41.529), and maximum child RSS across scenarios was 20.44 MiB. The small CLI range includes process-launch outliers.

At 1,000 resources, lifecycle medians are close to the pre-fix run: cold +0.5%, warm -0.9%, unrelated edit -2.7%, imported-effect edit +4.9%, and location-only edit -0.4%. The phase-specific 64-function cold medians are about 5% lower for both sync and async scenarios. These selected local workloads show no material timing change; no absolute latency or retained-memory threshold has been set. The accountable maintainer reviewed the ranges, host scope and budgets and accepted G4 on 2026-11-08. See the [release evidence](../../../releases/0.3.0-evidence.md) for the gate record.

### Query reuse checks

The full local check passed query-reuse regressions including `unchanged_session_check_reuses_the_check_query`, `unrelated_edit_reuses_other_file_lifecycle_checks`, `imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, `moving_an_imported_close_refreshes_the_related_lifecycle_span`, `resource_model_edits_invalidate_reused_cleanup_queries`, `reviewed_exceptional_close_outcomes_flow_through_session_results`, and `v03_22_incremental_body_edit_matches_a_fresh_check`. These tests verify reuse and invalidation separately from the timing results.

## Earlier revision measurements

The following results were measured against analyzer source revision `61dde0eb8d0dcf61c675b9a18671ba9b8f8f7fe5`, before the four correctness fixes. They remain historical comparison data and do not qualify the fixed candidate.

### Cleanup-path workloads

Milliseconds, seven repeats. N is the number of generated functions; cold is a new session and warm is a repeated no-op check on the same session.

| N | Sync cold | Sync warm | Sync source bytes | Async cold | Async warm | Async source bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.172 | 0.004 | 525 | 0.118 | 0.005 | 310 |
| 16 | 1.499 | 0.004 | 4,716 | 0.641 | 0.018 | 1,576 |
| 64 | 12.325 | 0.004 | 18,156 | 4.789 | 0.061 | 5,656 |

These measurements show execution cost for the selected scenarios. They do not qualify manager behavior, cancellation semantics or other Python programs; those require the V03 matrix and reviewed runtime/model evidence.

### Lifecycle comparison

Milliseconds, median (minimum–maximum), same 1/250/1,000-resource corpus and harness as the [0.2 baseline](../0.2/performance.md):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.314 (0.283–2.401) | 0.009 (0.009–0.012) | 0.278 (0.269–0.300) | 0.275 (0.271–0.342) | 0.117 (0.107–0.318) |
| 250 | 6.389 (6.114–10.919) | 0.372 (0.369–0.392) | 5.534 (5.499–5.745) | 7.112 (7.106–7.174) | 5.338 (5.237–5.386) |
| 1,000 | 29.223 (29.174–31.021) | 1.430 (1.426–1.471) | 26.828 (26.690–27.028) | 33.062 (32.837–33.736) | 25.390 (24.922–26.120) |

The 1,000-resource source corpus is 78,891 bytes. One `/usr/bin/time -l` sample measured maximum resident set at 39,731,200 bytes (37.89 MiB). A retained-session probe, with diagnostics dropped, measured baseline RSS of 1,968 KiB and retained RSS of 21,712 KiB, an increase of 19,744 KiB (19.28 MiB). These are coarse process measurements, not allocated-byte counts or memory budgets.

Compared with the accepted 0.2 baseline, the 1,000-resource cold median is 0.47×; unrelated body, imported-effect and location edits are 0.64×, 0.70× and 0.62×. Warm no-op is effectively unchanged at 0.99× (1.430 ms vs 1.442 ms). At 250 resources, cold time is 0.74×, warm is 1.03× and edit scenarios range from 0.96× to 1.00×. For one resource the absolute medians are small, but cold and edit samples are noisy and include process-local outliers; the accountable maintainer reviewed that spread as part of the G4 acceptance on 2026-11-08. The phase plan defines no absolute latency threshold.

Resolved-call transfer now visits only caller-reachable keys in the callee result instead of cloning every caller resource for every helper call. Scoped newline indexes remove repeated source-prefix scans on sources at least 1 KiB; small sources retain the direct scan. These changes reduced the 1,000-resource cold median from 150.5 ms to 29.2 ms and 1,000-resource edit medians from 126–134 ms to 25.9–33.5 ms. The maintainer may review and accept this evidence alone; no independent reviewer is required.

### Inspector results

The historical inspector comparison ran `scripts/benchmark_inspector.py --binary target/release/codepact --repeats 7 --classes 1000`: small CLI 2.639 ms median (2.543–478.791), 1,000 classes 51.493 ms (49.329–52.252), helper import 23.645 ms (23.000–29.504), maximum child RSS 20.39 MiB. The small CLI range includes process-launch outliers. This is a separate workload from lifecycle checking; fixed-candidate inspector results are recorded above.

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

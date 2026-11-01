# 0.2 lifecycle performance baseline

Status: exact-source-candidate replay from 2026-10-31 on
`b7a5236d00cd14c06fe7fd633b6a05d2e64c5856`. The subsequent release-record
follow-up contains documentation only; no analyzer source changed. Reviewer
acceptance of the measured regressions and provisional limits remains required
before G4 or LC01 can pass.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64.
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.11.15.
- Analyzer source target: Python 3.11.
- Lifecycle runs: `cargo run --release --locked -p typepact --example benchmark_lifecycle -- N`.
- Repeats: seven for cold sessions and each warm/edit scenario; table shows median and range.
- Corpus: two source files, one provider and one client. The client constructs N distinct resources and calls a helper that closes each before use. The 1, 250 and 1,000 object corpora contain 294, 19,641 and 78,891 source bytes.
- The benchmark asserts the expected definite or possible finding count before reporting timings. It includes in-process Salsa session time, not process startup.

These measurements replay the exact analyzer source candidate. They do not
establish performance on other platforms or corpora, and they are not accepted
release thresholds until reviewed.

## Lifecycle session timings

Milliseconds, median (minimum–maximum), measured after the exact-singleton
flow-state optimization:

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.118 (0.102–1.640) | 0.005 (0.004–0.006) | 0.082 (0.080–0.110) | 0.091 (0.084–0.098) | 0.082 (0.075–0.083) |
| 250 | 8.142 (7.962–8.457) | 0.381 (0.369–0.398) | 5.940 (5.836–6.153) | 7.432 (7.222–7.503) | 5.778 (5.712–6.094) |
| 1,000 | 66.165 (65.294–69.273) | 1.461 (1.457–1.505) | 43.048 (42.357–43.673) | 49.312 (48.225–50.414) | 42.699 (41.773–43.591) |

The 1,000-resource benchmark process reached 40.16 MiB maximum resident set
size (`/usr/bin/time -l`), across its seven repeats of each scenario. This is a
process high-water measurement, not retained-session memory; retained memory
remains unmeasured.

Compared with the earlier provisional measurements recorded before the latest
correctness fixes, 1,000-resource cold time is
1.57× higher and edit scenarios are 2.18×–2.47× higher; warm no-op time is
unchanged. At 250 resources, cold time is 1.88× higher and edit scenarios are
2.11×–2.43× higher; warm no-op time is slightly lower. The earlier source
version and these later correctness fixes have not received an independent
performance review, and no absolute latency threshold has been accepted. A
release-review profile found full flow-state clones on definite singleton
method calls; evaluating those calls in place reduced the 1,000-resource cold
median from 476.1 ms to 66.2 ms and the 250-resource median from 35.8 ms to
8.1 ms. The remaining difference from the earlier provisional result is
recorded for reviewer assessment rather than treated as a passed threshold.

## Query reuse evidence

The Rust query-event tests separately assert reuse; timing alone does not prove
it:

| Update | Expected flow-check queries |
| --- | ---: |
| Repeated no-op session check | 0 additional query executions |
| Unrelated file edit | 1, only the edited file |
| Imported effect edit | 2, provider and caller; unrelated file reused |
| Imported close location move | 2, provider and caller; related span refreshed |

See `db::tests::unchanged_session_check_reuses_the_check_query`,
`unrelated_edit_reuses_other_file_lifecycle_checks`,
`imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, and
`moving_an_imported_close_refreshes_the_related_lifecycle_span`.

## Inspector comparison with 0.1.0

The existing declaration-inspection corpus v1 was replayed with the candidate
binary on this host. Current medians were 3.079 ms for the small CLI, 83.944 ms
for 1,000 classes / 2,000 contracts, and 20.802 ms for helper import; maximum
child RSS was 18.58 MiB. The recorded 0.1.0 medians were 2.732 ms, 82.965 ms
and 22.348 ms, with 18.61 MiB maximum child RSS. The synthetic CLI measurement
is effectively unchanged. The small-process ranges include slow launch outliers
(282 ms now and 460 ms in 0.1.0); its median shift is smaller than the observed
launch variation. These results do not compare lifecycle checking with
inspection.

Reproduce with:

```sh
cargo build --release --locked -p typepact
cargo build --release --locked -p typepact --bins --examples
target/release/examples/benchmark_lifecycle 1
target/release/examples/benchmark_lifecycle 250
target/release/examples/benchmark_lifecycle 1000
/usr/bin/time -l target/release/examples/benchmark_lifecycle 1000
uv run --locked python scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000
```

## Provisional flow limits

| Limit | Current default | Boundary regression |
| --- | ---: | --- |
| Nested `if` regions per body | 128 | `flow::tests::control_flow_depth_exhaustion_weakens_resources_and_reports_a_boundary` |
| Possible targets for one local alias | 64 | `flow::tests::alias_overflow_drops_exact_targets_and_weakens_all_candidates` |
| Tracked resource objects per body | 4,096 | `flow::tests::object_budget_exhaustion_does_not_create_an_exact_place` |
| Nested call/effect evaluation | 8 | `flow::tests::call_depth_exhaustion_weakens_resources_and_reports_a_boundary` |
| Flow transfers per request | 100,000 | `tests/cli.rs::flow_work_budget_stops_exponential_helper_expansion`, `db::tests::flow_budget_is_an_input_to_incremental_file_queries` |

The request-wide transfer budget is divided evenly among files. Unused work in
one file is not reassigned, which keeps edits in earlier files from changing
unrelated query inputs. Each limit triggers visible incomplete coverage and
weakens affected state; the tests exercise exhaustion recovery. The limits
still need an accepted LC01 rationale and measured workload impact. No absolute
latency threshold has been set from this preliminary corpus.

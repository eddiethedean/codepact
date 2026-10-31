# 0.2 lifecycle performance baseline

Status: preliminary local baseline; exact-candidate replay and reviewer
acceptance remain required before G4 or LC01 can pass.

## Environment and method

- Host: Apple M4 Pro, macOS 26.5.2, ARM64.
- Toolchain: rustc 1.96.0; optimized Rust build; CPython 3.11.15.
- Analyzer source target: Python 3.11.
- Lifecycle runs: `cargo run --release --locked -p typepact --example benchmark_lifecycle -- N`.
- Repeats: seven for cold sessions and each warm/edit scenario; table shows median and range.
- Corpus: two source files, one provider and one client. The client constructs N distinct resources and calls a helper that closes each before use. The 1, 250 and 1,000 object corpora contain 294, 19,641 and 78,891 source bytes.
- The benchmark asserts the expected definite or possible finding count before reporting timings. It includes in-process Salsa session time, not process startup.

These measurements describe the current source tree and are not an accepted
release result until replayed on the exact 0.2.0 candidate. They do not establish
performance on other platforms or corpora.

## Lifecycle session timings

Milliseconds, median (minimum–maximum):

| Resources | Cold session | Warm no-op | Unrelated body edit | Imported effect edit | Location-only edit |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.185 (0.144–1.320) | 0.010 (0.009–0.012) | 0.113 (0.112–0.128) | 0.114 (0.113–0.122) | 0.112 (0.111–0.114) |
| 250 | 4.761 (4.742–5.237) | 0.390 (0.387–0.408) | 2.775 (2.753–2.824) | 3.097 (3.088–3.125) | 2.767 (2.757–2.783) |
| 1,000 | 41.486 (41.185–42.031) | 1.416 (1.414–1.461) | 18.997 (18.685–19.188) | 20.923 (20.612–21.315) | 19.421 (19.277–19.436) |

The 1,000-resource run reached 32.55 MiB peak child RSS, measured with Python's
`resource.getrusage`. This is process high-water memory, not retained-session
memory. Live RSS sampling was unavailable in this environment, so retained
memory remains unmeasured.

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

The existing declaration-inspection corpus v1 was run with the same optimized
binary method on this host. Current medians were 2.923 ms for the small CLI,
81.861 ms for 1,000 classes / 2,000 contracts, and 21.181 ms for helper import;
maximum child RSS was 18.59 MiB. The recorded 0.1.0 medians were 2.732 ms,
82.965 ms and 22.348 ms, with 18.61 MiB maximum child RSS. The synthetic CLI
measurement is effectively unchanged. The small-process ranges include slow
launch outliers (296 ms now and 460 ms in 0.1.0); its median shift is smaller
than the observed launch variation. These results do not compare lifecycle
checking with inspection.

Reproduce with:

```sh
cargo build --release --locked -p typepact
cargo build --release --locked -p typepact --example benchmark_lifecycle
target/release/examples/benchmark_lifecycle 1
target/release/examples/benchmark_lifecycle 250
target/release/examples/benchmark_lifecycle 1000
uv run --locked python scripts/benchmark_inspector.py --binary target/release/typepact --repeats 7 --classes 1000
```

## Provisional flow limits

| Limit | Current default | Boundary regression |
| --- | ---: | --- |
| Nested `if` regions per body | 128 | `flow::tests::control_flow_depth_exhaustion_weakens_resources_and_reports_a_boundary` |
| Possible targets for one local alias | 64 | `flow::tests::alias_overflow_drops_exact_targets_and_weakens_all_candidates` |
| Tracked resource objects per body | 4,096 | `flow::tests::object_budget_exhaustion_does_not_create_an_exact_place` |
| Nested call/effect evaluation | 8 | `flow::tests::call_depth_exhaustion_weakens_resources_and_reports_a_boundary` |

Each limit triggers visible incomplete coverage and weakens affected state; the
tests exercise exhaustion recovery. The limits still need an accepted LC01
rationale and measured workload impact. No absolute latency threshold has been
set from this preliminary corpus.

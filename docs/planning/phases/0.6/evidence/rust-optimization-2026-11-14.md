# Rust optimization follow-up

This follow-up compares the source optimizations with parent revision
`26698bb45c8641693d8c0c7a0d4451d84dad2d1a`. The baseline and working-tree
candidate were built in release mode on the same macOS 26.5.2 arm64 host with
Rust/Cargo 1.96.0.
The same benchmark source was used for both builds, with seven timing samples
per workload. Session checks assert that an edited incremental result equals a
fresh analysis. Timings below are medians.

## Session and query scaling

The independent-module workload uses small unrelated files; the provider
workload uses 250 consumers of one provider. Times are milliseconds.

| Workload | Cold, baseline → candidate | Warm, baseline → candidate | Edit, baseline → candidate |
| --- | ---: | ---: | ---: |
| 500 independent files | 10.891 → 6.441 | 1.090 → 0.148 | 5.562 → 1.501 |
| 1,000 independent files | 28.288 → 12.183 | 4.211 → 0.290 | 18.206 → 2.917 |
| 2,000 independent files | 83.598 → 24.870 | 15.977 → 0.593 | 68.826 → 6.254 |
| 4,000 independent files | 283.241 → 52.116 | 63.978 → 1.330 | 268.632 → 13.670 |
| 250 consumers, one provider | 7.981 → 5.783 | 0.303 → 0.058 | 6.369 → 4.389 |

At 4,000 files, warm analysis fell by about 97.9% (48.1× faster). The warm
timing includes input reconciliation, so it reflects the indexed path
membership and cached query reuse together. Provider edits also preserve
fresh-analysis parity.

The session cache resets after each 1,000 changed inputs. At exact multiples
of that threshold, the benchmark applies and restores a temporary edit before
timing the measured edit. This primes the cache after the automatic reset and
keeps the timed edit representative of cached-session reuse. The JSON output
marks those cases with `edit_cache_reset_primed`.

## Function source sharing and memory

This workload analyzes 250 functions, first with no trailing comment padding,
then with a 135,000-byte comment. The comment increases the source from 8,531
to 143,531 bytes without adding declarations. Cold analysis medians remained
similar while peak RSS dropped substantially.

| Source | Cold, baseline → candidate | Peak RSS, baseline → candidate |
| --- | ---: | ---: |
| 8,531 bytes | 38.146 → 37.400 ms | 23,068,672 → 8,142,848 bytes |
| 143,531 bytes | 41.561 → 39.939 ms | 194,838,528 → 8,994,816 bytes |

Peak RSS is from one `/usr/bin/time -l` run per variant and covers the complete
benchmark process. With the padded source, peak RSS fell by about 95.4%. The
benchmark asserts repeated analysis returns the same inspection.

## Retained workload parity

The release benchmarks also completed successfully for phase 0.5 legacy
ownership (250 functions, both modes), aliases (4,095), recursive transfers
(96), and nested TaskGroups (128 total children: one outer plus 127 inner).
Phase 0.6 state, exclusive-access and executor workloads (250 operations
each) also passed their built-in assertions. The TaskGroup generator's
qualified input is `tasks 127`; `tasks 128` adds one child beyond that
qualified case.

## Reproduction

Build with `cargo build --release --locked -p codepact --examples`. Run the
new harness with `target/release/examples/benchmark_rust_optimizations` and
one of `memory <padding_bytes> [samples]`, `session <file_count> [samples]`,
or `provider <consumer_count> [samples]`. The saved baseline used the same
example source copied into a clean archive of `26698bb` and a separate Cargo
target directory. For peak memory, prefix the memory workload with
`/usr/bin/time -l` and use one sample.

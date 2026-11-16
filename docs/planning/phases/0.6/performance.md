# Phase 0.6 candidate performance evidence

The retained 0.6-versus-0.5 performance comparison uses candidate `1da46b68d8e0d9ebc226441abb3eea3c1c28f13f` and the released `v0.5.1` tag at `d0f41fa`. Both were built in optimized mode with Rust/Cargo 1.96.0 on macOS 26.5.2 arm64 (Darwin 25.5.0), using separate target directories. Each time is the median of seven samples. Phase 0.6 workloads target Python 3.13; retained workloads target Python 3.11. Source is analyzed statically and never executed. The benchmark asserts reused results match fresh analysis after edits. RSS is the whole benchmark process, including parity checks.

## New 0.6 workloads

| Workload (250 operations) | Source bytes | Cold ms | Warm ms | Affected edit ms | Unrelated edit ms | Location edit ms | Report ms | Peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Finite state transitions | 24,806 | 58.472 | 0.011 | 68.367 | 0.336 | 68.940 | 0.628 | 50.77 |
| Exclusive access and cleanup | 16,349 | 77.252 | 0.012 | 88.247 | 0.292 | 88.033 | 0.515 | 35.44 |
| ThreadPoolExecutor protocol | 6,136 | 1.697 | 0.004 | 1.672 | 0.085 | 1.710 | 0.228 | 16.80 |

The unrelated-file edit reuses the advanced file's analysis; the phase 0.6 Salsa regression test separately asserts three initial file-flow executions and two after changing a protocol provider (the provider and its dependent caller), leaving the unrelated file cached. Location-only edits cost about as much as an affected edit for state and exclusive access. That invalidation cost is visible and should be monitored; this run has no separate location-edit latency budget.

## Retained 0.5 workloads

Times compare seven-sample medians from the exact candidate with a fresh optimized build of released `v0.5.1`, run on this host using the same commands and workload sizes. Percent changes are candidate versus baseline; negative values are faster.

| Retained workload | v0.5.1 cold / edit / report ms | Candidate cold / edit / report ms | Change, cold / edit / report | Peak RSS MiB, v0.5.1 / candidate |
| --- | ---: | ---: | ---: | ---: |
| 250 lifecycle calls, default scope | 56.613 / 55.867 / 0.505 | 56.211 / 56.037 / 0.496 | −0.7% / +0.3% / −1.8% | 33.56 / 33.84 combined |
| 250 lifecycle calls, ownership scope | 80.255 / 79.513 / 0.522 | 77.301 / 77.847 / 0.507 | −3.7% / −2.1% / −2.9% | combined process only |
| 4,095 live aliases | 18.057 / 17.642 / 2.899 | 18.003 / 18.082 / 2.900 | −0.3% / +2.5% / +0.0% | 51.14 / 53.00 |
| 96 recursive transfers | 87.942 / 87.003 / 0.283 | 87.288 / 87.248 / 0.280 | −0.7% / +0.3% / −0.8% | 13.00 / 13.50 |
| 128 TaskGroup children | 18.016 / 17.711 / 0.269 | 18.400 / 18.071 / 0.225 | +2.1% / +2.0% / −16.5% | 9.55 / 9.81 |

The retained comparison shows no large time regression: the largest cold/edit change is +2.5%, and peak RSS changes by at most 3.6%. These are single-host observations, not cross-platform performance claims.

## TaskGroup coroutine budget

The budget workload was measured from source revision `716655a43c7433267c8c07cd487406cbe84e116d`, separately from the retained 0.6-versus-0.5 comparison above. It uses Python 3.13 and `advanced-assume-default-task-factory`, with 8,849 source bytes at the 128-record limit. Scheduled tasks are represented by pending coroutine records, so the 129th task attempt reaches the shared coroutine budget and produces one visible boundary. There is no separate task counter or second task-budget boundary. Every sample checks recovered, reused and fresh results for semantic parity; target Python source is analyzed statically and never executed.

| Operation | Seven-sample median ms |
| --- | ---: |
| Cold analysis at 128 | 14.174 |
| Warm unchanged session | 0.003 |
| Edit to 129, including boundary | 27.936 |
| Edit back to 128, recovery | 14.154 |
| Unrelated-file edit | 0.079 |
| Location-only edit | 14.127 |
| Report serialization | 0.269 |

The seven raw samples, source revision, toolchain, and host are retained in [task-budget-performance.json](evidence/task-budget-performance.json). `/usr/bin/time -l` measured 0.59 seconds for the whole benchmark process and peak RSS of 9,289,728 bytes (8.9 MiB) on macOS 26.5.2 arm64, using optimized Rust/Cargo 1.96.0. This is an added local stress measurement, not a cross-platform performance claim.

## Rust-level optimization follow-up

Source-sharing, session reconciliation and query-indexing changes are measured
against the immediately preceding source revision in the separate [Rust
optimization report](evidence/rust-optimization-2026-11-14.md). It includes
matched baseline/candidate timings for 500–4,000 independent files, provider
edits, and function-source memory scaling. The repeatable generator is
[benchmark_rust_optimizations.rs](../../../../crates/codepact/examples/benchmark_rust_optimizations.rs).

## Reproduction and provenance

Build the example with `cargo build --release --locked -p codepact --example benchmark_phase06_budget`, then run `/usr/bin/time -l target/release/examples/benchmark_phase06_budget`. Build the other revisions separately with `cargo build --release --locked -p codepact --examples`; run `/usr/bin/time -l target/release/examples/benchmark_phase05 {legacy,alias,recursive,tasks} SIZE` and `/usr/bin/time -l target/release/examples/benchmark_phase06 {state,exclusive,executor} 250`. The complete samples for the budget stress case are retained in [task-budget-performance.json](evidence/task-budget-performance.json). The complete raw samples, commits, commands and memory readings for the retained comparison are in the [candidate performance record](evidence/candidate-performance-2026-11-14.json). The earlier [preliminary advanced record](evidence/phase06-benchmark-2026-11-14.json) and [preliminary retained record](evidence/phase05-retained-2026-11-14.json) were measured on mutable, unhashed worktree source and do not reproduce these compared values; they are retained for audit. The [0.5 performance baseline](../0.5/performance.md) documents the prior measurements.

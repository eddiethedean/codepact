# Phase 0.6 preliminary performance results

These measurements are preliminary worktree evidence, not an exact-candidate G4 pass. The source was based on `2d6bb6bcb27f501dd6c13421a77949cc9e70a33f` with the 0.6 changes uncommitted; rerun after creating the release candidate. The retained 0.5 workloads are still required for the candidate comparison.

Measurements used the optimized Rust build on macOS 26.5.2 arm64, Darwin 25.5.0, rustc/cargo 1.96.0. Each timing is the median of seven session samples. Source was analyzed without execution. The benchmark also checks cold/reused parity against a fresh analysis after each edit. Peak RSS is for the full benchmark process, including those parity checks.

| Workload (250 operations) | Source bytes | Cold ms | Warm ms | Affected edit ms | Unrelated edit ms | Location edit ms | Report ms | Peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Finite state transitions | 24,806 | 343.235 | 0.035 | 401.843 | 1.770 | 399.783 | 3.341 | 50.59 |
| Exclusive access and cleanup | 16,349 | 469.626 | 0.063 | 523.396 | 1.542 | 541.512 | 2.834 | 35.50 |
| ThreadPoolExecutor protocol | 6,136 | 10.997 | 0.023 | 10.873 | 0.528 | 10.242 | 1.203 | 16.67 |

The affected edit changes a modeled method body in `main.py`; the unrelated edit changes `independent.py`; the location edit inserts a comment at the start of the affected file. All results remained complete for the supported subset with no diagnostics. Unrelated edits were much cheaper than rechecking the advanced file. Location-only edits to the state and exclusive workloads cost about as much as full checks, so the candidate rerun should retain query-event evidence and review that invalidation cost.

The retained 0.5 workloads were also rerun in the optimized 0.6.0 worktree. The table compares seven-sample medians with the published 0.5.1 baseline measurements (which use the retained 0.5.0 comparison values). These are local observations, not release gate passes.

| Retained workload | 0.5 cold / edit / report ms | 0.6 cold / edit / report ms | Change, cold / edit / report | 0.5 / 0.6 peak RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| 250 lifecycle calls, default scope | 57.494 / 57.213 / 0.534 | 53.452 / 52.803 / 0.509 | −7.0% / −7.7% / −4.7% | 33.53 / 33.21 combined |
| 250 lifecycle calls, ownership scope | 76.095 / 76.592 / 0.480 | 72.099 / 72.451 / 0.493 | −5.3% / −5.4% / +2.7% | combined process only |
| 4,095 live aliases | 17.615 / 17.536 / 2.692 | 16.685 / 16.601 / 2.688 | −5.3% / −5.3% / −0.1% | 48.77 / 33.39 |
| 96 recursive transfers | 81.294 / 78.317 / 0.314 | 84.049 / 83.395 / 0.286 | +3.4% / +6.5% / −9.0% | 13.19 / 13.33 |
| 127 TaskGroup children | 16.239 / 16.105 / 0.221 | 17.618 / 17.194 / 0.219 | +8.5% / +6.8% / −0.8% | 9.23 / 9.61 |

The retained TaskGroup workload is the largest relative cold/edit increase in this comparison; the 0.6.0 result is about 1.4–1.5 ms slower. Recursive transfer is about 2.1–5.1 ms slower. Alias and paired lifecycle checks are similar or faster. The 0.6.0 alias benchmark process peak is substantially lower, but process RSS is workload-specific and should be checked again on the exact candidate. The raw 0.6.0 samples and process RSS are in [the retained-workload record](evidence/phase05-retained-2026-11-14.json).

Reproduce by building `cargo build --release --locked -p codepact --examples`, then running `/usr/bin/time -l target/release/examples/benchmark_phase05 {legacy,alias,recursive,tasks} SIZE` and `/usr/bin/time -l target/release/examples/benchmark_phase06 {state,exclusive,executor} 250`. The complete per-sample timings and RSS outputs are in the [advanced workload record](evidence/phase06-benchmark-2026-11-14.json) and the [retained workload record](evidence/phase05-retained-2026-11-14.json). The benchmark generators are [benchmark_phase05.rs](../../../../crates/codepact/examples/benchmark_phase05.rs) and [benchmark_phase06.rs](../../../../crates/codepact/examples/benchmark_phase06.rs). The [0.5 performance baseline](../0.5/performance.md) lists earlier comparison values.

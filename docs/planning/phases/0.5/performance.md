# 0.5 candidate performance

Local optimized measurements on macOS 26.5.2 arm64, rustc 1.96.0 and target
Python 3.11. Each timing is the median of seven samples. The single-edit
measurement inserts one source line into a reused session. Report time
includes building and serializing the adoption report. Peak RSS comes from
the benchmark child process and includes its whole run. A separate Python
process runs each workload and reads `getrusage(RUSAGE_CHILDREN).ru_maxrss`.

| Workload | Scope | Cold ms | Warm ms | Edit ms | Report ms | Peak RSS MiB |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 250 borrowed lifecycle calls | Default | 57.494 | 0.011 | 57.213 | 0.534 | 33.53 combined |
| 250 borrowed lifecycle calls | Ownership | 76.095 | 0.011 | 76.592 | 0.480 | 33.53 combined |
| 4,095 simultaneously live aliases | Ownership | 17.615 | 0.013 | 17.536 | 2.692 | 48.77 |
| 96 recursive transfers | Ownership | 81.294 | 0.010 | 78.317 | 0.314 | 13.19 |
| 128 children across nested task groups | Ownership and TaskGroup | 16.239 | 0.009 | 16.105 | 0.221 | 9.23 |

Enabling ownership increased cold and edit time on the paired workload by
32% and 34%. It adds argument-value snapshots, transfer checks and source
effect summaries to that workload. The no-op session timings remained near
the timing floor. The provider-edit query-event test observed three initial
file checks, two dependent checks after the edit and zero after a no-op.
[Benchmark generator](../../../../crates/codepact/examples/benchmark_phase05.rs) · [Session test](../../../../crates/codepact/src/db.rs)

The retained 0.4 binaries and candidate binaries also ran the inherited
workloads with default ownership scope.

| Inherited workload | 0.4 cold / warm ms | 0.5 cold / warm ms |
| --- | ---: | ---: |
| Lifecycle, 250 resources | 6.542 / 0.422 | 6.887 / 0.434 |
| Synchronous cleanup, 16 resources | 1.552 / 0.004 | 1.670 / 0.004 |
| Awaited cleanup and cancellation, 16 resources | 0.694 / 0.021 | 0.692 / 0.020 |

Peak RSS on the inherited lifecycle workload increased from 14.67 to 17.56
MiB. The two cleanup scenarios together increased from 7.08 to 7.22 MiB.
The helper's process-start-plus-import median on CPython 3.11.15 was
20.957 ms for the installed 0.4 wheel and 21.566 ms for the candidate wheel.
These are single-host observations for maintainer review. [0.4 baseline evidence](../0.4/evidence/qualification-2026-11-10/README.md) · [Retained samples](evidence/qualification-reports.tar.gz)

Reproduce with `cargo build --release --locked -p codepact --bins --examples`,
then run `benchmark_phase05` with `legacy 250`, `alias 4095`,
`recursive 96`, and `tasks 127`. Run `benchmark_lifecycle 250` and
`benchmark_phase03 16` for the inherited workloads.

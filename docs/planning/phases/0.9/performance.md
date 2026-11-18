# 0.9 performance evidence

The [validation matrix](validation.md) assigns these measurements to V09-23.
The comparison starts from [published CodePact 0.8.0](../../../releases/0.8.0-evidence.md)
and uses the same optimized Rust workloads on the candidate.

## Method

The capture alternates baseline-first and candidate-first starts across 24
process pairs per workload. Each process measures seven trials. The inherited
state, exclusive-access and executor examples use 250 functions. The installed
manifest example uses 64 caller functions and measures a provider upgrade with
unchanged caller bytes. Peak RSS is the largest observed whole-process resident
set in an arm.

The two new workloads each use 64 functions. The outcome workload follows
MustHandle aliases and compares source captures after checking both variants
and requesting repair of every original finding. The work workload checks two
members per TaskGroup, then replaces status checks with result retrieval on the
normal and exceptional paths. The retrieval fixture verifies that the normal
work produces no actionable observation finding and retains possible findings
from exceptional scheduling. The new workloads verify fresh and reused
captures after affected and independent edits.

## Working-tree results

The run used macOS 26.5.2 ARM64 and Rust 1.96.0, comparing the published
`v0.8.0` tag at `9393919a65dd215798ac001c661ae6ece2ffeace` to the Phase
0.9 working tree based on `12decbcfe1099fd0c93bc94f0cf168601890842b`.

Time changes are paired medians. The RSS column is the difference between
the arms' maximum observed process RSS.

| Workload | Cold | Unchanged | Affected edit | Independent edit | Peak RSS change |
| --- | ---: | ---: | ---: | ---: | ---: |
| State | +1.54% | +0.17 µs (+1.49%) | +1.54% | +4.69% | +0.95 MiB |
| Exclusive access | +1.16% | −0.06% | +1.15% | +1.35% | +0.56 MiB |
| Executor | +2.58% | +0.25 µs (+6.75%) | +3.99% | +2.94% | +0.61 MiB |
| Installed manifest | −0.56% | −0.12% | −0.53% | −0.63% | −0.17 MiB |

The largest relative paired increase was 6.75% on the unchanged executor check, equivalent to 0.25 µs.
The largest measured peak RSS increase was 0.95 MiB on the state workload.
Other candidate work on the shared development host overlapped part of the
run. The timings describe the named fixtures on this host.

The following candidate-only timings are pooled medians across the trial
samples. Snapshot capture builds portable diagnostic and semantic evidence.
Comparison starts with both captures already in memory.

| Workload | Cold capture | Unchanged capture | Affected edit | Independent edit | Comparison | Peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MustHandle and repair review | 13.868 ms | 8.383 ms | 12.641 ms | 2.750 ms | 0.057 ms | 11.19 MiB |
| TaskGroup work observation | 31.916 ms | 15.153 ms | 32.367 ms | 13.324 ms | 0.096 ms | 14.39 MiB |

The local receipt is `target/phase09-performance.json` with SHA-256
`0bf8ee2a9fd1d470a6eab6ec30c4e35d790dd6a2f1b7c4808cce6138f75fe5a0`.

## Reproduce

Build `benchmark_phase06` and `benchmark_phase08` in both the `v0.8.0` checkout
and the candidate using `cargo build --release --locked --example NAME`. Build
`benchmark_phase09` in the candidate the same way. Run the capture from the
candidate with the path to the baseline checkout.

```sh
CODEPACT_PHASE07_PROCESS_REPEATS=24 python scripts/capture_phase09_performance.py \\
  --baseline /path/to/codepact-v0.8.0 --output target/phase09-performance.json
```

The receipt includes raw trials, process ordering, executable digests, Rust
version and platform. [Capture script](../../../../scripts/capture_phase09_performance.py)

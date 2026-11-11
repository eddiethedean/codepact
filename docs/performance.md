# Performance guidance

CodePact sets explicit source and work limits. See the
[supported syntax and limits](supported-syntax.md#input-and-work-limits). The
CLI starts a new process per invocation; the Rust `AnalysisSession` can reuse
work when a host updates a project repeatedly.

## CodePact 0.5 measurements

These synthetic release-qualification measurements used an optimized build
on macOS 26.5.2 ARM64 with Rust 1.96.0 and Python 3.11 as the source target.
Timings are medians of seven samples. The warm column measures an unchanged
snapshot in the same session.

| Workload | Scope | Cold | Warm | After one edit | Report |
| --- | --- | ---: | ---: | ---: | ---: |
| 250 borrowed lifecycle calls | Default | 57.494 ms | 0.011 ms | 57.213 ms | 0.534 ms |
| 250 borrowed lifecycle calls | Ownership | 76.095 ms | 0.011 ms | 76.592 ms | 0.480 ms |
| 128 children across nested task groups | Ownership and TaskGroup | 16.239 ms | 0.009 ms | 16.105 ms | 0.221 ms |

Ownership increased median cold time by 32% on the paired workload. Its
default and ownership runs had a combined peak RSS measurement of 33.53 MiB.
These figures describe a single host and controlled workloads. Warm session
timings apply to Rust embeddings that reuse a session. For CI, measure fresh
CLI invocations over your selected source tree.
[Method, additional workloads, and reproduction commands](https://github.com/eddiethedean/codepact/blob/v0.5.0/docs/planning/phases/0.5/performance.md)

## Historical 0.1.0 baseline

The recorded 0.1.0 cold-process measurements were taken on an Apple M4 Pro with
macOS 26.5.2 ARM64, CPython 3.11.15, and rustc 1.96.0:

| Scenario | Median |
| --- | ---: |
| Small CLI inspection | 2.732 ms |
| Synthetic CLI corpus: 1,000 classes and 2,000 contracts | 82.965 ms |
| Python helper import | 22.348 ms |
| Maximum child-process RSS across scenarios | 18.61 MiB |

CLI measurements include process startup and operating-system cache effects.
The [baseline record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.1.0-baseline.md) contains ranges and the
measurement method. Salsa query reuse is tested separately; cold timings do not
measure incremental editor latency. These figures describe one host and are not
performance guarantees.

The 0.2.0 lifecycle timings, retained-session RSS, and accepted flow limits are
recorded in the [phase performance record](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.2/performance.md)
and [release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.2.0-evidence.md). The maintainer accepted the
measured regression with its limitations; these results describe one host and
are not performance guarantees. Do not apply the 0.1.0 numbers to lifecycle
checking or another platform.

## 0.3.0 release measurements

The [0.3.0 performance record](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/performance.md) contains
cleanup, cancellation, lifecycle, query-reuse, and memory measurements for the
released analyzer, including the correctness fixes to allocation identity,
direct-await matching, exception-target cleanup, and raise-cause evaluation.
Earlier pre-fix measurements remain available there for historical comparison.

## Practical guidance

- Use a narrow root for `check`: it scans Python files throughout that root.
- Keep generated, vendored, and unrelated files outside the selected source
  tree when possible.
- Pass only the relevant files to `inspect-contracts`; it does not scan a root.
- Treat input-budget boundaries as incomplete coverage. Increasing input size
  beyond documented limits does not make the result more complete.
- For repeated updates in an editor or service, use `AnalysisSession` and pass
  the current full source snapshot each time.

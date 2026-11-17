# Phase 0.7 performance evidence

Two independent optimized captures compare `v0.6.0` commit
`2d638e761a26211e5c98f1792731103dd9856994` with candidate source commit
`8108f9458b730636eb3e5f192ac9b8801f7a5ddb`. Each capture uses 10 separate
process runs and seven timed iterations per process. The table reports pooled
medians across both captures (140 samples per side and metric). Peak memory is
the maximum across the 20 isolated processes. Both raw files are retained as
[`performance.json`](performance.json) and
[`performance-repeat2.json`](performance-repeat2.json).

These measurements were captured on implementation revision
`8108f9458b730636eb3e5f192ac9b8801f7a5ddb`. Follow-up revision
`be8691f08a48e43abac726ecd648b827f7cb3f1a` adds qualification tests only;
the optimized runtime and analyzer code are unchanged.

The environment was macOS 26.5.2 arm64, CPython 3.11.14, and Rust 1.96.0. The
baseline and candidate use the same phase 0.6 workloads at size 250. Time is
milliseconds; memory is MiB.

| Workload | Cold 0.6 → 0.7 | Unchanged 0.6 → 0.7 | Affected edit 0.6 → 0.7 | Unrelated edit 0.6 → 0.7 | Peak RSS 0.6 → 0.7 |
| --- | ---: | ---: | ---: | ---: | ---: |
| State | 56.278 → 55.793 (-0.9%) | 0.008875 → 0.007855 (-11.5%) | 66.346 → 65.977 (-0.6%) | 0.312500 → 0.297833 (-4.7%) | 15.19 → 14.63 (-3.7%) |
| Exclusive | 75.009 → 76.356 (+1.8%) | 0.008167 → 0.010042 (+23.0%) | 85.162 → 86.248 (+1.3%) | 0.249855 → 0.261229 (+4.6%) | 14.44 → 14.91 (+3.3%) |
| Executor | 1.523 → 1.486 (-2.4%) | 0.002708 → 0.002917 (+7.7%) | 1.477 → 1.468 (-0.6%) | 0.079937 → 0.080812 (+1.1%) | 17.64 → 18.15 (+2.9%) |

## Review item

The exclusive unchanged-session measurement exceeds the proposed 10% median
regression investigation trigger in
[`testing.md`](../../../testing.md#performance). Its pooled change
is 8.167 to 10.042 microseconds (+1.875 microseconds, +23.0%). Each independent
capture showed the same direction: +0.917 microseconds (+10.1%) and +2.250
microseconds (+28.3%). The raw per-process samples are retained, and cold,
affected-edit and unrelated-edit medians for this workload changed by +1.8%,
+1.3% and +4.6%. This is a small absolute change, but the proposed trigger is
not marked as cleared; G4 remains Pending for maintainer review.

## New phase workloads

These timings pool 20 separate process runs per workload and seven timed
iterations per process. Peak memory is the maximum across the 20 processes.
Coverage is incomplete in each generated synthetic project because unrelated
unknown call boundaries remain visible; that is expected for these timing
inputs and is recorded in the raw JSON.

| Workload | Size | Cold | Unchanged | Affected edit | Unrelated edit | Peak RSS | Source bytes / discovered files |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Finite matches | 64 | 12.762 ms | 0.031 ms | 10.515 ms | 0.455 ms | 9.72 MiB | 9,911 / 2 |
| Finite matches | 256 | 198.197 ms | 0.116 ms | 161.092 ms | 4.605 ms | 23.70 MiB | 39,635 / 2 |
| Installed provider | 64 | 3.018 ms | 0.217 ms | 1.300 ms | 0.376 ms | 13.72 MiB | 6,098 / 3 |
| Installed provider | 256 | 39.883 ms | 2.268 ms | 17.297 ms | 4.256 ms | 92.70 MiB | 24,962 / 3 |

The match workload selects finite `Result`/`Option` checking while lifecycle and
must-use rules are disabled. The provider workload selects marked installed
provider summaries while finite matches are disabled. The raw evidence records
diagnostic counts, enabled/disabled scopes, source bytes, discovered files,
coverage, process memory, repetitions, and exact candidate/baseline revisions.


# 0.4 candidate performance evidence — partial

Status: **Partial measurements recorded; G4 remains pending maintainer review.**
The covered cleanup, async and inspector workloads were measured on the current
implementation candidate. The remaining Phase 0.4-specific workloads are listed
below; these results alone do not pass operational qualification.

## Candidate and environment

- Analyzer source candidate: `e3ba65d539adb4038e9d593aca91c221deda8c79`.
- Binary: optimized `target/release/codepact`, SHA-256
  `5b7370dee8bee71f61b23f130ea7003a6e8d518e0e07915303ee5e5812083ada`.
- Host: macOS 26.5.2, ARM64 (Darwin 25.5); CPU model not independently captured.
- Toolchain: rustc 1.96.0; Python 3.11.15 for `benchmark_inspector.py`.
- Analyzer target: Python 3.11.
- Each in-process harness uses seven cold checks and seven warm/edit checks,
  reports medians, and asserts its documented result invariants. The CLI harness
  uses seven fresh-process samples and includes launch/output-capture costs.
- Source-code SHA-256: `benchmark_lifecycle.rs`
  `f27f5610b228742cb672ee5c8546117f4fd5e8ed4e5bedce24ed34c3881979ae`,
  `benchmark_phase03.rs`
  `bfb061c911826463c64618d4731e7d01e9636ab8139f278ed6131f4f1a58174d`, and
  `benchmark_inspector.py`
  `769a789e4a1925225978154172448bfbb56b755a279757373b4a0c08e6724929`.

## Lifecycle session workload

Milliseconds; seven runs per scenario. `TP1001` count equals the number of
resources in every scenario. Combined source bytes are reported by the harness.

| Resources | Source bytes | Scenario | Median | Range |
| ---: | ---: | --- | ---: | ---: |
| 1 | 294 | Cold session | 0.334 | 0.285–2.953 |
| 1 | 294 | Warm no-op | 0.010 | 0.010–0.013 |
| 1 | 294 | Unrelated body edit | 0.279 | 0.275–0.322 |
| 1 | 294 | Imported effect edit | 0.292 | 0.283–0.328 |
| 1 | 294 | Location-only edit | 0.217 | 0.205–0.255 |
| 250 | 19,641 | Cold session | 6.831 | 6.190–8.991 |
| 250 | 19,641 | Warm no-op | 0.362 | 0.346–0.371 |
| 250 | 19,641 | Unrelated body edit | 5.330 | 5.289–5.443 |
| 250 | 19,641 | Imported effect edit | 7.542 | 7.350–7.613 |
| 250 | 19,641 | Location-only edit | 5.189 | 5.105–5.226 |
| 1,000 | 78,891 | Cold session | 27.177 | 26.734–28.891 |
| 1,000 | 78,891 | Warm no-op | 1.439 | 1.427–1.459 |
| 1,000 | 78,891 | Unrelated body edit | 22.840 | 22.773–23.479 |
| 1,000 | 78,891 | Imported effect edit | 32.780 | 32.148–33.362 |
| 1,000 | 78,891 | Location-only edit | 22.430 | 22.381–22.614 |

Canonical corpus identities hash concatenated `path NUL module NUL source NUL`
records in input order. Their SHA-256 values are:

| Resources | Corpus SHA-256 |
| ---: | --- |
| 1 | `1adca2b539b8df592531e44c6c33d079195195cec5e0c606d08d6dabaefb9f87` |
| 250 | `9f83c87a897389e9e288f5b688604836e2d36ce75397455f0176c3bc028edc91` |
| 1,000 | `2b449e2807e80dc070b125782f30c69471235cfa4b163e19f788945d9631ddb3` |

Against the published 0.3 baseline on the same generated corpus, the 1,000
resource medians changed by -7.5% cold, +1.6% warm, -12.5% for unrelated edits,
-5.5% for imported-effect edits and -11.3% for location-only edits. At 250
resources the differences range from -0.9% to +2.8%. The one-resource cold
range contains process-local outliers; timings are descriptive and no absolute
latency threshold is defined.

## Cleanup and async flow workload

The source generator exercises synchronous nested `try`/`except`/`else`/
`finally`, `while`, and two `contextlib.closing` managers. Its checker asserts
zero diagnostics and complete supported-subset coverage. The async generator
exercises direct awaited close and cancellation; it asserts there is no definite
`TP1002`, at least one possible `TP1002`, and incomplete coverage. This harness
reports seven-run cold and warm medians, but not ranges.

| Functions/resources | Sync source bytes | Sync cold / warm ms | Sync corpus SHA-256 | Async source bytes | Async cold / warm ms | Async corpus SHA-256 |
| ---: | ---: | ---: | --- | ---: | ---: | --- |
| 1 | 525 | 0.381 / 0.004 | `3bd95902507adf1aef2875963d4090bc496a7d872435802595614b4549ba4d2b` | 310 | 0.118 / 0.004 | `8a68e1fc604e9594bf78a371dd642e36e9cdfcd3d77a9cdd14078ec3a88c3e22` |
| 16 | 4,716 | 1.526 / 0.004 | `c2fc0560eedcc3c7ce8660be827e665e5a7b7212e38ec47008743a8c738dbc4e` | 1,576 | 0.658 / 0.016 | `01fae7444fed5ee8bd6060ba57ce4f3acbe67b60910e93f070bcb77f5358a05c` |
| 64 | 18,156 | 12.387 / 0.004 | `d5ba22fc7c7f12e05813a9af6fef3fdade31d203ce0688e0b9365a424b2448d7` | 5,656 | 4.746 / 0.056 | `aa96a6330e5f1779e4b136a1153c5a0af177f8a84c0e89d52df7196ae6a9309e` |

The harness does not report ranges for these scenarios, so no regression
conclusion is drawn from the median comparisons alone.

## CLI inspector and retained memory

`benchmark_inspector.py` ran seven subprocess samples using Python 3.11.15.
The 168-byte small corpus contains two declarations; the 93,735-byte generated
corpus contains 1,000 classes and 2,000 declarations.

| Scenario | Median ms | Range |
| --- | ---: | ---: |
| Small CLI | 3.072 | 2.751–733.302 |
| 1,000-class CLI | 50.758 | 50.165–52.877 |
| Helper import | 22.385 | 21.464–28.571 |

Maximum child RSS across those scenarios was 18.66 MiB. The small CLI range has
a process-launch outlier. The published 0.3 inspector run used Python 3.14.3,
so its helper-import number is not directly comparable.

The retained-session probe used the 1,000-resource lifecycle corpus, dropped
the diagnostics, waited two seconds and sampled RSS with `ps`. Baseline RSS was
1,968 KiB; retained-session RSS was 22,944 KiB; the increase was 20,976 KiB
(20.48 MiB). `/usr/bin/time -l` reported 23,494,656 bytes maximum RSS and
20,775,248 bytes peak memory footprint. These process-level measurements are
coarse, single samples, not allocation counts or enforced memory limits.

## Reproduction and remaining qualification

```sh
cargo build --release --locked -p codepact --bins --examples
target/release/examples/benchmark_lifecycle 1
target/release/examples/benchmark_lifecycle 250
target/release/examples/benchmark_lifecycle 1000
target/release/examples/benchmark_phase03 1
target/release/examples/benchmark_phase03 16
target/release/examples/benchmark_phase03 64
CODEPACT_RETAINED_MEMORY_PROBE=1 /usr/bin/time -l target/release/examples/benchmark_lifecycle 1000
uv run --locked python scripts/benchmark_inspector.py --binary target/release/codepact --repeats 7 --classes 1000
```

G4 still requires seven-run ranges and complete source/result identities for the
async workloads; phase 0.4 `MustUse` workloads; near-limit project/configuration
discovery; empty/dense baseline matching; policy-only edits with separate
semantic/report timings; and text/JSON/concise plus detail-limited output. The
maintainer has not reviewed or accepted these partial measurements.

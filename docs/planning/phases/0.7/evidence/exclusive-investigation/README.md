# Exclusive unchanged-session performance investigation

## Release question

The 0.7.0 qualification benchmark reported that the first unchanged check after a cold check on 250 exclusive-resource functions took 10.042 µs on the candidate and 8.167 µs on v0.6.0. The increase of 1.875 µs (+23.0%) crossed the proposed 10% G4 investigation threshold. Two captures showed increases of 10.1% and 28.3%. Each measured all ten baseline processes before starting the ten candidate processes. [Original measurements](../performance.md) · [Qualification capture program](../../../../../../scripts/capture_phase07_performance.py)

## Reproduction conditions

The baseline was `2d638e761a26211e5c98f1792731103dd9856994` and the candidate was `b57017f07efc76b59062cc87f66bd0df4022f659`. Both used optimized, locked Cargo builds on macOS 26.5.2 arm64 with Rust 1.96.0. The paired and instrumented runs used a CPython 3.9.6 launcher. The grouped replay used the original CPython 3.11.14 capture wrapper. The Rust executable times the API call internally. Both rebuilt executables matched the binaries at the paths recorded in the original capture byte for byte.

| Stock executable | SHA-256 |
| --- | --- |
| v0.6.0 | `3ae9cedebd382e5fa2bdcbe235163a5a4188880730117a0d7052f53270011b0d` |
| 0.7.0 candidate | `c82c269c1fd67b553940434f989813e48bb0f63a55e9e2ebcb57adb0655f42b9` |

Each stock process ran the full original `benchmark_phase06 exclusive 250` workload, including the edit checks and equality assertions. Each of its seven sessions contributed exactly one timed first unchanged call. Two paired captures ran adjacent baseline and candidate processes sequentially, balanced which version ran first and shuffled the pairs with separate seeds. The stage probe ran between captures. Core placement, processor frequency and other host activity were uncontrolled.

## Unmodified benchmark results

| Adjacent process pairs | Paired median difference (µs) | Paired median change | 95% bootstrap interval | Candidate slower | Original pooled metric |
| --- | ---: | ---: | ---: | ---: | ---: |
| First 16 pairs | −0.125 | −2.2% | −15.0% to +13.9% | 7/16 | +6.5% |
| Independent 24 pairs | +0.104 | +1.8% | −3.6% to +9.8% | 13/24 | +0.7% |
| All 40 pairs | +0.021 | +0.5% | −4.0% to +5.4% | 20/40 | +3.3% |

The last column uses the original capture's upper middle sample across all seven trials from every process. Across 280 observations per version, it gave 6.250 µs on v0.6.0 and 6.458 µs on the candidate. For the paired columns, take each process's median of seven trials, compare the two process medians within each pair, then take the median difference or percentage change across pairs. The exploratory interval is the 2.5th to 97.5th percentile from 10,000 resamples of whole process pairs. Among the 20 pairs run baseline-first, the median change was +1.8%. Among the 20 run candidate-first, it was −0.9%. [First capture](stock.json) · [Independent repeat](stock-repeat.json) · [Calculation](summarize.py)

As a separate check, the original capture wrapper ran ten consecutive processes per version and then repeated the comparison with the version order reversed. With v0.6.0 first, pooled upper medians were 5.791 µs and 5.958 µs (+2.9%). With the candidate first, they were 5.417 µs on v0.6.0 and 5.792 µs on the candidate (+6.9%). Both orders showed a smaller increase than the original captures. One batch per order cannot establish an order effect. [Grouped replay](grouped.json)

## Provider filtering and cache costs

This workload calls `AnalysisSession::check_with_ownership` with advanced ownership and finite matching disabled. It does not use the public options entry point that copies source files and appends provider sources. The shared internal path does filter provider-origin contracts, diagnostics and coverage reasons, then resets the coverage status when no reasons remain. The fixture returned two contracts, zero diagnostics and complete coverage. [Candidate check path](../../../../../../crates/codepact/src/db.rs)

An instrumented build of each revision timed six intervals in the first unchanged call. A third arm used the same candidate binary with the filtering and coverage reset bypassed. Each block ran the three arms sequentially. All six orders occurred three times, for 18 blocks and seven sessions per process. All arms asserted first-call equality with their cold result. The bypass is an experiment specific to this fixture. [Probe samples](probe.json) · [Baseline patch](baseline.patch) · [Candidate patch](candidate.patch)

| First unchanged call, median of process medians (µs) | v0.6.0 | 0.7.0 | 0.7.0 with bypass |
| --- | ---: | ---: | ---: |
| Entry, validation and options | 0.250 | 0.271 | 0.250 |
| Input synchronization, cached inspection and clone | 2.792 | 2.688 | 2.896 |
| Provider filtering and coverage reset, or empty interval | 0.041 | 0.084 | 0.042 |
| Flow query key and cached lookup | 1.313 | 1.167 | 1.313 |
| Clone and finalize inspection | 0.375 | 0.396 | 0.417 |
| Public result construction, model metadata and return | 1.188 | 1.146 | 1.166 |

The filtering interval exceeded the empty candidate interval by about 0.04 µs, close to the observed timer granularity. Bypassing it did not yield a measurable speedup in the full first call. The median paired change from candidate to bypass was +0.313 µs, with a wide interval around the percentage estimate (−9.8% to +18.2%). The interval measurements include clock overhead and their separate medians need not add to a total.

After timing the first check, the probe measured a batch of 256 further unchanged calls per session, including result destruction. The candidate's paired median change from baseline was +0.032 µs per call (+1.1%, bootstrap interval −0.1% to +2.4%). Enabling the bypass changed this by +0.003 µs relative to the regular candidate (+0.1%, −0.9% to +1.2%). This measures sustained session reuse, a different workload from the release metric.

## Technical disposition

The balanced comparison did not reproduce a 23% increase. It placed the paired median change at +0.5%, with an upper bootstrap endpoint of +5.4% on this host. The sustained-reuse timings and the candidate bypass also give no evidence that provider filtering accounts for the original 1.875 µs gap. The historical runs confounded version with time of execution. Variability between runs is a plausible explanation, but these measurements cannot attribute the original gap to a specific host or code effect.

Recommend treating this performance finding as non-blocking for 0.7.0 and retaining the original captures alongside the follow-up. G4 acceptance and pre-tag approval belong to the accountable maintainer.

## Release harness replay

The default release capture on `41e07ae` ran all seven workload configurations with ten processes per version or candidate-only workload. The paired median for the first unchanged exclusive check was +0.438 µs (+6.5%). Six of the ten candidate processes were slower than their adjacent baseline. Across all comparable metrics, the largest relative increase was +6.5%. The compiled benchmark inputs were unchanged in the subsequent documentation-only revision. [Raw release harness capture](paired-harness-replay.json)

## Replay

Run the following commands from the investigation checkout with Python 3.11+, Cargo, patch and a new scratch directory. The first command summarizes the committed samples. The second builds both revisions, applies instrumentation to scratch copies and repeats the captures. The raw stock times are milliseconds. Probe stage and batch-average times are nanoseconds.

```sh
python3 docs/planning/phases/0.7/evidence/exclusive-investigation/summarize.py
sh docs/planning/phases/0.7/evidence/exclusive-investigation/replay.sh /tmp/codepact-g4-replay
```

# 0.4 synthetic preview qualification

This record implements the maintainer-approved [AD01-A2 protocol](../../synthetic-preview-protocol.md).
It distinguishes technical controls from the final accountable maintainer gate
review and exact-distribution verification in the [release ledger](../../../../../releases/0.4.0-evidence.md).
Real-world usefulness remains unqualified. The original field-pilot observations
are unchanged and are not scored as synthetic successes.

## Reproduction

Build the optimized CLI and examples, then run the [qualification harness](reproduce.py)
from the repository root. Pass a candidate executable or an installed exact
candidate wheel executable, the output directory, and the matching published
0.3 benchmark directory. macOS memory measurements use `/usr/bin/time -l` and
`ps`; the process must have permission to read those host counters.

```sh
cargo build --release --bins --examples --locked
uv run --locked python docs/planning/phases/0.4/evidence/qualification-2026-11-10/reproduce.py \
  /absolute/path/to/codepact /tmp/codepact-qualification \
  /absolute/path/to/published-03-benchmark-examples
```

The 0.3 comparison uses the downloaded published 0.3 crate with the same current
benchmark generators. Only sample-output instrumentation is supplemented;
analyzer sources are unchanged. The harness retains its crate hash and both
benchmark executable hashes. Build those examples with the same Rust toolchain
and optimization profile as the candidate.

The harness checks the source/API against the supplied executable, records
full CLI/corpus outputs and schema results, runs Ruff and strict Pyright,
collects seven optimized samples with query-event assertions, and measures
ordinary/near-limit discovery/configuration, baseline matching, rendering,
all preview variants, inherited workloads, per-workload peak RSS and retained
session memory. These are engineered controls, not field precision or recall.

Exact installed-wheel replay uses [the artifact replay runner](replay_artifact.py): it verifies frozen source hashes, reruns the 24 release/four synthetic controls plus schema/config checks, and compares all 72 reports to the retained source run. Only absolute `project_root` and `config_file` display paths are omitted; findings, spans, certainty, policy and snapshot IDs must remain identical. The release ledger records the exact binary/package hashes and comparison digest.

## Assertion mapping

Each row below names actual assertion groups; a reference alone is not a gate
pass. Commands, source/binary hashes, raw outputs and sample records accompany
the final retained evidence. Final artifact replay and maintainer approval are
recorded separately in the release ledger.

| V04 | Controls and independently expected outcomes |
| --- | --- |
| 01 | `v04_01_equivalent_invocations_keep_config_and_path_origins` plus existing explicit-root/config tests: equivalent nested/absolute invocations agree, nearest configuration wins, no ancestor merge, default/CLI origins stay explicit. |
| 02 | Retained 23-case config runner and wrong-type/value CLI tests: actionable failures use 2 without a completed check; unrelated tool tables accepted. |
| 03 | Scope/dedup/collision/exclusion controls, selected-file and traversal unit limits: excluded dependencies remain boundaries, operands cannot bypass policy, exactly 10,000 physical inputs accept aliases, overflow fails and recovery succeeds. |
| 04 | Relative/absolute config/root/baseline controls, duplicate basenames/package initializer, symlink input/destination/escape controls: deterministic logical identities and no outside traversal. |
| 05 | Published schema validator and consumer version/missing-field controls; legacy/expanded assurance assertions: inspection schema 0 and Rust check schema 1 remain distinct from adoption schema 2; unexecuted rules and inspection-only results cannot claim completed checks. |
| 06 | Cross-format and input-order tests, concise seven-field round trips, adversarial tabs/newlines/quotes/Unicode labels: canonical diagnostics stay separate from presentation and stdout JSON remains decodable. |
| 07 | Unicode/emoji/CRLF primary and related byte-span checks, multiline AST-site controls and external-editor conversion tests: current-source coordinates and exclusive ends remain accurate; malformed/out-of-scope locations rejected. |
| 08 | Full definite/possible mode/threshold/severity/selection matrices, independent policy-only possible fixture, mandatory errors, suppression plus incomplete coverage: exact 0/1/2 exits without changing certainty or erasing coverage. |
| 09 | Multiple exact-code, primary-line/multiline, reasoned/unused/malformed/noneligible/unknown directives: audit findings retained, errors unsuppressible and coverage unchanged. |
| 10 | Baseline scope/contract/import changes, token-equivalent whitespace/multiline calls, distinct same-line AST sites, rename, certainty/trust/completion and engine/target/model/rule revisions: stale/changed evidence remains active; harmless movement matches. |
| 11 | Duplicate/collision insertion/reordering and unsupported/duplicate baseline documents; atomic interruption, byte boundary and recovery: no ordinal fallback, ambiguous findings remain active, previous bytes survive failed commit and temporary files are cleaned up. |
| 12 | Producer/consumer cross product for decorator/alias/verbose/duplicate metadata and bare/assignment/underscore/return/argument/discard forms: one immediate TP3001 per bare discard; no eventual-use proof or propagation. |
| 13 | Captured/re-exported aliases, rebinding/shadowing, known instance/static/class methods and reversed wrapper controls: exact identity governs recognition; unsupported order remains a boundary. |
| 14 | Official and lookalike discard, owning factories, argument raises and cleanup overlap: explicit discard removes only result-use duty; actual source effects remain and missing cleanup is still reported. |
| 15 | Misplaced markers, dynamic/composite uses, async/generator functions and methods, nested-yield negative control: errors or boundaries remain explicit, no invented synchronous TP3001 or inline generator execution, inherited possible duties retained. |
| 16 | Helper identity/signature/introspection, lightweight import, positive/negative Mypy/Pyright, producer/consumer combinations and CPython 3.11–3.14 CI: underlying types remain unchanged and unrelated mismatches fail. |
| 17 | Bounded explanation/detail tests, source/config/model/baseline stale-consumer controls and source-span attribution: unavailable/stale evidence requires recheck and trust is preserved. |
| 18 | Inherited V03/CS01 regression suite, CLI/API canonical comparison and async corpus: acquisition/duty/close/handoff/capture/cancellation locations agree; unsupported task/process boundaries do not become definite task-lifetime violations. |
| 19 | Full/limited counts and exit tests, 32-related/8-KiB UTF-8 detail controls, inclusive JSON/text byte limits, framing accounting, serializer failure and broken stdout/stderr/both-stream tests: no partial-clean claim; hard output failure uses 2. |
| 20 | Optimized seven-sample no-op/mode/threshold/severity/selection/suppression/baseline scenarios with zero semantic flow-query executions and fresh report parity; scope/exclusion controls refresh selected inputs. |
| 21 | Reused/deleted/renamed/imported-effect/must-use/target/location/failed-parse/recovery mutations with fresh session/one-shot/report/baseline parity, plus inherited model/root/cancellation mutations: affected facts and locations refresh while unrelated flow reuses. |
| 22 | Dependent repair fixture and corpus pairs: source repair clears intended findings with unchanged policy; an incomplete cleanup “repair” reveals its remaining duty; policy-only hiding does not qualify as repair. |
| 23 | Clean candidate distribution installation, native artifact smokes, settings/capabilities, supported repair, baseline update/reuse and CI exits; project source/config/AGENTS remain unchanged by checks. Exact-artifact replay is required before final signoff. |
| 24 | Non-execution/read-only fixtures, instruction-like data, invalid UTF-8/input/output cases, cancellation and inherited budget matrices, exact config/baseline/source/file limits and atomic recovery: fatal errors do not become completed clean reports. |
| 25 | Capabilities/explanations for selected stable rules and reference consumer: unsupported schema/snapshot versions, failed/limited/incomplete/out-of-scope reports never count as a policy pass; no LSP/MCP/task-ownership support claim. |
| 26 | AD01-A2 census: 33 planted cases, eight source repairs, four explicit boundary projects, three source targets, all 20 Ruff/Pyright-clean variants, and exact-distribution replay. This is synthetic preview correctness; real-world usefulness is unqualified. |

The primary assertion sources are [release controls](../../../../../../crates/codepact/tests/release_qualification.rs),
[Phase 0.4 regressions](../../../../../../crates/codepact/tests/phase04.rs),
[session/query controls](../../../../../../crates/codepact/src/db.rs),
[synthetic oracles](../../../../../../crates/codepact/tests/synthetic_projects.rs),
[consumer controls](../../../../../../tests/python/test_report_consumer.py),
[helper controls](../../../../../../tests/python/test_adoption_helpers.py), and
[inherited V03 controls](../../../../../../crates/codepact/tests/phase03.rs).

## Qualification fixes

The controls exposed and corrected generator result-use classification,
whitespace and same-line baseline identity, physical-file alias counting,
unbounded human-output buffering and broken stderr handling. Baseline algorithm
5 uses canonical token/site evidence with cached anchors. Scope guards reject
lifecycle-only results when must-use was selected, and reject cancelled or
inspection-only results; snapshot identity includes the analyzed scope and rule/
fingerprint revisions. The schema independently rejects the corresponding
unchecked-rule claim. Historical failed expectations about additive file
operands and incomplete repeated-close coverage were corrected against the
published contracts; they were not treated as checker defects.

## Retained final source results

The source freeze in the release ledger contains the exact source hashes in
[metadata](metadata.json); `base_revision` is the development parent, not a
claim that its unmodified tree produced these results. All analyzer files match
the frozen tree. The [raw archive](raw-results.tar.gz) retains 127 CLI command/output
records, 72 complete synthetic reports, schema/static-tool/consumer logs,
query-event assertions and individual optimized samples. The
[full check log](full-check.log.gz) retains the successful complete `make check`,
including a second test run from the packaged crate and clean CLI installation.

The full source check passed 480 Rust tests (including 198 inherited phase-0.3,
36 phase-0.4, 24 release controls and four synthetic tests), 39 Python tests,
strict Python typing, Ruff, Cargo format/check/clippy/MSRV/package/install,
inspector non-execution, nine schemas/13 fixtures/eight emitted documents, and
the strict Sphinx build. Dependency audit used the retained November 9 advisory
database with network fetching disabled; final CI audits fetch current data.
The qualification harness then passed all 24 release controls, four synthetic
tests, 23 configuration cases, eight consumer tests and 20 strict-Pyright/Ruff
variants against the optimized candidate executable. Every source target
(3.11, 3.12, 3.13) matched all 33 planted cases, eight repairs and four boundaries.
These are engineered regression outcomes, not field precision or recall.

## Operational results and regression review

Environment: Apple M4 Pro, macOS ARM64, rustc 1.96.0, optimized release profile;
exact OS/tool/runtime identities and binary hashes are in metadata. Seven
samples per timing scenario; no cache flush. CLI timings include process launch
and captured output. `/usr/bin/time -l` supplies per-workload peak RSS; `ps`
supplies retained-session RSS after dropping returned diagnostics. These coarse
process counters are not allocator counts or guarantees.

Lifecycle timings in milliseconds, median (minimum–maximum), using identical
corpora and harness generators for published 0.3 and this candidate:

| Revision | Resources | Cold | Warm no-op | Unrelated edit | Imported effect edit | Location edit | Peak RSS MiB |
| --- | ---: | --- | --- | --- | --- | --- | ---: |
| candidate | 1 | 0.340 (0.258–2.275) | 0.005 (0.005–0.008) | 0.127 (0.124–0.157) | 0.127 (0.126–0.142) | 0.113 (0.112–0.114) | 5.47 |
| candidate | 250 | 6.974 (6.874–7.413) | 0.464 (0.462–0.472) | 5.859 (5.726–5.979) | 8.026 (8.000–8.139) | 5.515 (5.497–5.670) | 14.67 |
| candidate | 1000 | 28.185 (28.112–29.238) | 1.792 (1.787–1.847) | 24.280 (24.021–24.799) | 33.490 (32.997–34.557) | 24.164 (23.659–24.307) | 42.62 |
| published-0.3 | 1 | 0.135 (0.121–1.161) | 0.005 (0.004–0.006) | 0.116 (0.114–0.137) | 0.117 (0.116–0.130) | 0.103 (0.102–0.104) | 5.23 |
| published-0.3 | 250 | 6.343 (6.318–6.961) | 0.372 (0.371–0.391) | 5.539 (5.487–5.606) | 7.688 (7.650–7.707) | 5.358 (5.349–5.372) | 13.88 |
| published-0.3 | 1000 | 30.936 (30.277–32.227) | 1.470 (1.464–1.510) | 27.053 (26.984–27.256) | 36.861 (36.436–37.682) | 26.107 (26.021–26.257) | 51.48 |

Cleanup/async timings in milliseconds. Peak RSS covers each combined process.

| Revision | Functions | Workload | Cold | Warm | Peak RSS MiB |
| --- | ---: | --- | --- | --- | ---: |
| candidate | 16 | Cleanup | 1.608 (1.566–5.145) | 0.004 (0.004–0.006) | 6.97 |
| candidate | 16 | Async | 0.704 (0.694–0.789) | 0.020 (0.020–0.024) | 6.97 |
| candidate | 128 | Cleanup | 43.986 (43.481–45.349) | 0.005 (0.005–0.009) | 52.36 |
| candidate | 128 | Async | 16.502 (15.961–16.966) | 0.136 (0.136–0.157) | 52.36 |
| published-0.3 | 16 | Cleanup | 1.491 (1.457–2.512) | 0.004 (0.004–0.005) | 6.92 |
| published-0.3 | 16 | Async | 0.635 (0.628–0.715) | 0.016 (0.016–0.018) | 6.92 |
| published-0.3 | 128 | Cleanup | 43.608 (41.716–44.483) | 0.005 (0.005–0.009) | 52.06 |
| published-0.3 | 128 | Async | 15.795 (15.722–16.489) | 0.107 (0.106–0.114) | 52.06 |

At 1,000 resources, candidate cold/imported/unrelated/location medians improve
by about 9%/9%/10%/7%; warm no-op rises from 1.470 to 1.792 ms (+22%). At
250 resources, cold and edit medians rise about 3–10%; warm rises from 0.372
to 0.464 ms (+25%). The one-resource cold median rises by 0.205 ms; its first
process samples are outliers in both versions. Async warm medians rise from
0.0158 to 0.0204 ms and 0.1068 to 0.1362 ms (about 28–29%); cleanup cold
rises about 8% at 16 functions and 1% at 128. These regressions are disclosed,
not erased by reporting only improved scenarios. Query-event controls establish
that policy/no-op checks execute zero flow queries; canonical result transport
now retains additional contract evidence for safe baseline matching. This is
consistent with additional result-copy/identity overhead, not semantic
recomputation; the measurements do not isolate allocator costs causally.

Retained RSS is 23,872 KiB (baseline 1,984; delta 21,888) versus published
0.3's 21,712 KiB (baseline 1,984; delta 19,728): +2,160 KiB, approximately
11% incremental retention. The 1,000-resource process peak is 42.63 MiB
versus 51.48 MiB; combined 128-function cleanup/async peak is 52.36 versus
52.06 MiB. There is no declared absolute latency or RSS SLA. G4 requires
maintainer review of these measured costs within the published fixed budgets.

[Seven policy sample sets](session-samples.json) separate semantic retrieval
from report processing and assert zero flow-query executions for no-op, strict,
threshold, severity, rule selection, suppression and baseline edits. Source,
effect, target, location, parse-failure/recovery and file lifecycle mutations
independently assert fresh/session/report/baseline parity and unaffected reuse.

[Adoption CLI samples](adoption-cli.json), milliseconds:

| Workload | Median (range) | Output bytes | Peak RSS MiB |
| --- | --- | ---: | ---: |
| mustuse-json | 75.479 (74.068–78.089) | 289934 | 18.78 |
| mustuse-limited | 73.988 (73.079–77.096) | 19120 | 18.77 |
| mustuse-text | 73.546 (72.458–75.576) | 65026 | 18.78 |
| mustuse-concise | 72.241 (71.944–76.274) | 41148 | 18.84 |
| baseline-dense | 75.657 (74.012–76.214) | 289220 | 19.02 |
| baseline-empty | 73.729 (72.490–74.680) | 289955 | 18.83 |
| baseline-changed | 76.072 (73.576–77.996) | 290383 | 19.09 |
| baseline-ambiguous | 76.384 (74.917–78.101) | 290449 | 19.05 |
| config-33 | 2.954 (2.834–3.339) | 4562 | 3.88 |
| config-65536 | 2.839 (2.784–2.971) | 4568 | 4.05 |
| discovery-10 | 3.475 (3.350–3.627) | 5788 | 4.41 |
| discovery-10000 | 2121.079 (2099.753–2173.769) | 1484314 | 68.17 |
| source-bytes-1000000 | 5.601 (5.499–6.557) | 4465 | 8.61 |
| source-bytes-1000001 | 2.465 (2.391–2.649) | 376 | 2.22 |

All 20 preview variants have seven cold CLI samples in [measurements](measurements.json). Their medians range from 2.917 to 3.880 ms; every report hash is stable across repeats, every source hash remains unchanged, and coverage/counts remain explicit.

Retained artifact SHA-256:

- `raw-results.tar.gz`: `959ff7363a4af351633083d6a3bbe7800f618726cfdf06c106c1fdf47590cac8`
- `full-check.log.gz`: `7ab8ae23059a63554eb7be276c1ab057b8ea5ec85630fae5b041aa1c6e61a7b3`
- `metadata.json`: `2d29e9bb87ca83802c294930c7bf41236a9f7d364c06cad0ed95aeea4587ee7f`
- `measurements.json`: `4212a92a7329600479c47273f50c74c21a5ecad0bae1d0467709b8c1ccff965e`
- `session-samples.json`: `c86c31817f6a550bc585cc7e57b34f3609ef9379c7a703f44ce068cb1603303d`
- `adoption-cli.json`: `00a00914cb371ae6e7bfc5128aae8d73b4c1cec306aab0b6e2d5cb8ce29bdaed`
- `configuration.json`: `aec4cd16d8124b8af7e153528112bf68acb7731be2c2325cbbea8eb9caee26a9`

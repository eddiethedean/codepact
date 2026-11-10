# 0.4 validation matrix

Status: **Source controls pass; final artifact verification and accountable maintainer gate approval are recorded separately in the release ledger.**
The maintainer-approved [AD01-A2 protocol](synthetic-preview-protocol.md) qualifies
an engineered synthetic preview and explicitly leaves real-world usefulness
unqualified. The [retained qualification](evidence/qualification-2026-11-10/README.md)
maps every row below to assertions, commands, complete reports and measurements.
The [release ledger](../../../releases/0.4.0-evidence.md) identifies the frozen
candidate, exact-revision CI, distribution replay and final gate decisions.
Historical pilot and partial performance records retain their original limits.
IDs are independent of V01/V02/V03; the [specification](specification.md) and
[phase plan](../0.4-adoption.md) supply the accepted contracts and gate mapping.

## Required matrix

Each applicable row needs valid, invalid and unsupported controls. Assert canonical findings independently from rendering: code, primary/related spans, completion, certainty, trust, coverage and policy disposition. Record exact fixture/test references as implementation lands; a test name proposed in an issue is not evidence.

| ID | Fixture / action | Required result | Gates |
| --- | --- | --- | --- |
| V04-01 | Defaults; explicit/nearest/no config; nested invocation directory, ancestor and nested pyprojects; explicit root vs configured root | Exactly one documented config and path base; no implicit merges; effective values and origins reproduce across equivalent invocations | A03 |
| V04-02 | Unknown keys, wrong types, duplicate keys, missing/unsupported config-version, invalid target/rule/severity/pattern and conflicting flags | Exit 2 with actionable category/key/location where available; no completed-check claim or ignored typo; unrelated tool tables accepted | A01, A03 |
| V04-03 | Includes/excludes at root/nested paths; zero selected files; explicit excluded file; built-in skips; duplicate physical inputs and module collision | Excludes win; selected scope deterministic; explicit operands cannot bypass policy; excluded dependencies remain boundaries; collision/empty/input failure exits 2 | A03, A06 |
| V04-04 | Relative/absolute config, root, input and baseline paths; symlink file/directory escape; duplicate basenames and package initializer | Correct logical module identity and containment; no outside traversal or installed-package discovery; JSON logical paths and root identity unambiguous | A03, A08 |
| V04-05 | Checker/inspection/baseline/capability/explanation schema fixtures; missing required fields, invalid enum, unknown schema versions | Every emitted document validates against its own published schema; invalid consumer inputs rejected; inspection schema 0 preserved; checker 1→2 change documented without implying a legacy adapter | A01, A08 |
| V04-06 | Same source/settings via text, JSON and concise; reorder input files, repeat invocation; spaces/colons/tabs/newlines in labels/messages | Same underlying findings, ordering, certainty, coverage and policy; concise records round-trip through documented decoder; operational logs do not corrupt stdout JSON | A01, A08 |
| V04-07 | Unicode identifiers, combining characters, emoji, CRLF, multiline calls, cross-file related spans; location-only edit | One-based UTF-8 byte columns and exclusive JSON ends point to current source; related role labels retained; one external-tool recipe resolves locations | A01, A06 |
| V04-08 | Every policy truth-table row × advisory/strict × error/warning threshold × severity override; selected/disabled rules; suppression plus incomplete coverage | Exact documented 0/1/2 exits and reasons; mandatory errors cannot be demoted; strict possible/coverage failure not inferred from filtered severity; mode never changes certainty | A01, A02 |
| V04-09 | Exact-code reasoned suppression; multiple codes; multiline primary vs related site; malformed/unknown/noneligible code; unused suppression | Only eligible findings starting on that line suppressed; raw findings/reasons remain auditable; invalid directive is unsuppressible error; unused directive visible; coverage unchanged | A02, A03 |
| V04-10 | Baseline existing finding, new site/rule, moved lines, whitespace/comment edit, rename, changed declaration/effects/certainty/trust/completion and changed target/model/rule revision | Match only unchanged unambiguous evidence; new or incompatible findings visible; harmless location edits preserve match; stale/incompatible entries reported | A02, A07 |
| V04-11 | Duplicate identical sites; insert/reorder identical violation; duplicate baseline entry; fingerprint collision; unknown fingerprint/baseline version | At most one-to-one unambiguous matching; ambiguity surfaces all affected findings; no ordinal/line fallback; malformed/unsupported baseline exits 2 | A02 |
| V04-12 | Decorator, MustUse alias and verbose metadata; duplicate same contract; direct call then bare discard/assignment/underscore/return/argument | Equal canonical result-use bit and one TP3001 per bare discard; assignment/forwarding count only as immediate handling; no automatic propagation or eventual-use proof | A04 |
| V04-13 | Imported alias/re-export, function alias captured before rebinding, shadowed/lookalike names, known instance/static/class methods and decorator ordering | Exact resolved identity controls recognition; no spelling-based behavior; qualified descriptor cases agree; reversed/unknown wrapper order error/boundary follows accepted AD01 decision | A04, A06 |
| V04-14 | discard(marked_call()), lookalike discard; argument raises; must-use resource factory dropped vs explicitly discarded; alias/cleanup interaction | Explicit discard clears only immediate TP3001; argument effects/raises and TP1002 duty remain; no implicit close/transfer or exception swallowing | A04, A06 |
| V04-15 | Misplaced metadata, unsupported annotation/decorator, unresolved dynamic callable, marked async/generator call/await, discarded composite expression | Defined source error or coverage boundary; no fabricated result use/async execution; inherited cleanup/cancellation findings retained | A04, A08 |
| V04-16 | Python helper import/identity/signature/introspection; supported interpreter/type-checker matrix; underlying T accepted and unrelated type mismatch rejected; mixed producer/consumer interfaces | Lightweight helpers load no native engine; decorator/annotation preserve ordinary typing and call semantics; all qualified forms normalize equally with origins retained | A04, A06 |
| V04-17 | Bounded explanation for cleanup failure and unsupported async operation; content hashes/snapshot check before/after source/config/model/baseline edits; moved related spans | Current-source evidence and policy identity; stale references rejected by consumer or trigger recheck; declared-unverified trust unchanged; missing evidence explicit | A08, CS02 |
| V04-18 | Existing CS01 sync/async corpus: acquisition, owning return, coroutine creation/capture, close, await/cancel and unsupported task/process escape | Same 0.3 semantics; modeled related sites have correct roles; absent task/join evidence stays absent; unsupported concurrency shown separately from definite violations | A01, A06, CS02 |
| V04-19 | More findings/related sites/snippet bytes than detail limits; only hidden findings actionable; full-output byte cap; serialization/broken stream | Counts, truncation and coverage explicit; exit calculated from full result; no truncated-clean claim; hard output failure exits 2; bounded rerun has verifiable snapshot | A01, A08 |
| V04-20 | Reused session: change mode, threshold, severity, rule selection, include/exclude, inline directive or baseline content; compare no-op and fresh check | Full result/disposition/coverage/span parity; presentation-only edits reuse canonical semantic queries; scope/rule changes refresh needed inputs; query events prove reuse independently of timing | A02, A07, CS02 |
| V04-21 | Reused session: edit must-use return/decorator/callee, imported cleanup/cancellation, root/target/model; rename/delete file; change config after failed run | Affected dependents refresh, unrelated work reuses; baseline identities do not resurrect stale findings; fresh/reused parity and recovery with current locations | A02, A07 |
| V04-22 | CLI check–edit–recheck: supported definite lifecycle or must-use violation, plus unresolved-boundary example; attempted policy-only “repair” | Supported source fix clears intended finding with affected-dependent check and relevant tests; policy fingerprints stable; new findings/remaining coverage reported; weakening policy fails repair evaluation | A06, A08 |
| V04-23 | Clean install → effective settings → useful finding → source repair → CI; baseline create/update explicitly; opt-in agent snippet and external-tool recipe | Reproduced commands/exits/reports on candidate artifacts; AGENTS.md/config/source unchanged by installation/check; one useful supported finding and unresolved coverage interpretation demonstrated | A06, A08 |
| V04-24 | Side-effectful source/instruction-like comments/filenames; unreadable/config/output failures; request cancellation/recovery; file/source/flow/report limits; inherited tests | No target execution/hooks/network/source writes; execution failures distinct from modeled Python cancellation; fatal vs recoverable budgets follow policy; mandatory inspector/helper/V03 regressions pass | A01, A03, A06, A07, A08 |
| V04-25 | Capabilities and static explanations for all selected rules; unsupported consumer versions; status/error envelopes vs completed findings; limited output consumed by CLI agent | Discoverable rules/targets/schemas/limits; no promised LSP/MCP/pagination; advisory repair preconditions and source attribution; incompatible/truncated/failed output never interpreted as verified clean | A01, A08 |
| V04-26 | Frozen independent synthetic bug/repair and boundary corpus including async; static-tool controls, onboarding and exact-artifact replay | Meet the [AD01-A2 protocol](synthetic-preview-protocol.md); all planted-case oracles and repairs agree; unsupported boundaries remain explicit; real-world usefulness unqualified | A05, A06, CS02 |

CLI portions of LE-V01/LE-V02 map to V04-05–07, V04-18 and V04-25. CLI AI-V01 maps to V04-05–08/25, AI-V02 to V04-22–23, AI-V03 to V04-17/19/21/24, and AI-V04 to V04-04/24. LSP/MCP parity, unsaved-editor versions and client delivery remain 0.8 work; future structured-task repair fixtures follow CS03.

## Recorded technical qualification

The [assertion mapping](evidence/qualification-2026-11-10/README.md#assertion-mapping)
records the specific valid, invalid and unsupported outcomes for V04-01–26.
Source controls pass under the retained full local check and qualification run;
V04-16's cross-interpreter evidence and V04-23/26's final-distribution assertions
also require the exact-revision workflow/replay recorded in the release ledger.
These technical results do not sign the accountable maintainer's release gates.

| Rows | Source result | Evidence |
| --- | --- | --- |
| V04-01–04 | Pass | Config equivalence/origins, 23 invalid/valid config controls, containment, excluded dependencies, module collisions, exact physical-input limit and recovery. |
| V04-05–08 | Pass | Nine schemas, valid/invalid fixtures and eight emitted documents; canonical formats/UTF-8/CRLF; separate definite/possible policy cross products and mandatory failures. |
| V04-09–11 | Pass | Exact suppressions, token/site and resolved-contract identity, changed semantic evidence, ambiguity, unsupported versions, interrupted atomic replacement and inclusive byte limits. |
| V04-12–16 | Pass (source) | Producer/consumer matrix, exact callable identity, cleanup/raise effects, async/generator/dynamic boundaries, helper typing/introspection. Interpreter matrix is in the release ledger. |
| V04-17–19 | Pass | Fresh identity/source consumer, explanation/detail limits, inherited cleanup roles, bounded rendering/framing and broken-stream failure controls. |
| V04-20–21 | Pass | Seven-sample policy/no-op query-event controls and semantic/file/target/model/root mutations with one-shot/fresh/reused parity and failure recovery. |
| V04-22–25 | Pass (source) | Unchanged-policy dependent repair, non-execution and source/AGENTS preservation, strict consumer, capabilities and hard/recoverable budgets. Final installed-artifact onboarding is in the release ledger. |
| V04-26 | Pass (source) | All 33 planted cases, eight repairs and four boundary projects on three targets; 20 Ruff/strict-Pyright variants; 72 retained CLI reports. Exact final artifact replay is in the release ledger. |

Commands, source hashes, actual outputs, optimized samples and the full assertion
mapping are retained together. The source freeze identified by the release ledger
contains those exact sources; later ledger-only updates do not change them.

## Assertions and implementation seams

Keep three independent assertions: canonical semantic findings, policy dispositions/exit reasons, and formatted output. Golden text alone cannot prove that suppressed findings or coverage were retained. Baseline tests compare independently specified before/after finding sets and adversarial insertions; merely hashing the same implementation twice is not an identity oracle. Test baseline update create/replace refusal, deterministic entry order, explicit diff, invalid destination, interrupted write and no writes after failed analysis.

Cross all qualified Pythonic/explicit producer and consumer combinations. Normalize source-only declaration spellings when comparing canonical contracts; retain trust, certainty, obligation/effect semantics and coverage. Preserve separate snapshots for locations and public prose. Inheritance, arbitrary descriptors and dynamic call targets gain no support from ordinary typing success.

Compare fresh one-shot, fresh session and reused session at each mutation. Observe execution/reuse counts for semantic queries and policy/report stages. A faster no-op does not establish reuse or correctness. Source location edits must refresh all current primary/related spans even when normalized summaries are reused. Baselines must read validated content, not only a path/mtime cache key.

Add concrete evidence beside `crates/codepact/tests/cli.rs`, `tests/phase03.rs` and `tests/lifecycle_regressions.rs`, helper/checker tests under `tests/python` and `tests/checkers`, and schema/CLI fixtures at paths chosen in AD01. Extend quality CI for schema/configuration/policy/baseline/must-use checks. If adding a matrix-index validator, require exact V04 IDs and real declared tests; document that it guards references, not semantic qualification.

## Operational qualification

The final [operational record](evidence/qualification-2026-11-10/README.md) supersedes the partial [earlier measurements](performance-2026-11-09.md). G4 acceptance remains an accountable maintainer decision in the release ledger. Reuse the [released 0.3 baseline](../0.3/performance.md), its host/method disclosure and bounded corpora. Retain optimized seven-run medians/ranges, corpus revisions/content hashes, source bytes, diagnostic counts, host/toolchain/target/model settings and peak/retained-memory method for:

- Cold CLI and in-process checks, warm no-op, unrelated edit, imported semantic edit and location-only edit on the same lifecycle/cleanup corpora.
- A must-use corpus, ordinary and near-limit project/configuration discovery, empty/dense baselines, and many matching versus changed/ambiguous findings.
- Text/JSON/concise rendering and detail-limited reports, with total/returned counts and output bytes. Detail limits must not bypass full exit/coverage calculation.
- Mode/severity/baseline/suppression edits in reused sessions, with query events and semantic-versus-report timings recorded separately.
- All 20 accepted synthetic preview variants, including async cancellation/escape reporting; record supported scope and unknown boundaries alongside timing rather than implying whole-project coverage.

AD01 retains inherited analyzer budgets and freezes explicit caps for config/baseline bytes and entries, whole-report bytes, related-site/snippet detail and any new identity-matching work. Complete near-limit fixtures and record precise exhaustion/recovery behavior before qualification; no unlimited parser, matcher or baseline loader is accepted by omission. Review performance and memory regressions against 0.3 before G4 and explain accepted changes. No latency threshold or result is fabricated by this plan.

## Evidence ledger and acceptance

The [release readiness work plan](release-readiness.md) maps the completed source work and remaining final-candidate decisions. Its task names do not replace the assertion-level results above.

For every V04 row record status (Pending/Pass/Fail), exact candidate revision, fixture/test/command, environment/schema versions, actual result, artifact/log link and maintainer/date. Link one evidence item to multiple rows only with an explanation of which assertions it covers. Add pilot scorecards and raw labeled samples separately from deterministic test logs.

AD08 creates the candidate [release evidence record](../../release-evidence-template.md) and maps every A01–A08/CS02 and G1–G8 decision. G1–G7 precede tagging; G8 follows authorized publication. A required row without evidence remains pending. Planning link checks or a passing inherited suite do not qualify the new CLI, policy or semantic behavior.

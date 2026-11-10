# 0.4 validation matrix

Status: **AD01 scope is accepted; all V04 rows and technical phase gates remain pending.** The references below are partial automated evidence, not full-row passes. The [synthetic project evaluation](synthetic-projects-2026-11-09.md) exercises controlled findings, repairs and unsupported boundaries; it does not count toward A05. Partial pilot results and performance measurements are recorded in the [pilot results](pilot-results-2026-11-09.md) and [performance evidence](performance-2026-11-09.md); neither completes its qualification criteria. IDs are independent of V01/V02/V03. The [specification](specification.md) supplies the accepted AD01 contracts; the [phase plan](../0.4-adoption.md) maps work packages and A01–A08/CS02.

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
| V04-26 | Frozen three-project pilot including async; at least 30 definite violations; labeling, disagreement, clean onboarding and repeat candidate run | At least 90% actionable under [pilot protocol](pilots.md); counts/selection/current revisions/coverage disclosed; too little evidence holds gate; async boundaries separated from confirmed violations | A05, A06, CS02 |

CLI portions of LE-V01/LE-V02 map to V04-05–07, V04-18 and V04-25. CLI AI-V01 maps to V04-05–08/25, AI-V02 to V04-22–23, AI-V03 to V04-17/19/21/24, and AI-V04 to V04-04/24. LSP/MCP parity, unsaved-editor versions and client delivery remain 0.8 work; future structured-task repair fixtures follow CS03.

## Current candidate test references

These tests exercise a subset of the listed assertions. Each V04 row still
requires its full valid/invalid/unsupported controls and retained evidence
before it can be marked Pass.

| Candidate test or check | Partial assertions touched |
| --- | --- |
| `must_use_forms_report_discard_and_accept_assignment_and_explicit_discard` in `crates/codepact/tests/phase04.rs` | V04-12 |
| `must_use_tracks_local_captured_and_reexported_callable_identities` and `must_use_resolves_known_instance_static_and_class_methods` | V04-13 |
| `must_use_reports_unsupported_nested_and_async_uses_without_mislabeling_arguments` | V04-14/15 |
| `nearest_config_wins_and_invalid_project_settings_fail_before_analysis`, `project_config_strict_mode_and_rule_selection_are_applied`, and `settings_resolution_is_read_only_and_reports_origins` | V04-01/02, partial V04-03 |
| `explicit_config_and_root_overrides_have_one_deterministic_precedence` and `project_config_wrong_types_and_invalid_values_fail_with_actionable_errors` | Additional V04-01/02/04 assertions for explicit config/root precedence, canonical paths, missing/symlinked roots, wrong TOML value types and invalid settings; partial-row evidence only |
| `includes_and_excludes_define_scope_and_explicit_operands_cannot_bypass_excludes` | Partial V04-03 |
| `discovery_counts_only_files_selected_by_project_policy`, `discovery_accepts_exact_file_limit_with_later_empty_directories`, `discovery_rejects_the_next_selected_file_without_exceeding_storage_limit`, `discovery_bounds_total_entries_across_nested_directories`, `discovery_bounds_entries_buffered_for_each_directory`, and `discovery_bounds_directory_depth_before_recursing_further` in `crates/codepact/src/main.rs` | Selected-input counting and independent total/per-directory/depth bounds for V04-03/24; full near-limit, containment and recovery qualification remains open |
| `configuration_read_accepts_the_byte_limit_and_caps_consumption_on_overflow` in `crates/codepact/src/adoption.rs` and `project_config_byte_limit_is_inclusive_and_failure_recovers` | Bounded configuration read, exact 64 KiB acceptance, one-byte overflow and recovery for V04-02/24 |
| `advisory_strict_and_mandatory_severity_follow_policy_contract` and `policy_truth_table_keeps_severity_certainty_coverage_and_rule_selection_separate` | V04-08 |
| `line_suppression_is_reasoned_and_does_not_delete_the_audit_finding` and `invalid_suppressions_cannot_be_baselined_and_stale_targets_do_not_hide_findings` | V04-09/10 |
| `baseline_update_is_explicit_and_baseline_only_hides_matching_results`, `baseline_preserves_unambiguous_sites_and_surfaces_inserted_duplicates`, and `baseline_inputs_and_replace_targets_are_validated_before_source_discovery` | V04-10/11 |
| `baseline_replace_refusal_and_failed_input_preserve_the_previous_file` | Existing bytes survive a refused replacement and a fatal invalid-UTF-8 input; valid replacement succeeds and cleans up its temporary file for V04-11 |
| `suppression_targets_only_the_primary_start_line_and_retains_raw_findings` and `baseline_rejects_unsupported_versions_and_duplicate_entries_without_hiding_findings` | Partial V04-09/11 |
| `limited_json_serialization_accepts_exact_limit_and_rejects_overflow` and `limited_json_serialization_reports_serializer_errors_separately` in `crates/codepact/src/adoption.rs` | Exact output limit, overflow rejection and serialization-error classification for V04-19 |
| `adoption_policy_and_baseline_edits_reuse_canonical_session_queries` in `crates/codepact/src/db.rs` | Query-event reuse and fresh-result parity for selected V04-20 policy/suppression/baseline edits; partial V04-21 |
| `output_policy_keeps_full_counts_and_concise_records_have_seven_fields` | Partial V04-06/19 |
| `output_formats_keep_unicode_crlf_spans_and_escape_project_paths` | Cross-format finding identity, related-site decoding, UTF-8 byte columns, CRLF and JSON escaping for V04-06/07 |
| `legacy_rust_check_keeps_its_rule_set_and_schema` | Compatibility boundary for V04-05 |
| `capabilities_and_rule_explanations_publish_the_supported_surface_and_limits` | Partial V04-25 |
| `semantic_source_removes_only_lexer_identified_codepact_comments` in `crates/codepact/src/frontend.rs` | Preserves string contents and line endings while separating policy-comment inputs for V04-20 |
| `scripts/check_schema_fixtures.py` with `docs/schemas/fixtures/` | Schema syntax and valid/invalid fixtures plus emitted capabilities, explanation, check report, settings, failure, baseline, baseline-update, and inspection JSON for V04-05/25 |
| `synthetic_projects_match_bug_and_repair_oracles_on_all_targets`, `synthetic_source_repairs_have_fresh_reused_and_input_order_parity`, and `synthetic_unknown_boundaries_cannot_be_interpreted_as_verified_clean` in `crates/codepact/tests/synthetic_projects.rs` | Controlled bug/repair outcomes and unresolved-boundary handling for selected V04-12–15/18/22/24 assertions; synthetic results do not qualify the V04-26/A05 real-project sample |

The focused Rust suite is `cargo test -p codepact --test phase04`. The schema
fixture validator is run by `make check-docs`. These are local candidate checks;
they do not replace the exact-candidate matrix, pilot, performance or release
records.

## Assertions and implementation seams

Keep three independent assertions: canonical semantic findings, policy dispositions/exit reasons, and formatted output. Golden text alone cannot prove that suppressed findings or coverage were retained. Baseline tests compare independently specified before/after finding sets and adversarial insertions; merely hashing the same implementation twice is not an identity oracle. Test baseline update create/replace refusal, deterministic entry order, explicit diff, invalid destination, interrupted write and no writes after failed analysis.

Cross all qualified Pythonic/explicit producer and consumer combinations. Normalize source-only declaration spellings when comparing canonical contracts; retain trust, certainty, obligation/effect semantics and coverage. Preserve separate snapshots for locations and public prose. Inheritance, arbitrary descriptors and dynamic call targets gain no support from ordinary typing success.

Compare fresh one-shot, fresh session and reused session at each mutation. Observe execution/reuse counts for semantic queries and policy/report stages. A faster no-op does not establish reuse or correctness. Source location edits must refresh all current primary/related spans even when normalized summaries are reused. Baselines must read validated content, not only a path/mtime cache key.

Add concrete evidence beside `crates/codepact/tests/cli.rs`, `tests/phase03.rs` and `tests/lifecycle_regressions.rs`, helper/checker tests under `tests/python` and `tests/checkers`, and schema/CLI fixtures at paths chosen in AD01. Extend quality CI for schema/configuration/policy/baseline/must-use checks. If adding a matrix-index validator, require exact V04 IDs and real declared tests; document that it guards references, not semantic qualification.

## Operational qualification

Partial 0.4 measurements are recorded in [candidate performance evidence](performance-2026-11-09.md); they do not pass G4. Reuse the [released 0.3 baseline](../0.3/performance.md), its host/method disclosure and bounded corpora. Retain optimized seven-run medians/ranges, corpus revisions/content hashes, source bytes, diagnostic counts, host/toolchain/target/model settings and peak/retained-memory method for:

- Cold CLI and in-process checks, warm no-op, unrelated edit, imported semantic edit and location-only edit on the same lifecycle/cleanup corpora.
- A must-use corpus, ordinary and near-limit project/configuration discovery, empty/dense baselines, and many matching versus changed/ambiguous findings.
- Text/JSON/concise rendering and detail-limited reports, with total/returned counts and output bytes. Detail limits must not bypass full exit/coverage calculation.
- Mode/severity/baseline/suppression edits in reused sessions, with query events and semantic-versus-report timings recorded separately.
- All three pilots, including async cancellation/escape reporting; record supported scope and unknown boundaries alongside timing rather than implying whole-project coverage.

AD01 retains inherited analyzer budgets and freezes explicit caps for config/baseline bytes and entries, whole-report bytes, related-site/snippet detail and any new identity-matching work. Complete near-limit fixtures and record precise exhaustion/recovery behavior before qualification; no unlimited parser, matcher or baseline loader is accepted by omission. Review performance and memory regressions against 0.3 before G4 and explain accepted changes. No latency threshold or result is fabricated by this plan.

## Evidence ledger and acceptance

The [release readiness work plan](release-readiness.md) orders the remaining
implementation, pilot, operational and final-candidate tasks. It does not pass
any row or replace the assertion-level requirements above.

For every V04 row record status (Pending/Pass/Fail), exact candidate revision, fixture/test/command, environment/schema versions, actual result, artifact/log link and maintainer/date. Link one evidence item to multiple rows only with an explanation of which assertions it covers. Add pilot scorecards and raw labeled samples separately from deterministic test logs.

AD08 creates the candidate [release evidence record](../../release-evidence-template.md) and maps every A01–A08/CS02 and G1–G8 decision. G1–G7 precede tagging; G8 follows authorized publication. A required row without evidence remains pending. Planning link checks or a passing inherited suite do not qualify the new CLI, policy or semantic behavior.

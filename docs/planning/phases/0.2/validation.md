# 0.2 validation matrix

Status: **No row is accepted as qualified yet.** Existing source and tests cover part of this matrix; the candidate evidence record must link exact results before any row or gate is marked passed. The [specification](specification.md) is the oracle; the [phase plan](../0.2-lifecycle.md) defines L01–L06. IDs below are independent of 0.1 V01–V24.

## Required matrix

| ID | Fixture / action | Expected result | Gates |
| --- | --- | --- | --- |
| V02-01 | Exact Connection tutorial; remove final use for valid counterpart | Exactly one definite TP1001 at final call, related close span; valid counterpart has no violations/boundaries | L01 |
| V02-02 | Decorator/explicit lifecycle × shorthand/verbose access; conflicting metadata; undecorated base-only reference | Shared semantics agree; provenance retained; conflict is declaration error; alias metadata does not mutate base | L01, L04 |
| V02-03 | Alias then close; rebind alias before close; distinct constructions | Alias invalidation, rebinding detachment and independent objects | L02 |
| V02-04 | Close in one/both/neither branch then use | Possible/definite/absent finding; branch origins retained | L02 |
| V02-05 | Branch chooses a or b into x; close x, use originals; same-object counterpart | Weak updates for uncertain targets; strong only for same exact object | L02 |
| V02-06 | Branch-dependent alias/unbound local; alias-set budget | No open proof from missing paths or truncated sets; visible boundary as needed | L02, L05 |
| V02-07 | Close Borrowed directly, via alias/helper; non-consuming status counterpart | Forbidden close at operation with annotation provenance; supported read passes | L03 |
| V02-08 | Helper closes on some/all paths; same resource in two parameter positions; return before operation | Correct conditional effects, alias-aware order or explicit boundary; unreachable operation excluded | L03, L05 |
| V02-09 | Shadowing/rebinding, captured old aliases, function-local names, unrelated close methods | Semantics follow resolved identity; lookalikes gain none | L04 |
| V02-10 | Discover resource/helper through named re-exports; reorder entry files | Same contract/call identity, findings, ordering and coverage | L04 |
| V02-11 | Missing module, duplicate module, file/package collision, import cycle, symlink escape | Deterministic bounded error/boundary; no outside-root traversal | L04, L05 |
| V02-12 | Unknown helper/method receives resource; zero-argument unknown call after escape; resource storage/return | Reachable state unknown; escape persists; no guessed purity or alias relation | L03, L05 |
| V02-13 | Normal close vs try/except/finally; unsupported constructor/method; repeat close | Normal transition precise; exceptional/unsupported behavior conservative; declared trust retained; no invented repeat-close rule | L03, L05 |
| V02-14 | Change callee read to conditional/definite close; change annotation/model trust | Affected callers update; fresh and updated facts/findings/coverage agree | L06 |
| V02-15 | Add/delete module, change re-export/root/target version; unrelated edit | Relevant invalidation; unrelated caller checks reuse, proven by query events | L06 |
| V02-16 | Move provider close/annotation lines without changing effects | Related spans refresh; summary semantics unchanged; unaffected semantic work reused | L06 |
| V02-17 | No-op update; cancel/recover; recursion; CFG/alias/call/import budgets | No stale publication/panic; correct revision; session recovers; exhaustion visibly incomplete | L05, L06 |
| V02-18 | Side-effect marker; loop/with/async/closure; file reorder/repeat | Marker absent; relevant state weakened; deterministic results/coverage | L04, L05 |
| V02-19 | Rust one-shot/session vs text CLI; definite/possible/boundary/invalid-input | Facts/spans/certainty agree; documented exits 1/0/0/2; visible coverage | L01 |
| V02-20 | Clean tutorial/artifact install; inherited inspection/helper/checker suite | Reproduced finding; inspection output/assurance preserved; lightweight typing-compatible helpers | L01 |

## Automation currently present

These tests are regression evidence in the current source tree. They do not
qualify a V02 row until the exact candidate's run, environment, output and
reviewer are retained in the [release evidence record](../../release-evidence-template.md).

| Rows | Current automated evidence |
| --- | --- |
| V02-01 | `tests/cli.rs::check_reports_one_use_after_close_at_the_call_site`; `scripts/verify_installed_package.py` |
| V02-02 | `tests/cli.rs::lifecycle_metadata_forms_share_flow_semantics`; `tests/fixtures.rs::pythonic_and_explicit_access_lower_to_same_fact`, `incompatible_metadata_reports_both_origins`, `conflicting_alias_metadata_is_an_error` |
| V02-03–06 | `tests/cli.rs::assignment_rhs_keeps_the_previous_binding_until_its_effects_finish`, `check_alias_rebinding_and_uncertain_joins_preserve_object_state`, `check_exposes_unbound_aliases_and_alias_budget_exhaustion`; `src/flow.rs::alias_overflow_drops_exact_targets_and_weakens_all_candidates`, `control_flow_depth_exhaustion_weakens_resources_and_reports_a_boundary`, `object_budget_exhaustion_does_not_create_an_exact_place` |
| V02-07–08 | `tests/cli.rs::check_branch_join_is_possible_and_borrowed_close_is_rejected`, `inline_annotated_borrowed_resource_rejects_close`, `close_through_an_aliased_helper_reports_the_borrow_origin`, `helper_effects_preserve_alias_order_returns_and_branch_certainty`, `helper_resolution_uses_provider_scope_and_borrowed_preconditions`, `constructor_close_body_and_early_return_effects_reach_callers`; `src/db.rs::conditional_and_model_edits_refresh_imported_lifecycle_findings` |
| V02-09 | `tests/fixtures.rs::docstrings_and_shadowed_imports_do_not_create_contracts`, `rebound_classes_and_imports_lose_their_typepact_identity`, `eager_alias_reassignments_capture_the_original_declaration` |
| V02-10 | `tests/cli.rs::check_follows_acyclic_named_reexports_without_losing_resource_identity`, `check_applies_a_reexported_helper_and_retains_its_provider_location`, `project_check_is_stable_across_file_creation_order_and_never_executes_source` |
| V02-11 | `tests/cli.rs::project_discovery_rejects_module_collisions_and_skips_symlink_escapes`, `project_import_cycles_return_deterministic_incomplete_checks`; `src/db.rs::adding_and_removing_a_lifecycle_provider_matches_fresh_checks` |
| V02-12–13 | `tests/cli.rs::check_preserves_call_effects_and_reports_unknown_qualified_calls`, `unknown_effects_escape_and_unsupported_flow_keep_results_incomplete`, `unknown_method_effect_weakens_tracked_resource_arguments`, `assignment_of_a_resource_returning_close_method_stays_unknown`, `decorated_methods_report_a_boundary_without_assuming_instance_binding`, `custom_new_effects_are_reported_as_unknown`, `close_body_runs_before_the_designated_close_transition`, `repeat_close_is_an_unknown_boundary_without_a_double_close_finding`, `loops_with_async_and_nested_closures_report_incomplete_coverage`, `constructor_close_body_and_early_return_effects_reach_callers`; `src/flow.rs::call_depth_exhaustion_weakens_resources_and_reports_a_boundary` |
| V02-14–16 | `src/db.rs::conditional_and_model_edits_refresh_imported_lifecycle_findings`, `imported_effect_edits_refresh_callers_but_reuse_unrelated_checks`, `unrelated_edit_reuses_other_file_lifecycle_checks`, `moving_an_imported_close_refreshes_the_related_lifecycle_span`, `adding_and_removing_a_lifecycle_provider_matches_fresh_checks` |
| V02-17 | `src/db.rs::unchanged_session_check_reuses_the_check_query`, `cancellation_discards_the_result_and_the_session_recovers`, `session_revision_is_stable_for_noops_and_changes_with_settings`, `flow_budget_is_an_input_to_incremental_file_queries`; `tests/cli.rs::recursive_summaries_stop_with_an_incomplete_boundary`, `flow_work_budget_stops_exponential_helper_expansion` |
| V02-18 | `tests/cli.rs::project_check_is_stable_across_file_creation_order_and_never_executes_source`, `loops_with_async_and_nested_closures_report_incomplete_coverage` |
| V02-19 | `tests/cli.rs::check_reports_one_use_after_close_at_the_call_site`, `possible_findings_exit_zero_while_invalid_project_input_exits_two`, `completeness_regressions_have_the_documented_exit_status` |
| V02-20 | `Makefile::check-rust-package`, `scripts/verify_installed_package.py`, inherited `make check` targets |

Still required for qualification: retain candidate logs and normalized snapshots,
review missing-module/collision/exception cases against the written oracle, and
measure cold and incremental lifecycle performance, memory and query reuse on
fixed corpora. The numeric flow limits remain provisional until those results
and LC01 review are recorded.

## Assertions and differential checks

Each row needs concrete source files, primary/related spans, normalized facts or summaries, certainty, coverage reasons and exits where applicable. Include valid, invalid and unknown counterparts. Golden output does not replace assertions about state, joins or missing coverage.

Retain representative CFG/summary snapshots for straight-line, branch and return cases. Cross-interface comparisons remove source-only provenance/locations while retaining lifecycle, access, effects, certainty and trust. Keep location snapshots separately. Across session resets compare alias/state relationships rather than database-local object IDs.

Incremental cases start cold, apply an edit to a reused session, then compare with a new session on edited source. Record query execution events for invalidation and reuse; timings alone prove neither. Compare findings, facts, spans, certainty, provenance, coverage and revision ownership.

Bounded generated branch/alias cases can additionally check that joins never improve uncertainty into safety and replacing a known summary with unknown never creates an open proof. They complement required counterexamples.

## Qualification record

For every row retain fixture path, candidate revision, invocation, expected/actual result, log/snapshot link and reviewer. Pending fixtures are not passes. Record an issue/owner for failures. Re-run affected rows after changes; explain reuse of unaffected evidence.

Measure fixed small/medium corpora: cold/warm checks, body/effect/location edits, elapsed time, peak/retained memory and query counts. Record machine, toolchain, interpreter, corpus hashes and limits. Compare parsing/import measurements with 0.1. Accept numerical budgets in LC01 after measurement; unexplained regressions hold G4.

Run inherited formatting, static analysis, Rust, Python/checker and artifact checks as applicable. Local results do not imply new platform support. Complete L01–L06 and G1–G7 in the candidate record; maintainer tag authorization and post-publication G8 remain separate.

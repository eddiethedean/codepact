# 0.6 validation matrix

All rows remain pending as release-gate statuses. Exact-candidate local checks now cover parts of several rows, but required cross-form and failure-path controls remain absent and the accountable maintainer has not recorded gate review. The evidence is summarized in [local candidate qualification](evidence/worktree-checks-2026-11-14.md), [pinned model evidence](evidence/models-2026-11-14.md), and [candidate performance evidence](performance.md). The acceptance requirements remain those in this matrix and the [phase gates](../0.6-advanced-contracts.md#required-exit-gates).

## Exact-candidate local coverage

These are regression signals from candidate `1da46b68d8e0d9ebc226441abb3eea3c1c28f13f`, not substitutes for the complete observations required in each row.

| Rows | Exact-candidate evidence |
| --- | --- |
| V06-01–03 | CLI opt-in/rule selection and scope defaults, helper identity/signatures, state alias typing, re-exported declarations, local lookalikes, nonliteral metadata, invalid state sets and conflict locations in [`release_qualification.rs`](../../../../crates/codepact/tests/release_qualification.rs), [`phase06.rs`](../../../../crates/codepact/tests/phase06.rs) and [`valid.py`](../../../../tests/checkers/valid.py). Mypy, Pyright and Basedpyright pass the typing fixture. Mixed decorator/type-interface parity remains incomplete. |
| V06-04–08 | Same-object conflicts, exact region/access spans, live TaskGroup capture, pending coroutine coverage, raised-call region release, await/async-context/yield boundaries, known violations with unresolved alternatives, and explicit/omitted raise and cancellation outcomes in [`phase06.rs`](../../../../crates/codepact/tests/phase06.rs). Binding-failure, finally replacement and cancellation-before-start oracles remain incomplete. |
| V06-09–10 | Declared-unverified provenance, unresolved effects, declared exceptional-state recovery and a conservative recursive protocol call in [`phase06.rs`](../../../../crates/codepact/tests/phase06.rs). Checked-wrapper/reviewed-model comparisons, bound exhaustion and configurable precision recovery remain incomplete. |
| V06-11–12 | Local TaskGroup and executor analyzer fixtures, including nested groups, invalid states, target-pin selection, dynamic configuration and tracked worker-argument boundaries in [`phase06.rs`](../../../../crates/codepact/tests/phase06.rs); controlled upstream probes and source fingerprints in [pinned model evidence](evidence/models-2026-11-14.md). Worker startup and shutdown failure analyzer controls remain incomplete. |
| V06-13 | Protocol-edit invalidation and exact query reuse in [`db.rs`](../../../../crates/codepact/src/db.rs), plus fresh/reused report equality in [`phase06.rs`](../../../../crates/codepact/tests/phase06.rs). Additional session tests cover location/model/settings invalidation and cancellation recovery; the combined candidate query-event and bounded-task evidence required by the row remains incomplete. |
| V06-14–15 | Live CLI schema outputs in [`check_schema_fixtures.py`](../../../../scripts/check_schema_fixtures.py), and advanced CLI advisory/strict policy, rule selection, suppression, concise output and detail-cap coverage in [`release_qualification.rs`](../../../../crates/codepact/tests/release_qualification.rs). Inherited regressions and typing checks pass locally; reproducible advanced synthetic project bug/repair/boundary reports remain incomplete. |
| V06-16 | Retained and new optimized workloads have exact-candidate raw samples and a same-host `v0.5.1` comparison in the [performance evidence](performance.md). Unit tests exercise flow, alias, object and request bounds; integrated task-budget stress and the full query-event record remain pending. |

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V06-01 | G1, G3, G6 | Exercise default, ownership-only and advanced configurations through CLI and Rust, including CLI overrides, contradictory settings and strict mode by itself. Compare inherited reports for default and ownership-only runs after normalizing version and snapshot metadata. Test helper imports, typing, identity and signature preservation. |
| V06-02 | T02, G2 | Lower equivalent decorators and explicit metadata to the same canonical operation and exit records. Run Pythonic, explicit and both mixed-interface directions, including qualified aliases and supported re-exports. A rebound name and a local lookalike follow normal binding rules. |
| V06-03 | T02 | Duplicate identical metadata, conflicting transitions on the same outcome, invalid initial and target states, nonliteral metadata and incompatible access. Invalid definitions report a declaration error before seeding state. Equality ignores source-only spans while diagnostics retain them. |
| V06-04 | T01, CS04 | Accept sequential calls, calls on distinct instances and a local alias of the exclusive view. Exercise same-object shared/exclusive and exclusive/exclusive argument pairs, competing aliases, and a known live TaskGroup or pending coroutine borrower. Assert exact conflict origin and access spans. |
| V06-05 | T01 | Exercise region cleanup on raise and finally, argument and binding failures, borrowed close or move, known storage or returned escape, and an unknown callback. Check that failed entry never grants a region, prior argument effects survive, and unrelated duties remain intact. |
| V06-06 | T06, CS04 | Compare a completed exclusive call before await with a tracked view reaching await, implicit async entry/exit or yield. Include a pending creation without execution and cancellation before start. Assert visible coverage, advisory/strict exit policy and retained capture uncertainty. |
| V06-07 | T03 | Check a valid singleton, invalid singleton, branch union, unreachable branch, and a known violating branch paired with an unresolved branch. Observe definite, possible and coverage results as appropriate. Aliases observe the same receiver update. |
| V06-08 | T04 | Pair the same operation's normal transition, explicit raise postcondition, omitted exceptional postcondition, and raise before binding. Include replaced exit through finally and a cancelled cleanup. Assert states, handle validity and cleanup duties independently on each edge. |
| V06-09 | T05 | Compare a checked wrapper, trusted declaration, reviewed model, contradicted declaration and unresolved call. Verify provenance and that unknown effects imply neither unchanged state nor purity, no-raise, non-escape or discharged cleanup. |
| V06-10 | T05, T07 | Exercise resolved recursion with a base exit, nonconverging or unresolved recursion, and boundaries at the applicable flow, state and region limits. Repeat with a larger allowance where configurable to check precision recovery and preserve deterministic reports at each allowance. |
| V06-11 | T04, CS04 | Qualify TaskGroup submission before entry, during normal use and after exit, attempted re-entry, nested groups, pending or rejected scheduling by pin, sibling failure and cancellation. Assert group states, capture disposition, completion and resource duties separately. Include unsupported factory and escaped handle controls. |
| V06-12 | T04, CS04 | Qualify executor construction failure, normal submit, submit after shutdown, repeated shutdown, waiting and nonwaiting shutdown, worker startup failure and shutdown failure after state change. Include dynamic configuration and tracked worker captures. Distinguish protocol state from uncertain work and resource effects. |
| V06-13 | T07, CS04 | Edit declarations, caller/callee effects, capture/model outcomes, pins, targets, settings and spans. Compare fresh and reused canonical reports and exact related locations, record dependent query refresh and unrelated reuse, and verify recovery after analyzer-request cancellation. |
| V06-14 | T06, CS04, G1, G6 | Verify CLI text, concise and JSON agreement, mode/threshold combinations, suppression, baselines, model IDs and scope changes, detail caps and invalid input. Publish the supported subset and boundary examples. Confirm schema compatibility or provide the migration decided in AC01. |
| V06-15 | G2, G3, G6 | Run inherited 0.3–0.5 regressions and accepted synthetic preview controls. Retain reproducible seeded-bug, repaired and boundary reports for exclusive regions, typestate including exceptional exits, and both named protocols. Compile/type-check accepted declaration examples and retain pinned runtime conformance results. |
| V06-16 | T07, G4 | Run the retained ownership workload and added state/region/model workloads on optimized builds, with cold, warm, affected and unrelated edits, location-only changes and budget stress. Record time, memory, query counts, reproducibility inputs and an investigation of regressions. Verify deterministic output and source-only analysis. |

## Policy controls

Use default advisory thresholds for the boundary-only cases and explicit selection for new violation rules. The matrix assumes a successful analyzer request unless the last row applies. Retain canonical findings, full coverage and exit reasons even when output details are capped.

| Only result in the report | Advisory | Strict |
| --- | --- | --- |
| Valid within selected scope, complete coverage | 0 | 0 |
| Unsuppressed known violation at the configured failure threshold | 1 | 1 |
| Unsuppressed possible violation below the configured threshold | 0 | 1 |
| Live exclusive region spanning suspension, with only an unsupported boundary | 0 | 1 |
| Unresolved state or effects, with only an incomplete-coverage reason | 0 | 1 |
| Suppressed eligible finding and incomplete coverage | 0 | 1 |
| Invalid contract declaration | 1 | 1 |
| Fatal input or execution failure | 2 | 2 |

## Evidence to collect

Use independent oracles stating the expected primary finding, certainty, related spans, relevant state/effect facts, coverage and exit result. Match fixtures across both declaration forms and retain mixed-interface calls. Compare semantic results independently of display wording and assert locations separately.

Use static analyzer fixtures for target programs. Run only controlled programs for upstream conformance, including injected startup and join failures where qualified, and record the exact CPython pin and environment. Test a marker proving that normal checker analysis did not execute the target. Candidate evidence records each exercised assumption explicitly.

For the proposed synthetic preview, retain an independently specified valid case, failing case, minimal repair and unsupported control for each claimed feature area. Run inherited and new checks against the exact candidate and retain the raw evidence. A repaired result must lose the intended finding through a real semantic change without suppressing evidence or adding a new coverage boundary. Field usefulness remains unqualified by synthetic tests. AC01 records the accepted preview protocol.

Compare optimized measurements with the [0.5 baseline](../0.5/performance.md) on comparable workloads and host conditions. Retain commands, revisions, settings, input sizes, raw timings, peak memory and query events. Keep acquisition, alias, region and task budget controls independent where feasible. Record the maintainer's decisions and exact revision when [release evidence](../../release-evidence-template.md) is assembled.

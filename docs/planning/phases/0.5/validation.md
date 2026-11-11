# 0.5 validation matrix — V05

Status **qualified for the published 0.5.0 release**. Rows specify acceptance oracles and evidence to retain. [Phase plan and gates](../0.5-ownership.md) · [Ownership specification](specification.md)

## Required rows

| ID | Controls and expected result | Evidence |
| --- | --- | --- |
| V05-01 | Compare the default CLI and Rust session to 0.4 on the inherited corpus. Exercise explicit on/off settings, precedence, `--no-config`, strict without opt-in, and inspection. Equivalent annotation and resource spellings produce the same normalized ownership facts. | OW01 decision record, CLI/config, inspector and session fixtures |
| V05-02 | `dst = move(src)` keeps one open instance and one cleanup duty. Use, close, copy or move through the old binding reports reuse. Close through the destination discharges the original duty once. Rebinding `src` to a fresh object permits use. A move on one branch produces a possible reuse at the join. | Flow truth table and exact primary/related spans, counts and certainty |
| V05-03 | Use positional and keyword arguments, including a read or alias capture before or after the move. Raise before the move, in a later argument, and during supported call binding. Check source validity, unclaimed duty and actual exit on every edge. A discarded move leaves a known outstanding duty. | Evaluation-order and signature fixtures with per-edge handle and duty assertions |
| V05-04 | A resolved `Owned` callee accepts the moved argument, consumes it or transfers it onward. A bare argument produces the missing-transfer finding. Exercise normal close, raise before close, raise after close, and uncertain close. A borrowed callee cannot accept an owned transfer as if it had taken the duty. | Paired caller/callee fixtures and summaries with separate completion outcomes |
| V05-05 | An open unique acquisition returned under `Owned` and an explicit owned return through `move` preserve the duty at the caller. Cover ignored return, replaced return through `finally`, and rejected closed, borrowed, aliased or unresolved returns. Keep must-use and cleanup independent. | Return and cleanup oracle plus inherited V03/V04 regressions |
| V05-06 | Plain assignment aliases, alias rebinding drops the visible conflict, and earlier escapes still block a uniqueness claim. Test live local aliases, an active cleanup frame, branch-specific aliases, and multiple instances from a loop. Closing one instance leaves other outstanding duties visible. | Alias/loop truth table and known versus unresolved conflict fixtures |
| V05-07 | A shared view can be read, aliased locally and forwarded to a checked nested borrow. Its region ends on return and raise. Reject close, consume and known return, field/global/container writes and returned-closure capture. Unknown callback or capture yields incomplete coverage. | Positive, negative and boundary fixtures with lexical-region evidence |
| V05-08 | Exercise local, imported, qualified and acyclic re-exported calls. Run both resource interfaces and both access spellings in mixed producer/consumer combinations. Same-spelling impostors and rebinding leave no recognized move. A violating borrowed body supplies no verified call effect. | Contract and canonical diagnostic parity, checked-source provenance and binding fixtures |
| V05-09 | Native calls, unsupported signatures, dynamic dispatch, unresolved imports, hidden aliases and external declarations retain visible boundaries. Only exact applicable reviewed models supply effects. A missing, shadowed or out-of-range model cannot supply uniqueness or discharge. Independent known facts survive an unrelated boundary. | Source-only boundary controls and provenance/selection report |
| V05-10 | Cover direct and mutual recursion with feasible base exits, mixed cleanup/escape outcomes, unresolved recursion and deterministic widening. Hit the exact and first overflowing recursion, alias, flow and capture budgets. Widening retains incomplete coverage and another request can still succeed. | Recursive effect oracles, limit results and bounded-work traces |
| V05-11 | In a qualified TaskGroup, accept owner close after all known borrowers complete and flag close before completion. Cover nested groups, multiple children, unused pending coroutines, out-of-scope await/capture, task escape, request-only cancellation, and normal and exceptional group exits. An inner exit cannot release an outer borrower. | Static task/capture matrix and per-version controlled conformance |
| V05-12 | Compare direct await, task-local acquisition, normal outer-to-worker handoff, rejected submission, cancellation before worker entry, cancellation during borrowed work and awaited cleanup, sibling failure and repeated cancellation. Check validity, completion, lifecycle and cleanup separately. Include inactive-group behavior by version, task-factory selection and reported assumptions, and detached, custom/eager-factory, shield, thread and process boundaries. | Static per-edge oracles and controlled CPython probes with exact version and source pins |
| V05-13 | Change only a move/borrow/escape effect, a task/model completion fact, semantic settings, target, budget, provenance or source locations. Compare fresh and reused results across findings, related spans, certainty, trust and coverage. Show dependent checks refresh and unaffected work is reused. | Session/query-event traces, reverse-input-order comparison and full report diffs |
| V05-14 | Compare text, concise and JSON. Exercise default, advisory and strict policies, capabilities and rule explanations, filtering, suppression and baselines made with a prior semantic scope. Strict exits 1 for relevant unresolved coverage, while invocation/configuration failure exits 2. No policy-only repair erases coverage. | Rule registry, schema/consumer, fingerprint and exit fixtures plus tutorial replay |
| V05-15 | Run the agreed preview protocol below, including independent source-repair oracles and clean replay using installed artifacts. Keep supported violations and unresolved boundaries separately labeled. | Frozen manifest, ordinary type/lint checks and candidate project reports |
| V05-16 | Replay the 0.4 baseline workloads with ownership off and on. Add alias-heavy, recursive, and nested task/cancellation workloads near the proposed limits. Measure cold, warm and single-edit analysis, report cost, memory and query reuse. Compare to the recorded baseline and investigate regressions. | Optimized commands, environment, repeated samples, time/memory and query events |
| V05-17 | Run inherited validation and G1–G7 for the exact candidate. Verify typed identity-helper behavior and import cost on the supported Python matrix, package checks on advertised artifacts, published limitations and migration, and G8 after authorization and tagging. | Release evidence, exact-candidate CI, distribution results and release decision |

## Local evidence

The [Phase 0.5 integration tests](../../../../crates/codepact/tests/phase05.rs)
exercise V05-01 through V05-15. The [Salsa tests](../../../../crates/codepact/src/db.rs)
add dependent refresh and unrelated query reuse for V05-13. The
[controlled TaskGroup probes](../../../../tests/python/test_task_group_model_conformance.py)
and [performance measurements](performance.md) supply CS03 and V05-16
evidence. Record exact-candidate and platform results for V05-17 in the
[release ledger](../../../releases/0.5.0-evidence.md).

## Synthetic preview qualification

The development candidate uses a synthetic ownership preview. Freeze the candidate,
fixture hashes, target versions, opt-in settings, invocations and expected
findings before final scoring. Define the oracle independently from analyzer
output. Reuse the 0.4 fixtures as inherited controls. Include ownership-specific
bug/repair pairs for transfer and reuse, borrow escape, and premature cleanup
inside a task group. Include unresolved escape and scheduling controls. [0.4 synthetic preview protocol](../0.4/synthetic-preview-protocol.md)

Each planted case must match the specified rule, certainty, location and
completion edge where applicable. Each paired repair must clear its expected
findings by changing source under the same analysis settings. Each boundary
must retain its expected coverage reason. Run ordinary lint and type checks on
the paired examples and compare fresh/reused reports. Retain the manifest,
raw reports, tool versions, artifact identity and results. Replay the corpus
with the installed candidate CLI and helpers. Synthetic outcomes qualify the
modeled examples and boundaries. Real-project usefulness needs separate field
evidence.

## Evidence conventions

Record the source or model revision, settings, expected and observed facts,
diagnostic sites and counts, coverage, command, exit status and reviewer.
For CS03, retain controlled upstream conformance separately from static fixture
results. Record the Python targets claimed for source analysis separately
from the interpreter versions tested for the helper distribution and from
the TaskGroup model versions actually qualified. Reuse the
[0.4 operational baseline](../0.4/evidence/qualification-2026-11-10/README.md)
for the performance comparison. [Common gates](../../release-gates.md) · [Candidate evidence template](../../release-evidence-template.md)

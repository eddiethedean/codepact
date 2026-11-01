# 0.3 validation matrix

Status: **Planned; all V03 rows pending.** IDs are independent of V01/V02. The [specification](specification.md) is the proposed oracle; the [phase plan](../0.3-cleanup.md) defines CL01–CL08 and C01–C05/C07/CS01. No fixture path or test result is claimed until implementation maps it below.

## Required matrix

Each row needs concrete valid, invalid and unknown counterparts where applicable, with source spans, outcome/duty state, certainty, provenance, coverage and CLI exits. Fixture models with precise postconditions must be distinguished from declared-unverified illustrative methods.

| ID | Fixture / action | Expected result | Gates |
| --- | --- | --- | --- |
| V03-01 | Local acquisition then close vs fallthrough/explicit return without close; acquisition fails | Success discharges; each distinct unclosed exit is reported; failure creates no returned-object duty | C01 |
| V03-02 | Owned vs Borrowed vs unqualified parameter; decorator/explicit resource and shorthand/verbose access forms | Shared semantics; only Owned inherits duty; Borrowed close fails; local acquisitions always retain their own duty | C01, C03 |
| V03-03 | Finally on fallthrough, early return and raise; nested finally; unreachable cleanup | Required cleanup executes in order; unreachable close cannot discharge; actual exit and pending return retained | C01, C02 |
| V03-04 | Close normally succeeds vs raises with open/closed/unknown exceptional state | Normal postcondition never leaks onto exception edge; known closed discharges, known open remains pending, unknown stays incomplete | C02, C05 |
| V03-05 | Handler order, unmatched exception, try-else, handler/else raises, handler-expression effects | Route all feasible outcomes; else only after normal try; every outgoing path crosses applicable finally | C02, C04 |
| V03-06 | Two returns vs branch paths sharing one exit; finally replaces return/raise; nested cleanup raises | Exact agreed finding counts, completion kinds and primary/related spans; retain earlier effects without duplicate witnesses | C01, C02, C04 |
| V03-07 | Direct Owned factory/return, alias return, return expression raises, close or replacing return in finally | Handoff only on actual successful owning return; caller gets same object duty; closed result violates return contract; abandoned result retains duty | C01 |
| V03-08 | Borrowed-to-Owned return, unannotated return, unknown return aliases, storing returned resource in field/container | No fabricated transfer or uniqueness; contract violation or explicit incomplete boundary according to frozen policy | C01, C05 |
| V03-09 | Resolved owning helper closes/returns/leaks/raises; argument evaluation fails; same object in two parameter positions | Duty delegates only at accepted entry; outcomes map back to same identity; one reporting witness; borrowing never delegates duty | C01, C03 |
| V03-10 | Reviewed context enter success/failure, result alias, nested/multiple managers, suppression and cleanup failure | Exit only for successfully entered managers, reverse unwind, correct continuation/propagation and outcome-specific duties | C02, C03 |
| V03-11 | `contextlib.closing` and `aclosing`, imported alias/re-export and lookalike; wrapper receives Borrowed | Resolve reviewed identity; delegate cleanup effects and trust; wrappers cannot authorize borrowed close | C02, C03 |
| V03-12 | Idempotent/requires-open/unspecified repeat close; allowed/forbidden post-close methods; unknown manager/suppression/finalizer | Follow explicit policies only; unknown contexts/finalizers never discharge; allowed operation does not reopen | C02, C05 |
| V03-13 | Close through exact/uncertain aliases, rebind last alias, unknown helper/callback after escape | One duty per object; uncertain close cannot discharge all targets; lost alias retains duty; opaque effects remain unknown | C01, C05 |
| V03-14 | While/for zero/one/many iterations; conditional close, break/continue inside finally, loop-else | Conservative joins and correct cleanup/target routing; no cleanup from an unexecuted iteration | C04 |
| V03-15 | Acquire-close each iteration vs overwrite without close; close only last object; iterator raises; nonterminating loop | Repeated instances retain multiplicity; closing latest cannot erase older duties; iterator uncertainty visible; no invented exiting path | C04 |
| V03-16 | Loop fixed point vs high-budget finite reference; nested loops, alias/object/duty/exit-partition limits | Termination; widening never proves cleanup; every exhausted dimension reports affected unknown duties and usable recovery | C04, C05 |
| V03-17 | Create coroutine without await; direct await vs local coroutine alias; async close created/awaited; repeat await | Creation applies no body/close effects; captures retained; only supported completed execution applies postcondition; unsupported repeat use visible | CS01 |
| V03-18 | Cancel before body entry, at use await, during finally, and again during awaited close | No locals acquired before entry; caller captures/duties retained; cleanup may be interrupted; unknown cancellation cannot discharge | CS01 |
| V03-19 | Async context entry/exit on normal/raise/cancel; entry fails; close has known closed/open/unknown cancelled outcome | Each execution edge uses its own postcondition; failed entry installs no exit; wrapper supplies no stronger cleanup guarantee | CS01 |
| V03-20 | Pending coroutine escapes scope or captures then close; TaskGroup/create_task/gather/shield/timeout; process/thread handoffs | Visible capture/escape and incomplete coverage; no task completion, unique transfer, cleanup or isolation inferred | C05, CS01 |
| V03-21 | Model valid/unsupported version, missing version input, conflict, shadowed symbol; edit exceptional/repeat-close/suppression policy | Only resolved reviewed version applies; no target import; affected duties/coverage update; provenance and model assumptions retained | C02, C07 |
| V03-22 | Edit callee normal/exceptional/cancelled cleanup, return ownership, coroutine capture or model selection in reused session | Fresh/reused semantic and span parity on all outcomes; affected dependents rerun; unrelated checks reuse by query events | C07, CS01 |
| V03-23 | Move model/close/acquisition/annotation spans; no-op; root/target/budget change; cancel request then recover | Current locations/revision, semantic reuse when applicable; no stale publication; bounded incomplete result recovers | C05, C07, CS01 |
| V03-24 | Source-side-effect marker, reordered files, recursion/import cycle and unsupported generator/except-star/custom awaitable | No marker executed; deterministic bounded boundaries; no erased duties or frontend-specific semantics | C07, CS01 |
| V03-25 | Rust one-shot/session, text CLI and JSON; definite/possible/unknown/invalid input; inherited inspector/helper/checker fixtures | Equivalent findings/trust/coverage and exact deduplication; exits 1/0/0/2; versioned check schema and preserved inspection contract | C07, CS01 |
| V03-26 | Pending coroutine/capture and async unwind budgets; unknown escaped alias across suspension | Exhaustion/unknown actor weakens affected duties and lifecycle, never grants cleanup; bounded termination/recovery | C05, CS01 |

## Exit-path oracle

Cross every supported cleanup construct with each feasible outcome below. Mark an infeasible combination with its model reason; do not silently omit it. In fixtures claiming complete coverage, use reviewed effects for every potentially raising operation, including acquisition, use and cleanup.

| Construct | Required outcomes and assertions |
| --- | --- |
| No cleanup / explicit close | Fallthrough, early return, raise before/after close; acquisition failure and lost alias |
| Try/except/else/finally | Normal, matched/unmatched raise, return, replacement return/raise, handler/else failure and nested cleanup |
| While/for | Zero and repeated iterations, break, continue, exhaustion/else, evaluation/iterator failure and outer cleanup |
| Reviewed with | Entry failure, body normal/return/raise/break/continue, suppression, failed exit and multiple-manager ordering |
| Await/finally/async with | Creation without execution, pre-entry cancellation, normal/raise/cancelled suspension, entry failure, interrupted/repeated cancellation during exit |
| Owned handoff | Evaluation failure, accepted entry, normal owning return, replacement exit, closed returned value and unknown escape |

## Assertions and automation plan

Add frontend/CFG snapshots for each completion reason, loop backedge and cleanup stack. Assert lifecycle, duty presence/status/holder, capture identity, result aliases and provenance independently of golden text. Cross-interface comparisons remove source-only origins but retain effects, trust, certainty and coverage; locations have separate snapshots.

Compare fresh sessions with a reused session after each edit. Check facts, duties, findings, primary/related spans, certainty, model provenance, coverage and revision attribution. Assert query execution/reuse counts separately from timings. Include no-op, unrelated edit, imported exceptional effect, async/cancellation effect, model-content/version change and location-only change. Compare object/alias/duty relationships across sessions, not database-local IDs.

Use bounded generated branch/loop cases to check monotonicity: replacing a known close with an unknown effect cannot improve cleanup certainty; widening or dropping aliases/captures cannot discharge a duty. Compare against independently enumerated finite exit paths on small bounded fixtures. Raising the budget is useful differential evidence, not an independent correctness oracle by itself.

Keep upstream model-conformance runtime tests isolated from static analysis fixtures. Execute only controlled test code against exact qualified interpreter/library versions; never import or execute a user's target project or model hooks. Retain model source revision/hash, supported versions, reviewed outcomes and platform assumptions. A model test on one interpreter does not establish the whole helper-runtime matrix.

## Qualification record

For each V03 row record the implemented fixture/test path, revision, invocation, expected/actual outputs, log/snapshot link, accountable maintainer and Pending/Pass/Fail. The maintainer may run, validate and review every row alone; no separate validator or reviewer is required. No V03 row is currently automated by this plan. Link failures to owned issues and rerun affected evidence after code/model/settings changes. Inherited V02 tests must be retained or explicitly updated with a reviewed explanation where 0.3 replaces a former unsupported boundary.

Measure fixed small/medium cleanup corpora, nested exceptions/loops, multiple managers and direct-await cancellation cases. Record cold/warm time, effect/model/location edit time, peak and retained memory, query counts, selected numerical budgets, corpus hashes and toolchain/host. Compare unchanged lifecycle/inspection cases against the [0.2 baseline](../0.2/performance.md), investigate regressions and record accepted limits in a new phase performance record during qualification. A completed budget table is not an absolute latency guarantee.

Run inherited `make check` and installed-artifact checks on the exact candidate; extend quality CI with cleanup/async/model regressions. Verify text/JSON and installed tutorials, matching distributions and supported platform builds under D015. Local checks alone do not qualify other targets. Complete C01–C05, C07, CS01 and G1–G7 before tagging; record release authorization separately and G8 after authorized publication using the [common evidence template](../../release-evidence-template.md).

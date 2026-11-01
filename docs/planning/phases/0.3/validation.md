# 0.3 validation matrix

Status: **Automated implementation coverage is mapped below; release qualification is pending.** IDs are independent of V01/V02. The [specification](specification.md) is the candidate oracle; the [phase plan](../0.3-cleanup.md) defines CL01–CL08 and C01–C05/C07/CS01. Test presence is implementation evidence only: a mapped row is not a passed qualification gate.

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

## Current automated coverage map

The paths below identify exercised candidate behavior. Coverage is partial for
every row: the required matrix above still calls for additional cases, outcome
and span assertions, fixture-model provenance, differential/session checks or
runtime conformance evidence where applicable. None of these entries is a
qualification Pass.

| ID | Current automated paths | Remaining qualification evidence |
| --- | --- | --- |
| V03-01 | `tests/phase03.rs`: local acquisition and failed initialization | Distinct exits, failing acquisition variants, spans and exact finding grouping |
| V03-02 | `tests/phase03.rs`: lost alias and unqualified parameter; `tests/cli.rs`: Borrowed-close diagnostics | Owned/Borrowed forms, aliases, shorthand/verbose declarations and local-duty independence |
| V03-03 | `tests/phase03.rs`: nested finally cleanup through a pending raise, alias rebinding, replacement return/raise and unreachable cleanup | Ordered exit snapshots and exact primary/related spans for every replacement |
| V03-04 | `tests/phase03.rs`: raising source close preserves open duty; `src/flow.rs`: fixture model outcomes preserve open, establish closed or retain unknown lifecycle state; `src/db.rs`: session-level closed/open/unknown exceptional outcomes assert findings, coverage and selected model provenance | Full outcome/duty/span snapshots and maintainer qualification review |
| V03-05 | `tests/phase03.rs`: exact and shadowed built-in constructor routing, matched handler, else success, handled raise skipping else, and handler-type expression effects | Handler order, unmatched custom classes, handler/else expression failure paths, and outgoing finally snapshots |
| V03-06 | `tests/phase03.rs`: separate findings for distinct return and raise exits, return replacement and closed Owned return; CLI text/JSON parity | Exact related spans, nested cleanup raises, and deduplication across all completion kinds |
| V03-07 | `tests/phase03.rs`: direct, aliased, forwarded, finally-cleaned, closed Owned returns, and return-expression failure | Remaining replacement/abandonment combinations and exact handoff snapshots |
| V03-08 | `tests/phase03.rs`: Borrowed-to-Owned return rejection | Unannotated and unknown returns, fields/containers, and frozen escape-boundary policy |
| V03-09 | `tests/phase03.rs`: Owned helper transfer/cleanup/return/raise with one callee witness, Borrowed helper non-delegation, argument-evaluation failure before entry, repeated-object argument positions, and leak deduplication | Full close/return/raise outcome map and broader Borrowed helper cases |
| V03-10 | `tests/phase03.rs`: multiple closers, entry-result aliasing, failed later manager argument/entry unwind, checked close raise, body/close exception replacement, and unknown suppression | Reviewed entry/exit models and normal/raise/cancelled exit postconditions |
| V03-11 | `tests/phase03.rs`: `from` alias, module-qualified import, entered-resource alias, body-exception routing, supplied-module re-export, lookalike and Borrowed rejection; [runtime conformance record](contextlib-conformance.md): six sync/async cases and exact source hashes on CPython 3.11.14–3.13.11 (plus 3.14.3 control) | Model-content invalidation, broader manager cases, exact candidate qualification and remaining V03-11 assertions |
| V03-12 | `src/flow.rs`: fixture summaries for idempotent/requires-open repeat close and post-close allowlists; `tests/cli.rs`: default unknown repeat-close and forbidden post-close operation boundaries | Full valid/invalid/unknown matrix and accepted model behavior |
| V03-13 | `tests/phase03.rs`: lost-last-alias duty, uncertain receiver close keeps separate possible findings per acquisition, and callback capture escape retains an unknown duty | Additional callback escape combinations and alias/object budget counterexamples |
| V03-14 | `tests/phase03.rs`: nested while zero-iteration, tuple value capture, unresolved-name boundary, static tuple for/else/break, and finally cleanup through break/continue | Conditional cleanup and full target-routing assertions |
| V03-15 | `tests/phase03.rs`: repeated allocation and closing the latest instance retains older possible duties; unresolved iterator effect does not prove cleanup; a nonterminating loop is not an exit witness | Broader iterator effects and additional termination counterexamples |
| V03-16 | `tests/phase03.rs`: static tuple iteration cap/early break, unknown-effect monotonicity; flow budget unit tests | Nested/dimension budgets, usable recovery, generated monotonicity cases, and measured high-budget comparisons |
| V03-17 | `tests/phase03.rs`: unawaited/awaited async close, direct-coroutine aliasing, repeat-await boundary, cancellation, and coroutine budget | Execution/completion snapshots, coroutine identity across ambiguous joins, and remaining awaitable boundaries |
| V03-18 | `tests/phase03.rs`: coroutine creation does not transfer Owned duty; cancellation before coroutine entry keeps only caller duties; cancellation during awaited use enters `finally`; nested awaited cleanup remains interruptible; `tests/python/test_contextlib_model_conformance.py`: cancellation enters `aclose`, and repeated cancellation can interrupt it | Full caller capture/duty snapshots and maintainer qualification review |
| V03-19 | `tests/phase03.rs`: async context cancellation and failed later entry unwinding an earlier manager; `tests/python/test_contextlib_model_conformance.py`: `aclosing` on body cancellation and interrupted cleanup; `src/flow.rs`: fixture async-close cancellation postconditions | Nested awaited cleanup, full context exit snapshots and model qualification |
| V03-20 | `tests/phase03.rs`: scheduler handoff boundaries for `create_task`, `gather`, `shield`, `wait_for`, TaskGroup, and thread/process executor submissions | Broader escaped-capture combinations |
| V03-21 | `tests/phase03.rs`: unsupported target version; `tests/cli.rs`: selected contextlib model revisions in JSON; `src/model.rs`: exact-version selection and conflict rejection; `src/db.rs`: supported-to-unsupported target and model edit updates in a reused session | Missing-version behavior and externally accountable provenance review |
| V03-22 | `tests/phase03.rs`: changed-body fresh-session parity; `src/db.rs`: target/model-selection and resource-model edits match fresh results while reusing unrelated flow queries; existing callee/import invalidation tests | Model/callee edits across all outcomes, moved related spans and broader query-event snapshots |
| V03-23 | `src/db.rs`: target-version mutation, cancellation and recovery tests | Full V03 edit matrix, root/budget changes, and related-location snapshots |
| V03-24 | `tests/phase03.rs`: custom awaitable execution boundary; existing CLI tests cover no target execution, unsupported syntax/import behavior and recursion | Generator, except-star, and broader deterministic-boundary matrix |
| V03-25 | `tests/cli.rs`: text/JSON leak parity and module-qualified cleanup/model metadata parity plus existing CLI/schema/exit tests | Cross-interface semantic/trust/coverage parity across invalid, possible and unknown findings; inherited helper/inspector contracts |
| V03-26 | `tests/phase03.rs`: coroutine-record cap weakens captured state; closing a captured resource does not complete its pending coroutine | Async-unwind/capture budgets, unknown actor across suspension, and bounded recovery matrix |

## Assertions and automation plan

Add frontend/CFG snapshots for each completion reason, loop backedge and cleanup stack. Assert lifecycle, duty presence/status/holder, capture identity, result aliases and provenance independently of golden text. Cross-interface comparisons remove source-only origins but retain effects, trust, certainty and coverage; locations have separate snapshots.

Compare fresh sessions with a reused session after each edit. Check facts, duties, findings, primary/related spans, certainty, model provenance, coverage and revision attribution. Assert query execution/reuse counts separately from timings. Include no-op, unrelated edit, imported exceptional effect, async/cancellation effect, model-content/version change and location-only change. Compare object/alias/duty relationships across sessions, not database-local IDs.

Use bounded generated branch/loop cases to check monotonicity: replacing a known close with an unknown effect cannot improve cleanup certainty; widening or dropping aliases/captures cannot discharge a duty. Compare against independently enumerated finite exit paths on small bounded fixtures. Raising the budget is useful differential evidence, not an independent correctness oracle by itself.

Keep upstream model-conformance runtime tests isolated from static analysis fixtures. Execute only controlled test code against exact qualified interpreter/library versions; never import or execute a user's target project or model hooks. Retain model source revision/hash, supported versions, reviewed outcomes and platform assumptions. A model test on one interpreter does not establish the whole helper-runtime matrix.

## Qualification record

For each V03 row record the implemented fixture/test path, revision, invocation, expected/actual outputs, log/snapshot link, accountable maintainer and Pending/Pass/Fail. The maintainer may run, validate and review every row alone; no separate validator or reviewer is required. The coverage map above links current automated tests but does not mark any row qualified. Link failures to owned issues and rerun affected evidence after code/model/settings changes. Inherited V02 tests must be retained or explicitly updated with a reviewed explanation where 0.3 replaces a former unsupported boundary.

## Candidate-local checks

The phase 0.3 implementation/test candidate [`980f6b3b1394c33d6a0810e4f1b082e8bb011233`](https://github.com/eddiethedean/typepact/commit/980f6b3b1394c33d6a0810e4f1b082e8bb011233) passed every job in the [Quality workflow](https://github.com/eddiethedean/typepact/actions/runs/41284098590), including docs, Rust quality/dependency audit, and Python 3.11–3.14. The 79-test phase suite passed in that CI run. The exact candidate also passed `uv run --locked make check` locally on CPython 3.14.3, macOS 26.5.2 ARM64, on 2026-11-01; the full check covers formatting, Rust checks/tests, Python tests and typing/linting, package wheel/sdist builds and install smoke checks, dependency and MSRV checks, inspector behavior, and documentation links/build. The Python-specific checks also passed locally with `uv sync --locked --python X.Y` followed by `uv run --locked --python X.Y make check-python check-python-package` on CPython 3.12.13, 3.13.16 and 3.14.3. The two candidate tutorial fixtures were run with the source CLI and matched the documented definite synchronous leak and cancellation-possible async findings. Separate six-case runtime model results and source hashes are in the [contextlib conformance record](contextlib-conformance.md). The candidate worktree includes draft 0.3 release notes and upgrade guidance; the full local check also passed the relative-link checker and warnings-as-errors Sphinx build. Remaining phase-specific qualification evidence and maintainer semantic acceptance are still pending.

The [current 0.3 candidate performance record](performance.md) measures cleanup corpora, nested exceptions/loops, multiple managers and direct-await cancellation cases, plus lifecycle/inspection comparisons, query reuse, memory and budgets. It records a regression against the [0.2 baseline](../0.2/performance.md); the maintainer's acceptance and any additional qualification measurements remain pending. A completed budget table is not an absolute latency guarantee.

The runnable [candidate cleanup tutorial](cleanup-tutorial.md) demonstrates synchronous exit obligations and async cancellation findings with separate fixtures and expected CLI outcomes.

Run inherited `make check` and installed-artifact checks on the exact candidate; extend quality CI with cleanup/async/model regressions. Verify text/JSON and installed tutorials, matching distributions and supported platform builds under D015. Local checks alone do not qualify other targets. Complete C01–C05, C07, CS01 and G1–G7 before tagging; record release authorization separately and G8 after authorized publication using the [common evidence template](../../release-evidence-template.md).

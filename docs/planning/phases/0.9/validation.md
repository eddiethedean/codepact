# 0.9 validation matrix

Use this matrix to reproduce validation for CodePact 0.9.0. The
[phase plan](../0.9-obligations-and-change-review.md) assigns release gates.
The [specification](specification.md) defines the profiles. Compare inherited behavior with published
[CodePact 0.8.0](../../../releases/0.8.0-evidence.md).

## Required cases

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V09-01 | OH01 | Use exact, aliased and installed CodePact declarations. An unmarked `Ok`/`Err`/`Some`/`Nothing` and same-named foreign types create no duty. Existing `@must_use` assignment and immediate `discard` preserve released behavior, with an independent pending duty when both contracts apply. Unsupported parameter, local, field, nested and resource-payload declarations retain boundaries. Marked successful return creates one outward duty. Raising before delivery creates none. Pending returns forward once and acknowledged, discarded or unmarked returns renew. |
| V09-02 | OH02 | Exercise temporaries, underscore bindings, aliases, equal independent productions, conditional presence and final-carrier loss on overwrite, deletion or exit. A failed RHS preserves the prior binding. Unknown storage reports incomplete evidence, avoids a false definite loss and preserves later supported effects on known aliases. |
| V09-03 | OH03 | Compare exact and OR arms, `pass` bodies, singleton and multi-variant wildcards, partial matches and acknowledgment across multiple matches. A false or raising guard leaves its arm unentered while preserving established guard effects. Check per-variant state on each exit and retain the separate finite-match diagnostics. |
| V09-04 | OH04 | A source-resolved helper applies only its established per-exit effects to the same identity. Cover argument failure before entry, alias return, plain-return escape of a newly acquired outcome duty and marked forwarding. An acknowledgment in `finally` applies on the actual exit. Replacement return or exception changes the outward handoff. |
| V09-05 | OH05, OH06 | Accept a nonempty literal discard reason and a single value evaluation. Report missing, blank and dynamic reasons without clearing the duty. In paired fixtures, acknowledge the outcome, discard it with a reason, or suppress its finding while a resource cleanup duty and a work observation duty are pending. Verify that both independent duties stay pending and suppression alters only the finding disposition. Direct await delivers one duty on success. Coroutine creation, failed await, pre-start cancellation and interrupted observation leave delivery or acknowledgment unestablished on the affected paths. |
| V09-06 | OH07 | Use the same caller with selected installed outcome contracts and with removal, shadowing or unavailable effects. Record the contract identity and lost applicability. With a stable provider, acknowledge a real pending duty before exit and review the repair. Pair it with an unchanged caller and a removed duty, a newly added waiver, and a repair attempt that passes the outcome to an unresolved helper. In the last case, retain the duty and report incomplete helper coverage and an `inconclusive` review when there is no other conclusive failure. |
| V09-07 | AS03 | Exercise task and future aliases, successful value retrieval, raised worker exception, cancelled or timed-out wrappers, and stored and discarded unobserved work. Exercise `wait` returning pending work and async `as_completed` yielding a member before retrieval. Repeated retrieval of a marked result preserves the same outcome duty. Run the [gather controls](#gather-observation-controls). |
| V09-08 | MP03 | Exercise explicit-spawn admission, delivery failure, `successful()` before readiness, timed and untimed `wait`, `ready`, successful `get`, worker exception and alias observation. Pair stored/discarded handles with retrieved results. Pool shutdown or worker completion alone leaves the relevant result unobserved. Unqualified serialization retains a boundary. |
| V09-09 | BR01 | Pair pre-admission rejection with a submission failure whose admission is uncertain. Cover alias retrieval, a timed-out result, rejected cancellation, the `running()` dispatch boundary, worker exceptions and successful `exception()` leaving the value unobserved. Executor context exit leaves the stored or discarded result duty pending. Retrieving a marked result exposes its independent outcome duty. |
| V09-10 | LC05, CR08, E04 | Run HTTPX and HTTPX2 separately and together. For each, cover direct and aliased client use, buffered body use after client cleanup, lexical stream closure, attempt to read an unread closed stream, client use after close and failed cleanup. Keep a fixed caller while changing the selected package, shadowing its import, or selecting an unsupported transport. Check the provider and applicability ledger even when no diagnostic is present. |
| V09-11 | LC07, AS07, E04 | On each actual HTTP provider, use barriers to establish two overlapping buffered requests on one open client and verify completion before `aclose`. Pair with close during active use, cancellation requested while a request remains active, interrupted wait, and conditional/unknown alias and operation cases. Exercise async entry, exit and exceptional request/cleanup. An unsupported-only case earns boundary evidence. |
| V09-12 | LC09, E04 | Use a temporary SQLite fixture for scoped connections/sessions, commit, rollback, explicit connection release, session reuse after reset-on-close, result consumption and engine disposal. Verify database state independently of settlement and cleanup calls. Cover omitted cleanup, early ownership escape, body exception, and an unreviewed driver or session option. |
| V09-13 | LC10, E04 | Probe SQLModel scalar and row `exec` results and exact adapter selection. Probe installed ETLantic `session_scope` normal commit, body `Exception` rollback, a non-`Exception` exit, failed commit, failed rollback and failed close. Observe attempt order and established effects independently. Use the external summary in static analysis and exercise a stale or absent summary and `request_scoped_session` as boundaries. |
| V09-14 | LC12, AS07, E04 | Run separate SQLAlchemy and SQLModel async matrices on the pinned driver. Run the [session cancellation controls](#sql-session-cancellation-controls) and distinguish synchronous consumption of buffered `execute`/`exec` results from the async streaming result boundary. Accept two task-local sessions. Diagnose model-established overlap through aliases of one session and close during known live use, including when both tasks would otherwise complete before cleanup. Retain unknown task and operation boundaries and rerun generic `BorrowedMut` controls. |
| V09-15 | E12 | Exercise a missing declaration and its repair for each selected declaration family. Show independent preview and enforcement, exact policy scope and compatible baseline matching. Show strict incomplete-coverage failure with a matching baseline. Change the policy or applicable contract and check report and baseline invalidation. Stale or ambiguous contracts retain incomplete coverage. Malformed or conflicting configuration exits `2`. |
| V09-16 | CR01, CR03, CR04 | Review a new leak or duty, a supported repair, a clean line shift and a verified relocation. Establish correspondence to a clean after site and preserve an inherited violation that still fails standalone `check`. Compare ambiguous duplicates, ambiguous repair targets and deleted operations. Report removal separately from repair. |
| V09-17 | CR02, CR04, CR07, CR08, CR09 | Pair a repair with suppression, policy relaxation, removed applicability, excluded scope, a dependency change on unchanged source and a target migration with no common qualified semantics. Track callers and unresolved siblings. Exercise definite-to-unknown and exhausted budgets. With complete comparable evidence and no newly disallowed findings, change only a relevant effective policy and require `review-required` with that policy change as a reason. Require `failed` for a proven regression with unrelated uncertainty and `inconclusive` for incomplete required evidence plus a known agreement change. Retain every blocking reason. |
| V09-18 | CR05 | Run installed text and JSON review from completed before and after reports, including a before `check` that exits `1`. Assert `passed` with exit `0` and each of `failed`, `review-required` and `inconclusive` with exit `1` in both formats. Require exit `2` for invalid input, unsupported schemas and execution failure. Cover valid truncated evidence, stale after source/config/provider inputs and a valid historical comparison. The trusted-CI recipe derives the reference, policy and checker from the host workflow. A self-asserted trusted input supplies no authority. |
| V09-19 | CR06, CR08, CR09 | Compare normalized CLI and Rust facts, deltas, reasons and verdicts on a full selected root. Reuse one Rust session across line shifts, local fixes, provider/lock changes, model edits, scope changes, budget exhaustion and recovery. Compare fresh results at each revision, including source-identical dependency changes. |
| V09-20 | E10-B | Build and verify actual wheel pairs. Run installed author comparison on stable public re-exports and compatible changes, new or stronger caller duties, weakened guarantees and valid but insufficient evidence. Analyze independent downstream callers against the paired wheels. Use malformed and stale metadata, unsupported capabilities and missing applicability as distinct controls. Assert deterministic human and machine output. |
| V09-21 | E10-B, E10-A | Compare installed CLI and Rust contract deltas for the same verified artifacts. Replace or reorder artifact inputs in a reused session and compare with fresh results. Recheck affected author export, built-wheel verification, consumer selection and source-only controls from E10-A. |
| V09-22 | E04, E05, G2–G3, G5–G6 | Capture the exact package, source-target, interpreter and upstream artifact matrix. Test helper-only, exact CLI extra, standalone CLI, all required native wheels, both sdists and Cargo as applicable. Replay outcome, policy, review and author examples from built wheels. Verify the analyzer reads target and provider source statically with execution sentinels. Review upgrade and limitation documentation. |
| V09-23 | G4 | Compare inherited workloads against 0.8.0 on the same host. Add selected workloads for alias/variant tracking, work membership, installed provider changes and before-and-after review. Record cold and unchanged-session latency, affected and unrelated edits, comparison cost and peak RSS. Exercise bounded work, interrupted analysis and recovery with deterministic report output. Record the maintainer's assessment against the measured baseline. |

## Gather observation controls

- With default `gather`, propagate one worker failure to the caller, then let a
  sibling fail. Verify the first exception is observed and the sibling stays
  unobserved despite internal retrieval. Pair this with a caller retrieving the
  sibling's exception through its original handle.
- Directly unpack a successful fixed-member `gather` into local names. With
  `return_exceptions=True`, unpack a marked outcome and a worker exception.
  Assert which work duties were observed and that the outcome stays pending
  until explicit acknowledgment. Retrieve it through the original handle
  before and after acknowledgment. Verify the identity stays the same and the
  final retrieval creates no new duty.
- Pass the returned list to an unresolved carrier. Require incomplete coverage
  and retention of the known pending outcome without a definite last-carrier
  loss claim.

## SQL session cancellation controls

- Verify direct `await session.close()` on successful and cancelled paths. Pause
  the direct close at an await before connection check-in and interrupt it.
  Require pending or unknown cleanup when release is unconfirmed. Check
  connection check-in independently of the caller's cancellation exception.
- Pause cleanup on `async with` exit at the same point. Cancel the caller while
  the close task is waiting. Confirm the provider received cancellation while
  the close task was still pending, then release the task. Require successful
  connection check-in before `CancelledError` propagates. Pair this with a
  close failure before check-in and require that cleanup remains unestablished.
  In both cases track transaction settlement separately from cleanup.
- In both provider matrices, cancel a supported in-flight operation and verify
  that its lease ends only when the modeled operation has actually ended.

## Evidence capture

Use source-only fixtures to check exact finding IDs and certainty, obligation
identity, coverage, model use and verdict facts. Pair supported runtime effects
with controlled probes of the pinned interpreter or actual installed provider.
Use barriers for overlap, cancellation and timeouts where ordering matters.
Retain each valid, invalid and unknown control with the candidate commit,
fixture or probe ID, observed effect, runtime and provider identities, and
applicable CLI/Rust and fresh/reused results. Apply the
[common release gates](../../release-gates.md) and the
[release evidence template](../../release-evidence-template.md).

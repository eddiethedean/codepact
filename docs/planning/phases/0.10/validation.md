# 0.10 validation matrix

Use this matrix to qualify the proposed [0.10 models](specification.md) for
the [phase plan](../0.10-queues-ipc-and-synchronization.md). Compare inherited
behavior with published [CodePact 0.9.0](../../../releases/0.9.0-evidence.md).

## Required cases

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V10-01 | AS04, AS05, MP04, MP05, E04 | Select the pinned runtime and exact constructors through direct references and aliases. Keep the caller fixed while changing the target, interpreter, platform, start method, launch mode, entry file or model, and exercise a shadowed constructor. Check model identity, applicability and boundary location. Cover `Queue.shutdown` on source targets 3.11 and 3.12 and on the selected 3.13 profile. Verify that checker host capabilities have no effect on selection. |
| V10-02 | AS04 | Pair a balanced queue with missing, early, duplicate and wrong-queue `task_done`. Include equal payloads, aliases of the same queue, and a wrong-queue call that happens to succeed because the other queue has outstanding items. Exercise `QueueFull`, `QueueEmpty`, and ambiguous pairing after overlapping receives. Verify admissions, receipts and the raw counter effect independently of the attributed accounting duty. Run the [queue join controls](#queue-join-controls). |
| V10-03 | AS04 | Cancel a blocked put, blocked get and blocked join, then inspect queue state and surviving duties. Cancel after a successful receive during work. Pair a `finally` acknowledgment with a missing cleanup branch. Distinguish rejection or interruption before admission from cancellation after admission, and account for the actual exit after argument evaluation. |
| V10-04 | AS04 | Compare graceful drain with immediate shutdown. For both, exercise blocked putters, empty getters and new puts. For immediate shutdown, run one fixture with only buffered items and another with an item already checked out as well as buffered items. Confirm exactly which entries the shutdown bypasses and when `join` can return. Keep the worker and its result pending independently. |
| V10-05 | AS05 | Exercise a lock alias, lexical exit, explicit acquire/release, early return, exception, and cancellation before acquire returns and during a body await. Verify release on the actual exit and a pending duty when cleanup is missing. Pair unlocked release with an unknown release implementation. With an exact reviewed resource-to-lock binding, call the same resolved resource method under the required lock, under a different lock and with neither held. Require clean coverage for the first and a guard violation for the other two. Repeat through known aliases and an unresolved binding. Keep a valid await inside the lock and rerun the separate `BorrowedMut` suspension control. |
| V10-06 | AS04, AS05 | Compare ordinary and bounded semaphore release above the initial count and capture their actual runtime behavior along with the unmatched-release finding. Run the [semaphore cancellation controls](#semaphore-cancellation-controls). Pair a fixed queue-worker set with task creation outside the semaphore's permit scope. Verify that the permit or a positive queue capacity supplies no evidence about the number of already-created tasks. |
| V10-07 | AS05 | Pair `Condition.wait` and notification under the exact lock with operations using a different lock or outside the protected scope. Observe release while waiting and reacquisition before normal or cancelled exit. Use a notification without an established predicate and an unknown callback or lock alias. For `Event`, test set, clear, cancelled wait, and a set/clear sequence that wakes an existing waiter. Record the signal and verify that pending completion and result duties survive it. |
| V10-08 | MP05 | Qualify a declared direct script with a worker defined in the entry file and one imported from a resolved module. Pair with import-time startup, a local target and a lambda. Exercise matching and missing launch evidence, a resolved helper from the main guard and an unqualified caller of that helper. Run the [spawn launch controls](#spawn-launch-controls) and cover custom reducers, unknown imports and arguments, captured live resources, and incompatible process/primitive contexts. |
| V10-09 | MP04 | Use a Queue producer and a gated consumer. Confirm `put` admission, close and local feeder completion independently of the consumer's receipt. Pair with feeder serialization failure after `put` returns, broken feeder delivery, close before put/get, join-thread before close, and `cancel_join_thread`. Include a copy that never starts a feeder. Use a source-only control for an order that could block flushing because the reader has yet to drain. |
| V10-10 | MP04 | Qualify the same transport controls for JoinableQueue, then pair balanced accounting with abandoned, duplicate and wrong-queue acknowledgments. Reproduce feeder rejection leaving a pending unfinished count. Hold the child alive after acknowledgments and verify accounting join can return while the child and any result remain outstanding. Run the [queue join controls](#queue-join-controls). |
| V10-11 | MP04 | Compare SimpleQueue success and caller-side serialization failure with the buffered Queue case. Exercise successful write before worker receipt, read/deserialize failure, local close and use after close. Calls to feeder or item accounting methods supply no corresponding effects. |
| V10-12 | MP04 | Exercise duplex round trip and unidirectional send/receive with exact endpoint aliases. Test wrong-direction use, a closed local copy, serialization failure and a broken write. Keep a writer copy open in the parent to withhold EOF after the child's copy closes, then close the last writer and verify `poll(0)` can report readiness before `recv` raises EOF. Keep local close, peer receipt and EOF distinct in the static evidence. |
| V10-13 | MP05 | Pair lexical and explicit process lock handling with a failed nonblocking acquire, unconditional release on the false branch, and interrupted or unknown effects. Acquire the first of two guards, force a known failure to acquire the second, and verify that lexical or `finally` cleanup releases only the first. Pair with a successful two-guard scope and an unknown second acquisition that still preserves first-guard cleanup. Check recursive acquisition depth and wrong-actor release for RLock. Compare Semaphore release growth with BoundedSemaphore rejection on the pinned platform. |
| V10-14 | MP05 | Pair `Value.value` and `Array[index]` compound updates under the exact wrapper RLock with individually synchronized access, a different held lock, and a known guard missing on one path. Exercise an aliased `get_lock()` result and an explicit same-context RLock. Test `lock=False`, a nonrecursive wrapper, and an unknown index as boundaries. Coordinate unguarded reads and writes to demonstrate a lost update without relying on scheduler timing. |
| V10-15 | PC07, AS04, AS05, MP04, MP05 | Compose a bounded TaskGroup producer/consumer and a separate spawned parent/worker example. Compare TaskGroup exit with retrieval through the task handle, and Process join with receipt of an explicit reply on a tracked connection. Track input receipt, `task_done`, work completion, result observation and worker-local resource cleanup as independent facts. Compare equal payloads and distinct reconstructed sender/receiver identities. Repeat with a missing receipt, outstanding result, escaped object and unresolved callback. Keep proven earlier effects visible when later effects become unknown. |
| V10-16 | AS04, AS05, MP04, MP05, G2, G4 | Review a missing acknowledgment or guard repair with stable correspondence. Run the [clean guard review control](#clean-guard-review-control). Also compare removed model applicability, a newly unknown queue alias and ambiguous operation matches. Verify contract and coverage reasons and verdicts using the published review rules. Compare normalized installed CLI and fresh/reused Rust facts across source, model and settings edits, cancellation, budget exhaustion and recovery. Confirm affected query invalidation and reuse on an unchanged or unrelated edit. |
| V10-17 | E04, E05, G2–G3, G5–G6 | Record exact candidate, runtime build and upstream artifact identities. Qualify supported installs and distributions against the inherited matrix. Replay representative valid, invalid and boundary examples with the installed CLI, and retain analyzer-only execution sentinels for user sources and worker imports. Check diagnostic and limitation documentation against qualified operations. |
| V10-18 | G4 | Compare inherited 0.9.0 workloads and new queue/IPC/guard workloads on the same host. Record cold and unchanged-session time, affected and unrelated edits, model/profile invalidation, review capture and comparison cost, and peak RSS. Exercise the actor, primitive, live-message and existing flow limits independently and verify deterministic boundaries, retained known facts and recovery after an edit. |

## Queue join controls

For asyncio Queue and JoinableQueue, start with a blocked join. Acknowledge
the last outstanding item to release the waiter, then admit a new item before
the waiter resumes. Establish that join returns and the new item is still
pending. The static expectation preserves the new item's accounting duty and
attributes the join effect to the earlier admissions. Pair with a join that
observes zero on entry and with a case where the ordering is unproved.

## Semaphore cancellation controls

Test `Semaphore` and `BoundedSemaphore`. Cancel a blocked waiter before any
permit is reserved, while the holder retains the last permit. Verify that the
holder still owns it and the cancelled caller has no release duty. Then
release a permit to the first of two blocked waiters and cancel that waiter
before acquisition returns. Verify that the second waiter can acquire that
same permit and that the cancelled waiter has no acquisition or release duty.

## Spawn launch controls

Run equivalent guarded top-level targets from a direct script and from
`package/__main__.py` via `python -m package`. Observe successful worker entry
in the direct launch. For the package launch, assert that `start()` returns,
the child fails to reconstruct the target, and a worker-body sentinel stays
unset. The static package-launch case records a boundary for worker effects.
Pair the observed child exit with pre-launch rejection of a local target or
lambda under the direct script profile.

## Clean guard review control

Keep the source anchor for a compound `Value.value` update fixed. Change which
of two known recursive locks the wrapper uses, and update the enclosing scope
to acquire the new exact guard. Require complete coverage and zero findings
on both sides. Require `review-required` with the before and after guard
identities at the matched site. A paired edit that only adds the missing
acquisition for the existing required guard should qualify as a repair under
the same contract when the other preservation requirements hold.

## Evidence capture

Use static fixtures to assert finding IDs, certainty, source locations,
obligation correspondence, model applicability and coverage. A valid control
requires complete coverage for its supported subset. On an unknown path,
record the operation whose effect is unproved and the independently established
effects that survive. Derive expected protocol outcomes from the specification
and runtime probes so fresh/reused parity also has an independent expectation.

Register pinned runtime probes in the [conformance registry](../../../../tests/conformance/runtime-models.json).
Use events or control channels to establish timing-sensitive states before
triggering cancellation, feeder failure or EOF. Bound blocking operations and
clean up workers on failure. For unsafe-to-execute negative sequences, use a
static fixture and probe the underlying primitive effect in isolation. Retain
the model identity, upstream source hashes, runtime build and per-probe result
with the exact candidate. [Conformance runner](../../../../scripts/check_model_conformance.py)

Use paired baseline and candidate runs for inherited performance workloads
and record absolute costs for new ones. Retain raw samples, workload sizes,
toolchain and hardware for replay. Record the maintainer's assessment under
G4. [Phase 0.10 performance capture](implementation-evidence.md#performance-replay)

Summarize the fixture and probe IDs, evidence paths and gate decisions in the
[release evidence record](../../release-evidence-template.md).

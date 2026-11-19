# 0.10 specification

This is the model scope qualified for CodePact 0.10.0. The
[phase plan](../0.10-queues-ipc-and-synchronization.md) assigns the release
gates. The [validation matrix](validation.md) defines the promotion evidence.

## Runtime and analysis inputs

Start with **CPython 3.13.16 on Linux x86_64** and analyzed source target
Python 3.13. Use one default asyncio event loop and the default non-eager task
factory. Use a source-resolved literal `multiprocessing.get_context("spawn")`
for process models and retain the published assumptions for process waits.
Resolve each primitive, constructor and operation against the existing reviewed
model catalog. Bind aliases to the resolved identity and record the model,
runtime and assumptions in the applicability ledger. Missing, conflicting or
unsupported inputs give incomplete coverage at affected operations. Select the
models from the analyzed profile even when the checker runs on another host.
Treat Python 3.11 and 3.12 targets, other platforms and alternate start
methods as boundary controls for the new models. Qualify any additional
profile independently. [Existing runtime inputs](../../../reviewed-contracts.md#select-the-runtime-profile)

Qualify local bindings and aliases, direct calls and awaits, and fixed task or
process arguments with source-resolved entry points. Start with at most four
known actors and eight primitive identities per analyzed protocol, and sixteen
live message records per path. Count the coordinating actor in that bound.
Apply the existing flow budgets as well. A repeated production site whose
instances cannot be distinguished, an exceeded bound, or an escaped identity
produces an explicit boundary. Retain facts that hold across the remaining
paths and scope uncertainty to affected operations.

Reuse the flow engine's argument evaluation, path joins, cleanup and per-exit
semantics. Model application distinguishes normal return, established failure
and unknown effects. Carry item identity, queue accounting, guard acquisition,
work completion, result observation, outcome acknowledgment and cleanup
as independent facts. Keep known duties through conditional paths and unknown
calls, and use the existing certainty and coverage system when a duty cannot
be resolved. [0.9 work and outcome semantics](../0.9/specification.md)

## AS04 asyncio Queue

Qualify FIFO `asyncio.Queue()` with default capacity or a nonnegative literal
`maxsize`. Include direct `await put`, `await get` and `await join`, plus
`put_nowait`, `get_nowait`, `task_done` and `shutdown` with a default or
literal boolean mode. Use bounded source-resolved producers and consumers
with simple local payloads and at most one outstanding receive per consumer
on a queue. Track successful puts as independent admissions, even when two
items carry equal values or refer to the same object. Track a successful
receive and its accounting duty on the same queue. `task_done()` uses a
queue-wide runtime counter. Attribute the static acknowledgment only when
the consumer's duty is unambiguous. An early, duplicate or wrong-queue
acknowledgment is a protocol violation even when a positive counter allows
the call. Use the [accounting boundary for queue joins](#accounting-boundary-for-queue-joins)
to determine which admissions a return covers.

Report a known rejected put or get through the applicable exception edge.
Cancellation on the qualified blocked `put` or `get` path precedes admission
or receipt. After successful receipt, the accounting duty stays pending on
cancellation until the path acknowledges it, including through `finally`.
Cancellation during `join` interrupts the wait. Graceful shutdown rejects
new puts and permits receipt and acknowledgment of buffered items. Immediate
shutdown abandons buffered items and adjusts their counter contribution.
Items already received can still have outstanding accounting. A join released
by that adjustment records the bypass and supplies only the qualified counter
effect. Queue capacity bounds buffered items. Task creation and live work use
their own evidence. [CPython Queue implementation](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/queues.py)

### Accounting boundary for queue joins

For `asyncio.Queue.join()` and `ctx.JoinableQueue.join()`, match the join to
the zero counter observed on entry or the zero notification that released the
waiter. Its cohort includes admissions through that point. Later admissions
retain their own accounting duties even when admitted before the waiter
returns. Use the per-item ledger to distinguish valid acknowledgments from
invalid calls and items abandoned by shutdown. When the zero observation or
the membership of the cohort is uncertain, report incomplete coverage for
the affected accounting claim. Preserve confirmed receipts, acknowledgments
and outstanding duties.
[CPython asyncio queue join](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/queues.py#L205-L251) ·
[CPython JoinableQueue join](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/queues.py#L305-L326)

## AS05 asyncio synchronization

| Primitive | Selected operations and effects |
| --- | --- |
| `Lock` | Direct `async with` and awaited `acquire()` paired with synchronous `release()`, including `try/finally`. Successful entry establishes the guard. A state query reports lock state and supplies no acquisition evidence. A release on a known unlocked lock raises. The local scope carries a release duty while the runtime lock has no task owner. An unresolved handoff or release whose pairing cannot be established retains incomplete coverage. |
| `Semaphore`, `BoundedSemaphore` | Default or positive literal initial value, `async with`, and explicit acquire/release. Record a permit only after acquisition. An ordinary semaphore permits increases above its initial count. A bounded semaphore raises on a release above its initial count. Report proven unmatched release separately as a protocol finding. Held permits bound the modeled guarded work. Pending tasks require their own bound. |
| `Condition` | Default or exact local `Lock`, `async with`, direct `wait`, `notify()` and `notify_all()`. These operations require its exact held guard. A wait relinquishes the guard while suspended and reacquires it before return or the modeled cancellation propagation. A wakeup requires a predicate recheck to establish application state. Callback predicates and unknown lock aliases retain boundaries. |
| `Event` | `set`, `clear`, `is_set` and direct `await wait`. Track the flag and the signal observed by the waiter. A set followed by a clear may wake an already waiting task. A signal supplies no guard, worker completion or result observation effect. |

Distinguish cancellation while waiting for acquisition from cancellation
inside a held scope. Cancelling a semaphore waiter before a permit is reserved
leaves the permit count unchanged. Cancellation after reservation and before
successful acquisition returns the reserved permit, which may immediately be
assigned to another waiter. Both paths leave the cancelled task without an
acquisition or release duty. Confirm any release on the actual exit, including
condition reacquisition under cancellation before lexical cleanup. Standard
`release()` and the default async context's release step are synchronous. An
overridden or otherwise unknown release retains incomplete effects.
[CPython synchronization implementation](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/locks.py)

In addition to the intrinsic condition-to-lock binding, qualify one explicitly
reviewed resource-to-lock association through the project model input. The
initial fact identifies an enclosing coroutine, a resource binding and an
`asyncio.Lock` binding that each resolve to a unique local allocation in that
coroutine, and a source-resolved resource method subject to the guard. Check
the caller's held-lock identity at that operation, including direct aliases
of the resource and the lock. Report a violation when the required lock is
provably absent, including when the caller holds only a different lock. An
ambiguous binding or an escaped alias leaves coverage incomplete. Preserve
supported awaits while the guard is held and enforce the independent
`BorrowedMut` rules.

## MP04 multiprocessing IPC

Qualify one parent and one resolved child per IPC link. Pass a supported
channel or endpoint as an explicit spawn argument, with separate process-local
copies of its handle. Use fixed sends and receives of bounded immutable
built-in values such as integers, strings, bytes and fixed tuples of those
values. A matched receipt creates a reconstructed value with its own
analysis identity. Treat sending custom objects, live resources or endpoint
handles as messages as an explicit boundary. The spawn transport of selected
primitives is governed by MP05.

| Primitive | Selected operations and effects |
| --- | --- |
| `ctx.Queue` | Default or nonnegative literal capacity. A successful `put` supplies local buffer admission. Serialization and pipe writes then happen in the feeder. A successful `get` observes a reconstructed item. `close` ends operations on the local queue copy and schedules a started local feeder to finish. `join_thread` after close with joining enabled establishes local feeder completion on qualified return. Failed or uncertain delivery stays unresolved and consumer receipt has its own evidence. |
| `ctx.JoinableQueue` | Apply the Queue transport and local cleanup effects, plus a separate unfinished-item ledger for `task_done` and `join`. Track known missing, duplicate and wrong-queue acknowledgments. Feeder failure can leave an unfinished count even when the item is unreachable. Apply the same [accounting boundary for queue joins](#accounting-boundary-for-queue-joins). |
| `ctx.SimpleQueue` | `put` serializes and writes in the caller, `get` reads and reconstructs in the receiver, and `close` closes its local endpoints. This protocol has no separate feeder completion or item accounting duty. |
| `ctx.Pipe` / `Connection` | Use fixed unpacking and default duplex or literal `duplex=False`. Cover `send`, `recv`, `close` and `poll(0)`. A unidirectional pipe returns a receive-only end followed by a send-only end. Serialization happens in the sending caller. A successful send establishes a local write and `poll` can report readiness for EOF. Receipt requires a successful `recv`. EOF reasoning accounts for every modeled writer copy, including the parent's retained copy after spawn. |

Qualify direct blocking operations when the applicable peer, ordering and
wait evidence is present. An unproved wait leaves the associated effect
unestablished. Establish drain for a finite cohort from its matched receives,
and account for the reader's ability to drain while the sender waits for the
feeder. Cover closed or wrong-direction use, serialization rejection before
writing, feeder errors and broken or partial writes. Recognize
`cancel_join_thread()` as a loss of guaranteed feeder completion. Keep it
visible in coverage and retain affected delivery and accounting uncertainty.
Advanced drain and forced worker shutdown follow in 0.11. Clear local cleanup
only when the modeled close or feeder effect has actually happened.
[CPython queues](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/queues.py) ·
[CPython connections](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/connection.py)

## MP05 process entry and synchronization

For the initial explicit-spawn model, require a declared direct script launch
(`python path/to/script.py`) with the exact entry file in the selected analysis
scope. Supply the same launch input through the CLI and Rust analysis requests,
and record it in applicability evidence. Qualify a resolved top-level worker
available when the child loads that script or imports a statically resolved
module. Accept `start()` under a literal `if __name__ == "__main__":` guard or
in a resolved direct helper reached exclusively from that guard. Detect proven
import-time startup and known unimportable local or lambda targets.

Treat `-m` launches and an unknown entry mode as coverage boundaries for the
new worker effects. During `python -m package`, `Process.start()` can return
before the child fails to reconstruct a target defined in `package/__main__.py`.
Preserve the established start effect and distinguish
it from both pre-launch rejection and a successful worker entry. Examine
literal `args` and `kwargs` for bounded plain values and exact reviewed IPC or
synchronization primitives passed during spawn. Check the process and primitive
context selections. Unknown import behavior, unresolved targets, custom
reducers, dynamic arguments and captured live resources preserve the boundary.
Worker-local acquisition and cleanup receive their own analysis.
[CPython spawn bootstrap](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/spawn.py#L224-L275)

Qualify `ctx.Lock`, `ctx.RLock`, `ctx.Semaphore` and `ctx.BoundedSemaphore` with
lexical `with` and explicit acquire/release. Include literal nonblocking
acquisition with the returned boolean checked before use or release. Track
recursive ownership and depth for `RLock` for the known acquiring actor.
An unlocked `Lock.release()` raises `ValueError`. A runtime release by a
different actor can succeed on a held plain lock, so distinguish runtime
success from a statically paired release duty. A wrong-actor `RLock.release()`
raises `AssertionError`. A semaphore can rise above its initial count within
the runtime limit. A bounded semaphore raises `ValueError` on over-release.

For two ordered acquisitions, retain the first guard when the second fails.
On a confirmed pre-acquisition failure of the second guard, unwind only the
first established duty through its lexical exit or an explicit `finally`.
With an unknown second acquisition effect, preserve the first guard's known
cleanup and mark only the second effect incomplete.

Start with literal `"i"` typecodes, a literal integer initializer for
`ctx.Value`, and a literal length from one to eight or an equivalent fixed tuple
of integers for `ctx.Array`. Track the exact wrapper lock through `get_lock()`.
Qualify compound updates to `.value` or to one literal in-range index under the
default recursive lock or an exact supplied `RLock` from the same spawn
context. Attribute protection to the full read/modify/write span. An unrelated
lock or individually synchronized accesses provide insufficient guard evidence.
Raw storage, custom wrappers, unknown aliases and a nonrecursive wrapper lock
retain boundaries for the compound-update guarantee. [CPython synchronization](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/synchronize.py) ·
[CPython synchronized values and arrays](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/sharedctypes.py)

## Evidence and before-and-after review

Record selected operation, model and guard identity, relevant origin and
cleanup sites, and certainty on the shared CLI/Rust checks. Reuse Phase 0.9
finding and repair correspondence and coverage prerequisites. For clean and
finding-bearing protocol sites, use a contract-independent match key comprising
the resolved enclosing declaration, operation family and semantic source anchor
of the queue or protected resource operation. Require a unique match in both
captures. Store applicable model identity, the required guard or accounting
obligation, and their applicability as comparison facts outside the match key.
A repaired path can satisfy an unchanged obligation. With complete comparable
evidence, a confirmed change to the required guard or accounting contract
requires `review-required` even when both captures have zero findings. An
ambiguous match or unavailable required evidence leaves the preservation verdict
`inconclusive` in the absence of a conclusive regression. Retain all reasons
under the existing verdict rules. Model, source-target and runtime changes
invalidate affected facts in reused sessions. Analyze every caller, worker and
dependency from source. Execute the runtime cases only as repository-owned
conformance probes. [Change review v1](../../change-review.md)

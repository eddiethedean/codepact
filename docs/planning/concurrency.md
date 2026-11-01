# Concurrent resource safety

Status: **Planned**, adopted 2026-10-31. This is the shared design for resource safety in async tasks and at multiprocessing boundaries. It does not expand the published inspector or the synchronous 0.2.0 lifecycle checker. No gate below has passed.

## Outcome and scope

Prioritize resource lifetime safety for structured async tasks: a task must not use a resource after its owner closes it, a borrowed capture must not outlive its owner, and ownership transfer must carry the cleanup duty. Start with reviewed `asyncio.TaskGroup` models and bounded local task/resource identities. Task completion and task ownership are separate from resource ownership.

This work checks modeled access and cleanup protocols. It does not establish data-race freedom, deadlock freedom, thread safety, scheduling fairness or arbitrary Python memory safety. `Borrowed` does not mean immutable or safe to share. `Owned` does not mean safe to serialize or send to another process. Send/Sync analogues, lock proofs and general cross-process ownership remain research.

## Release mapping and required gates

The [root roadmap](../../ROADMAP.md) remains authoritative. Each CS gate below is required by the listed phase; phase plans reference it rather than defining another acceptance standard. Gates qualify only named models and subsets. Changing scope requires the normal roadmap amendment and evidence review.

| Gate | Release | Work and acceptance evidence |
| --- | --- | --- |
| CS01 | 0.3 | Build coroutine execution, suspension, cancellation and async cleanup edges for a bounded intratask subset. Verify normal return, raise, cancellation during use and cleanup, and unknown cleanup effects. Unsupported task handoffs remain visible. |
| CS02 | 0.4 | Report task origin, resource owner, close/transfer site, suspension or escape boundary, certainty and coverage where modeled. Pilot at least one async application; record unsupported patterns separately from confirmed violations. |
| CS03 | 0.5 | Qualify opt-in structured task captures, owner/borrow regions and explicit transfers with a pinned `asyncio.TaskGroup` model. Accept completion before resource closure; diagnose supported escaping borrows, reuse/double transfer and close while a known borrower remains live. Unknown callbacks, detached tasks and process handoffs cannot establish uniqueness or completion. |
| CS04 | 0.6 | Reject modeled overlapping exclusive access and reject or explicitly report unsupported exclusive borrows spanning suspension according to mode. Qualify finite task-group and local pool protocol states and normal/exceptional effects only for named models. No lock or race-freedom inference. |
| CS05 | 0.7 | Result/exception adapters preserve cancellation and process-control exceptions by default; an outcome wrapper never establishes task completion or resource cleanup. |
| CS06 | 0.8 | Publish a versioned concurrency-model support matrix; qualify CLI/editor/agent and fresh/cached parity for supported models and unsupported process boundaries. Multiprocessing semantic support requires a separate accepted model proposal and evidence; boundary reporting is sufficient for this gate. |
| CS07 | 0.9 | Audit every async/process claim against the model matrix and CS evidence; exclude unsupported or experimental behavior from stable guarantees. |

## Coroutine and structured task semantics

Calling a coroutine function creates a coroutine; it does not establish that its body ran, a resource was consumed, or cleanup completed. Distinguish creation, scheduling, running, cancellation requested and joined/completed tasks. Model resource capture when work can retain an alias, including coroutine creation, rather than waiting until first execution. Do not grant an async close's successful postcondition merely because its coroutine was created.

A reviewed model must specify when execution starts, when an ownership handoff commits, and what happens on scheduling failure, cancellation, exception and group exit. Ordinary argument evaluation retains its specified effects even if scheduling later fails. Do not restore a moved handle without a modeled recovery operation. A pending coroutine can retain a borrow even if it was never scheduled.

A join discharges a task's outstanding borrow only when the model establishes completion of all relevant work. A cancellation request alone does not do that. Exceptional group exit must account for sibling cancellation and waiting; unknown or interrupted cleanup keeps the obligation pending or unknown. Joining a task does not prove it closed an owned resource, and closing a resource does not prove its users finished.

Coroutine functions and async generators need separate models; async generator execution and finalization remain unsupported until qualified.

The initial valid pattern is resource acquisition, a structured group borrowing that resource, completion of all borrowers, then resource cleanup. The invalid counterpart closes the resource inside the group while a known borrower remains active. A borrowed capture that escapes the group is invalid in the checked subset; unresolved closure/container/callback captures produce incomplete coverage. Detached tasks, dynamic task registries and shielded work remain unsupported until separately modeled.

## Suspension, cancellation and cleanup

Represent `await`, `async with` and supported async cleanup as execution edges, with normal, exceptional and cancellation outcomes. A generic context manager does not prove cleanup. Each concrete model describes whether close is synchronous or awaited, when state changes, and whether cancellation or cleanup failure leaves the resource open, closed or unknown.

At suspension, retain facts justified by tracked ownership and effects; weaken facts that an escaping alias or concurrent actor can invalidate. Unknown concurrent access cannot be treated as read-only. An exclusive borrow spanning `await` or `yield` is rejected or reported unsupported according to mode; it is never silently accepted as race-free. Yield remains a separate generator boundary unless its semantics are qualified.

Automatic fixes must not insert cancellation suppression, shielding, task joins, ownership moves or close calls without verified behavioral preconditions. Repair guidance should explain moving cleanup after established task completion or reducing the borrow region; rechecking must retain unknown coverage.

## Multiprocessing and threads

Treat process submission, process queues, executors, shared-memory exposure and thread offload as explicit escape/handoff boundaries unless a reviewed model resolves their effects. Static contract declarations do not make a live file, socket, lock or connection transferable.

Any future process model must specify:

- Serialization/copy versus reference/handle sharing; custom reduction and reconstruction effects.
- Start method (`spawn`, `fork`, `forkserver`), target platform and interpreter versions; inherited descriptors and native-resource validity.
- Sender and receiver resource identities, local handle validity, who closes each handle, and shared backing-resource lifetime.
- Submission success/failure, delivery acknowledgement, worker failure, termination, cancellation and join behavior. Enqueueing, pickling or joining does not alone prove transfer or cleanup.
- Shared-memory unlink/close responsibilities and manager/proxy aliasing; absence of discovered aliases cannot establish isolation.

For 0.5 and later, unresolved process and thread handoffs report incomplete coverage and apply conservative escape/effect rules. No multiprocessing ownership API, serialization guarantee or start-method equivalence is promised by this plan. Pool typestate may check a local submission/shutdown protocol without proving anything about worker resource safety. Research proposals must not silently expand this scope.

## Analyzer and model requirements

Extend canonical summaries with coroutine/task creation, capture/escape, scheduling, completion/join, transfer commit points and cancellation/cleanup effects. Track task identities, borrow regions and resource obligations separately; joins and exceptional exits preserve uncertainty. Bound task/alias graphs and recursive summaries; exhausted budgets produce visible incompleteness rather than completion or uniqueness facts. No scheduler simulation or execution of target code is required.

Models are reviewed source/stub summaries with provenance, version bounds, supported platform/start-method assumptions and valid/invalid/unknown cases. Select exact upstream versions during implementation; proposing TaskGroup is not a current compatibility claim. Model identities and settings are tracked inputs. Edits to capture, join, close or cancellation summaries invalidate dependent facts, locations, certainty and coverage in fresh and reused sessions. Analyzer request cancellation remains separate from cancellation in the analyzed program.

## Validation corpus

Retain paired Pythonic/explicit fixtures for every supported contract and identify the model/version assumptions. Required cases include:

- A borrower completing before close, close before join, nested group scopes, multiple borrowers and a task escaping its owner.
- Coroutine created but never awaited, created inside and awaited outside the resource scope, rejected scheduling, explicit transfer followed by scheduling failure, and detached task capture.
- Cancellation before start, during a borrow, during awaited close, and exceptional group exit with sibling cancellation; cancellation requested without completion evidence.
- Overlapping shared/exclusive regions, exclusive access spanning suspension, mutation by an unknown callback and incomplete task graphs.
- Protocol transitions before/after shutdown, failed start or submission, and exceptional postconditions.
- Unsupported process queue/executor, custom serialization, shared memory, inherited handles and thread-offload cases; no ownership or isolation facts fabricated.
- Model/body/settings edits, moved related spans, task-budget exhaustion, and cached/fresh CLI/editor/agent parity.

Keep static analyzer fixtures separate from optional upstream model-conformance runtime tests. Runtime tests must be controlled fixtures, never imports or execution of users' target projects. Retain candidate evidence under the [common gates](release-gates.md).

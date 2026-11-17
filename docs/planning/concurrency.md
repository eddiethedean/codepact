# Concurrent resource safety

Adopted 2026-10-31; post-0.7 sequence updated 2026-11-17. CS01–CS05 are complete for the published 0.3–0.7 scopes. CS06 moves to 0.12 and CS07 to 0.13 without changing their definitions. New AS/MP/BR gates are planned and pending. [0.3.0 release evidence](../releases/0.3.0-evidence.md) · [0.4.0 release evidence](../releases/0.4.0-evidence.md) · [0.5.0 release evidence](../releases/0.5.0-evidence.md) · [0.6.0 release evidence](../releases/0.6.0-evidence.md) · [0.7.0 release evidence](../releases/0.7.0-evidence.md)

## Outcome and scope

Build on released bounded TaskGroup resource-lifetime checking: a task must not use a resource after its owner closes it, a borrowed capture must not outlive its owner, and transfer must carry cleanup responsibility. Post-0.7 work treats asyncio and multiprocessing as co-primary tracks, with a useful process lifecycle slice in 0.8. Retain reviewed `asyncio.TaskGroup` models and bounded local task/resource identities. Task completion and task ownership are separate from resource ownership.

This work checks modeled access and cleanup protocols. It does not establish data-race freedom, deadlock freedom, thread safety, scheduling fairness or arbitrary Python memory safety. `Borrowed` does not mean immutable or safe to share. `Owned` does not mean safe to serialize or send to another process. Send/Sync analogues, lock proofs and general cross-process ownership remain research.

## Release mapping and required gates

The [root roadmap](../../ROADMAP.md) remains authoritative. Each CS gate below is required by the listed phase; phase plans reference it rather than defining another acceptance standard. Gates qualify only named models and subsets. Changing scope requires the normal roadmap amendment and evidence review.

| Gate | Release | Work and acceptance evidence |
| --- | --- | --- |
| CS01 | 0.3 | Build coroutine execution, suspension, cancellation and async cleanup edges for a bounded intratask subset. Verify normal return, raise, cancellation during use and cleanup, and unknown cleanup effects. Unsupported task handoffs remain visible. |
| CS02 | 0.4 | Report task origin, resource owner, close/transfer site, suspension or escape boundary, certainty and coverage where modeled. Qualify the synthetic async application and boundary controls under [0.4 AD01-A2](phases/0.4/synthetic-preview-protocol.md); real-world async usefulness remains unqualified. Record unsupported patterns separately from supported violations. |
| CS03 | 0.5 | Qualify opt-in structured task captures, owner/borrow regions and explicit transfers with a pinned `asyncio.TaskGroup` model. Accept completion before resource closure; diagnose supported escaping borrows, reuse/double transfer and close while a known borrower remains live. Unknown callbacks, detached tasks and process handoffs cannot establish uniqueness or completion. |
| CS04 | 0.6 | Reject modeled overlapping exclusive access and reject or explicitly report unsupported exclusive borrows spanning suspension according to mode. Qualify finite task-group and local pool protocol states and normal/exceptional effects only for named models. No lock or race-freedom inference. |
| CS05 | 0.7 | Qualify sync and async capture under cancellation, process-control exceptions and exception groups. An outcome wrapper supplies no task completion or cleanup effect. Retain controlled cancellation-before-start, interrupted await and interrupted cleanup cases, and compare fresh/reused analyzer results for the affected summaries. [Adapter specification](phases/0.7/specification.md#explicit-conversions) · [V07 matrix](phases/0.7/validation.md) |
| CS06 | 0.12 | Publish a versioned concurrency-model support matrix; qualify CLI/editor/agent and fresh/cached parity for supported models and unsupported process boundaries. Multiprocessing semantic support requires a separate accepted model proposal and evidence; boundary reporting is sufficient for this gate. |
| CS07 | 0.13 | Audit every async/process claim against the model matrix and CS evidence; exclude unsupported or experimental behavior from stable guarantees. |

## New stdlib semantic gates

Status: **All AS, MP and BR gates pending**. These add separately reviewed semantics and do not reinterpret CS01–CS07. Portfolio PC07 references this catalog. The [library/review/outcome/portfolio work references](contract-and-portfolio-workstreams.md) are not substitute release gates.

| Gate | Release | Required semantics and measurable acceptance |
| --- | --- | --- |
| AS01 | 0.8 | Track coroutine/task creation, capture, scheduling and completion. Pair completion-before-close with live-borrower-at-close; retain unawaited, rejected-scheduling, escaping-capture and exhausted-budget controls with fresh/reused parity. |
| AS02 | 0.8 | Distinguish cancellation/timeout/shield behavior from completion under the bounded TaskGroup baseline and named task profiles. Cover cancellation before start, interrupted wait/cleanup and exceptional siblings; a wrapper return cannot erase unfinished work. Combinator/member responsibility completes under AS03. |
| MP01 | 0.8 | Qualify local Process start/join/close and failed-start/worker-failure effects on a pinned explicit-spawn profile. Cover duplicate start, timed join and premature close. A timed join return is not completion evidence; is_alive false requires an established started path before it can indicate termination. Include plain-value/worker-local-factory controls and unknown live-handle/inheritance cases. |
| MP02 | 0.8 | Qualify local Pool submission, close/terminate/join and context-exit protocols. Cover failed or post-shutdown submission, orderly versus forced shutdown and exceptional exit. A successful forced context exit may complete its termination/join protocol without graceful work completion, result observation or worker-resource cleanup. |
| AS03 | 0.9 | Track asyncio task/future work identity and result/exception observation separately from completion and typed-outcome acknowledgment. Cover assignment, aliases, timeout, cancelled wrappers and stored unobserved work; named gather/wait/as_completed profiles retain unfinished/unobserved members according to accepted effects. |
| AS07 | 0.9 | Qualify bounded async operation sharing and task-use leases before promoting the corresponding LC07/LC12 provider profiles. Separately accept supported concurrent HTTPX and HTTPX2 client requests and separate SQL sessions per task; diagnose concurrent operations on the same AsyncSession and close during known live use. Unknown aliases, callbacks, operation families and unfinished cancellation retain explicit boundaries. Lifetime completion alone cannot establish permission for concurrent operations. See [AS07 acceptance](#bounded-async-operation-sharing-and-as07). |
| MP03 | 0.9 | Qualify Pool AsyncResult submission identity and wait/ready/successful/get effects. Cover timeout, worker exception, alias observation and stored/discarded results. Readiness or process join does not stand for result retrieval or outcome acknowledgment. |
| BR01 | 0.9 | Qualify concurrent.futures submission/Future identities and completion/exception/result effects. Cover failed submission, aliases, result timeout, rejected cancellation, stored unobserved futures and executor context exit. Future.running may mark the cancellation boundary rather than actual user-function execution. Retain underlying work and independent outcome duties; bridge effects require BR02. |
| AS04 | 0.10 | Qualify asyncio.Queue item/accounting, task_done/join and target-specific shutdown effects. Pair valid accounting with missing/duplicate acknowledgment and cancellation boundaries; immediate shutdown cannot fabricate processed work or worker completion. |
| AS05 | 0.10 | Qualify bounded asyncio lock/semaphore/condition/event guard protocols. Cover failed/cancelled acquisition, supported release, missing release and resource access outside a known guard; unknown aliases/callbacks prevent global synchronization claims. |
| MP04 | 0.10 | Qualify selected multiprocessing Queue/JoinableQueue/SimpleQueue and Pipe/Connection close, flush, receive and accounting protocols. Distinguish feeder-thread serialization from caller-side serialization, local flush from consumer receipt, and endpoint copies from EOF. Distinguish acceptance, delivery, accounting, process completion and cleanup; cover broken delivery, abandoned work, closed endpoints and worker-local controls. |
| MP05 | 0.10 | Qualify selected multiprocessing synchronization and process-entry/importability/serialization boundary controls under explicit profiles. Include exact guards for bounded Value/Array compound updates; distinguish Semaphore from BoundedSemaphore. Cover guard violations, partial acquisition, context mismatch, unsupported targets/reducers and live-resource capture; a passing profile does not imply another start method or platform. |
| AS06 | 0.11 | Qualify selected async generator/stream/server/subprocess and advanced async-provider cleanup protocols. Cover lazy acquisition, early abandonment, exhaustion, drain/wait and interrupted finalization; termination request or context exit supplies only exact reviewed effects. |
| MP06 | 0.11 | Qualify partial worker startup, failure, shutdown and bounded shared-memory local-mapping/exported-view/backing-resource ownership. Track name availability and designated unlink ownership independently. Cover close versus unlink, later/existing attachments, premature unlink, missing/retryable cleanup and tracker/platform differences; joining/terminating does not prove worker resources cleaned. |
| BR02 | 0.11 | Qualify named thread/executor/event-loop bridges with dispatch timing, wrapper/underlying identity and cancellation/shutdown effects. Cover selected to_thread/run_in_executor and future/coroutine bridges, cancellation while work continues, queued versus running work and cleanup before/after established completion. |

Phase plans fix the required subset before implementation. Acceptance requires valid, invalid and unknown controls, exact Python/platform/start-method/provider identity, controlled model conformance, CLI/Rust agreement and fresh/reused parity. Persistent-cache/editor/MCP acceptance remains CS06 in 0.12. Target-specific APIs, including queue shutdown and shared-memory tracking options, require their own version profiles. Retain the current [analyzed-source support baseline](../supported-syntax.md); Python 3.13-specific APIs and any proposed Python 3.14 source/default changes require their own target/migration evidence. Event-loop/backend, task-factory and relevant configuration are separate model inputs.

### Independent responsibilities

Track separately: stop/cancellation requested; underlying work completed; exception/result observed; typed result acknowledged; communication accounting completed; resource cleanup completed. Process/task/queue/future waiting, result matching and resource close establish only their modeled effects. They must not collapse into one generic done state.

Contracts and before/after review retain the model, target profile, provider artifact and selected scope supporting each fact. Unknown execution/capture/delivery and exhausted budgets remain visible. Checker-host API availability does not qualify the analyzed target.

## Bounded async operation sharing and AS07

AS07 is required in 0.9 for the operation-sharing claims of the LC07 HTTP and LC12 SQL async profiles. The required provider, operation and configuration subset is fixed before implementation. HTTPX, HTTPX2, SQLAlchemy and any SQLModel async adapter retain separate artifact identities and evidence. A related provider's success cannot qualify another profile.

Keeping a resource alive until all tasks complete establishes a lifetime fact. Permission for those tasks to perform overlapping operations is a separate provider contract. The selected HTTP client request profiles permit shared use; the selected SQLAlchemy AsyncSession profile requires a separate session per concurrent task. Request sharing supplies no permission to close, transfer or mutate arbitrary client settings while another operation is active.

A bounded task-use lease records the resource identity, task or operation identity, permitted operation family and modeled end of use. Each model specifies when capture begins, when an operation can begin, when its permission is required, and which normal, exceptional or cancellation effects end its use. Capture and active operation are separate states. A cancellation request, completed wrapper or interrupted wait cannot release a lease for unfinished underlying work. Unknown aliases, callbacks or exhausted task budgets retain incomplete coverage.

These leases extend only the named async provider protocols. They do not reinterpret the released `BorrowedMut` contract or permit a generic exclusive borrow to span suspension. The existing exclusive-borrow rules remain unchanged; provider-specific session exclusivity is qualified through AS07's operation and task identities. No general lock, race-freedom or arbitrary concurrent-mutation guarantee follows.

Required AS07 controls are:

- Independently accept two supported requests sharing one HTTPX AsyncClient and two sharing one HTTPX2 AsyncClient, with cleanup after established completion. The fixtures select each actual installed provider and do not infer behavior from matching API names.
- Diagnose two concurrent tasks operating on the same tracked AsyncSession even when its owner waits for both tasks before cleanup. Pair this with accepted task-local sessions, each with its own modeled cleanup responsibility.
- Diagnose closing a resource while a known operation still uses it. Pair the case with completion-before-close and retain cancellation-requested-but-unfinished use. Concurrent request permission does not grant concurrent close permission.
- Exercise aliases of the same client/session, supported exceptional exits, rejected scheduling and unknown callbacks or operation families. Separate a supported violation from unsupported operation-sharing evidence.
- Retain model/provider changes in the comparison descriptor and recheck affected callers with CLI/Rust and fresh/reused parity. Keep the ordinary exclusive-borrow-across-suspension regression controls unchanged.

An unqualified sharing or session-exclusivity case is reported as unsupported or incomplete, never accepted as safe from lifetime evidence alone. Boundary reporting is required while implementation is pending, but cannot substitute for the valid and invalid controls needed to promote the corresponding operation-sharing profile. A narrower direct-await profile must state that its concurrent-operation behavior remains unqualified.

## Coroutine and structured task semantics

The 0.5 proposal specifies the initial TaskGroup subset, transfer commit points and required CS03 controls. [Ownership specification](phases/0.5/specification.md#structured-tasks-and-cs03) · [V05 validation matrix](phases/0.5/validation.md)

The published 0.6 preview qualifies local TaskGroup and executor protocol states for CS04; its completed evidence remains the baseline for later extensions. [Named protocol models](phases/0.6/specification.md#named-protocol-models-and-cs04) · [V06 validation matrix](phases/0.6/validation.md)

Calling a coroutine function creates a coroutine; it does not establish that its body ran, a resource was consumed, or cleanup completed. Distinguish creation, scheduling, running, cancellation requested and joined/completed tasks. Model resource capture when work can retain an alias, including coroutine creation, rather than waiting until first execution. Do not grant an async close's successful postcondition merely because its coroutine was created.

A reviewed model must specify when execution starts, when an ownership handoff commits, and what happens on scheduling failure, cancellation, exception and group exit. Ordinary argument evaluation retains its specified effects even if scheduling later fails. Do not restore a moved handle without a modeled recovery operation. A pending coroutine can retain a borrow even if it was never scheduled.

A join discharges a task's outstanding borrow only when the model establishes completion of all relevant work. A cancellation request alone does not do that. Exceptional group exit must account for sibling cancellation and waiting; unknown or interrupted cleanup keeps the obligation pending or unknown. Joining a task does not prove it closed an owned resource, and closing a resource does not prove its users finished.

Coroutine functions and async generators need separate models. Selected generator execution/finalization profiles are planned for AS06 in 0.11 and remain unsupported until qualified.

The initial valid pattern is resource acquisition, a structured group borrowing that resource, completion of all borrowers, then resource cleanup. The invalid counterpart closes the resource inside the group while a known borrower remains active. A borrowed capture that escapes the group is invalid in the checked subset; unresolved closure/container/callback captures produce incomplete coverage. Detached tasks, dynamic task registries and shielded work require separate bounded models. A wrapper or collection model must preserve unfinished work and incomplete coverage.

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

For 0.5 and later, unresolved process and thread handoffs report incomplete coverage and apply conservative escape/effect rules. New process semantics enter through MP01–MP06 and BR01–BR02, starting with explicit spawn and worker-local resource construction in 0.8. Every profile needs an accepted model proposal before implementation and qualification. The first positive control serializes plain values and constructs/closes its resource inside the worker; serializing values does not transfer a parent's resource.

No general multiprocessing ownership API, serialization guarantee or start-method equivalence is promised. A local Pool protocol does not establish worker resource safety. Fork, forkserver, custom reducers, inherited handles and platform differences remain unknown unless their profiles qualify. The analyzed target configuration, not the checker's host defaults, selects Python/platform/start-method behavior.

## Analyzer and model requirements

Extend canonical summaries with coroutine/task creation, capture/escape, scheduling, completion/join, transfer commit points and cancellation/cleanup effects. Track task identities, borrow regions and resource obligations separately; joins and exceptional exits preserve uncertainty. Bound task/alias graphs and recursive summaries; exhausted budgets produce visible incompleteness rather than completion or uniqueness facts. No scheduler simulation or execution of target code is required.

Models are reviewed source/stub summaries with provenance, version bounds, supported platform/start-method assumptions and valid/invalid/unknown cases. Select exact upstream versions during implementation; proposing a new model/version profile is not a current compatibility claim; the released bounded TaskGroup subset retains its existing evidence. Model identities and settings are tracked inputs. Edits to capture, join, close or cancellation summaries invalidate dependent facts, locations, certainty and coverage in fresh and reused sessions. Analyzer request cancellation remains separate from cancellation in the analyzed program.

## Validation corpus

Retain paired Pythonic/explicit fixtures for every supported contract and identify the model/version assumptions. Required cases include:

- A borrower completing before close, close before join, nested group scopes, multiple borrowers and a task escaping its owner.
- Coroutine created but never awaited, created inside and awaited outside the resource scope, rejected scheduling, explicit transfer followed by scheduling failure, and detached task capture.
- Cancellation before start, during a borrow, during awaited close, and exceptional group exit with sibling cancellation; cancellation requested without completion evidence.
- Overlapping shared/exclusive regions, exclusive access spanning suspension, mutation by an unknown callback and incomplete task graphs.
- AS07's independent HTTPX/HTTPX2 shared-request controls, task-local versus concurrently shared AsyncSession controls, and close during known live provider use.
- Protocol transitions before/after shutdown, failed start or submission, and exceptional postconditions.
- Qualified queue/executor/shared-memory/bridge controls alongside unsupported serialization, inherited handles, platforms and start methods; no ownership, isolation, observation or cleanup facts fabricated.
- Model/body/settings edits, moved related spans, task-budget exhaustion, and cached/fresh CLI/editor/agent parity.

Keep static analyzer fixtures separate from upstream model-conformance runtime tests. Controlled runtime conformance is required for each claimed model/version profile; additional research probes can be optional. Runtime tests use controlled fixtures, never imports or execution of users' target projects. Retain candidate evidence under the [common gates](release-gates.md).

## Model reference sources

Select exact supported target profiles against primary references and controlled conformance evidence: [Python 3.13 asyncio tasks](https://docs.python.org/3.13/library/asyncio-task.html), [queues](https://docs.python.org/3.13/library/asyncio-queue.html), [synchronization](https://docs.python.org/3.13/library/asyncio-sync.html), [streams](https://docs.python.org/3.13/library/asyncio-stream.html), [subprocesses](https://docs.python.org/3.13/library/asyncio-subprocess.html), [multiprocessing](https://docs.python.org/3.13/library/multiprocessing.html), [concurrent futures](https://docs.python.org/3.13/library/concurrent.futures.html) and [shared memory](https://docs.python.org/3.13/library/multiprocessing.shared_memory.html).

Implementation-sensitive controls include [CPython 3.13 Pool exit/shutdown](https://github.com/python/cpython/blob/v3.13.0/Lib/multiprocessing/pool.py) and [ProcessPoolExecutor dispatch](https://github.com/python/cpython/blob/v3.13.0/Lib/concurrent/futures/process.py). Source references inform a model proposal; they do not replace conformance for every claimed version/platform profile.

AS07 provider proposals also use the [HTTPX AsyncClient API](https://www.python-httpx.org/api/), [HTTPX2 AsyncClient API](https://pydantic.dev/docs/httpx2/api/api/) and [SQLAlchemy AsyncSession concurrency guidance](https://docs.sqlalchemy.org/en/20/orm/extensions/asyncio.html#using-asyncsession-with-concurrent-tasks). The model matrix pins the actual provider versions and operation families; these references do not qualify all client mutations, SQL adapters or runtime configurations.

# 0.11 specification

These profiles define the initial scope to qualify for CodePact 0.11.0.
The [phase plan](../0.11-shutdown-and-bridges.md) assigns release gates. The
[validation matrix](validation.md) defines the evidence needed for promotion.

## Runtime and analysis inputs

Inherit the 0.10 profile of CPython 3.13.16 on Linux x86_64, source target
Python 3.13, the default asyncio loop and non-eager task factory, and literal
`multiprocessing.get_context("spawn")` with the declared direct-script launch
where Python workers are spawned. Enable the new checks through the existing
opt-in reviewed model settings. Resolve exact stdlib and provider identities
through the existing model catalog. Record the interpreter build, source
target, platform, launch assumptions, model identity and provider or runtime
artifact hashes. The checker selects effects from the analyzed profile.
Shadowing, stale models, unsupported settings and alternate runtimes leave
affected operations incomplete. [0.10 runtime inputs](../0.10/specification.md#runtime-and-analysis-inputs)

Use direct calls and awaits, fixed source-resolved helpers, local bindings and
known aliases. Reuse the inherited flow and work budgets and the 0.10 bounds
of four actors, eight primitive identities and sixteen live message records
in composed protocols. Count cooperating tasks, worker threads and worker
processes as actors. Check fixed known cohorts. Repeated indistinguishable
production, an exceeded bound, unreviewed callbacks or escaped handles leave
an explicit boundary and preserve earlier established facts.

Reuse argument evaluation, branch joins, cancellation and per-exit cleanup.
A confirmed failure before admission creates no new work duty. Partial
success preserves each established acquisition and narrows uncertainty to the
affected effect. Keep admission, execution, stop request, completion,
observation, outcome acknowledgment and cleanup independent. Retain resource
captures while a generator, coroutine or worker may still use them. A
qualified cancellation before admission or successful cancellation of known
queued work can discharge that work's capture. When evidence of that
transition is missing, retain the capture and surface the boundary.
[0.9 work and outcome semantics](../0.9/specification.md)

## AS06 and PC03 async resources

### Generators and cleanup contexts

Qualify a local source-resolved async generator with one known yield site,
plain yielded values and a bounded resource lifetime in `try/finally`. Use
direct `anext()`, `async for`, awaited `aclose()` and lexical
`async with contextlib.aclosing(generator)`. On construction record known
captures. A resource acquired in the generator body begins its lifetime at
the actual acquisition on first or later execution. Closing a never-started
generator releases its captures after qualified completion and adds no
body-local cleanup effect. A yield suspends the frame and retains its duties.

Prove cleanup from the actual exit. Full exhaustion can run the generator's
finalizer. Early break can leave the frame suspended, and an unawaited
`aclose()` supplies only a created coroutine. A failed or interrupted
finalizer retains any established side effects and leaves the unresolved
cleanup duty pending or unknown. Retry counts only when the concrete resource
operation can still complete. Event-loop and garbage-collection finalization
leave explicit boundaries for an abandoned generator. Delegation, concurrent
iteration and ownership transfer in yielded values require a later profile.

Recognize source-resolved `contextlib.asynccontextmanager` on a single-yield
generator when entry, yield and finalization are attributable to that exact
body. Check acquisition before the yield, entry failure, normal exit and
exceptional or cancelled exit through the actual `finally` path. Start with
propagating body exceptions. Unresolved decorators, missing or multiple yield
paths and exception suppression leave incomplete coverage. [Python contextlib](https://docs.python.org/3.13/library/contextlib.html#contextlib.aclosing) ·
[Python async generator methods](https://docs.python.org/3.13/reference/expressions.html#asynchronous-generator-iterator-methods)

### Asyncio I/O shutdown

| Profile | Selected operations and effects |
| --- | --- |
| TCP streams | Direct `asyncio.open_connection` to a fixed loopback endpoint, with fixed reader/writer unpacking. Check bounded literal-sized reads, `write` with bounded bytes, `drain`, `close` and `wait_closed`. Write and drain supply local buffering/flow-control evidence. Peer receipt requires its own modeled read. A close request starts shutdown. A qualified wait establishes closure. Interrupted drain or wait retains the corresponding uncertainty. |
| TCP server | Direct `asyncio.start_server` on loopback with one source-resolved client handler and a fixed connection cohort. Track the listening socket, accepted writer and handler task separately. Default startup can be interrupted after listening begins and before handle delivery. With literal `start_serving=False`, the caller owns the handle before awaiting `start_serving`. `close` stops acceptance. After listener close, `close_clients` or `abort_clients` initiates the corresponding transport shutdown. Qualified `wait_closed` establishes server and connection closure. Handler completion and result observation require separate evidence. |
| Subprocess | Direct `asyncio.create_subprocess_exec` with a fixed resolved executable and literal arguments. Qualify `wait` with no output pipes and `communicate` with selected stdout/stderr pipes, default or disabled stdin, and bounded fixture output. A qualified `communicate` return establishes captured output and exit. A qualified `wait` records the exit status. `terminate` and `kill` send stop requests. A timeout or cancelled wait can leave the child or pipe work outstanding. Pipe drain, child exit and application success each require evidence. |

Retain incomplete coverage for TLS, dynamic handlers, shell launches,
unproved drain ordering or an unresolved partial startup effect. A bare
`wait` with undrained pipes stays a boundary where progress depends on pipe
capacity. A read at EOF, a connection close or a child exit records only
the modeled operation's established effects. [Python streams](https://docs.python.org/3.13/library/asyncio-stream.html) ·
[Python Server](https://docs.python.org/3.13/library/asyncio-eventloop.html#server-objects) ·
[Python subprocesses](https://docs.python.org/3.13/library/asyncio-subprocess.html)

### Library LC07 and LC12

Inherit the exact 0.9 pins and fixture configurations, including the
HTTP/1.1 loopback transport and the file-backed SQLite database with
`aiosqlite==0.22.1`. Capture the full resolved artifacts and backend
configuration for each extension. Select the following families by actual
provider and operation identity. [0.9 provider baseline](../0.9/specification.md#installed-provider-profiles)

| Provider | Initial extension |
| --- | --- |
| `httpx==0.28.1` | Lexical `async with AsyncClient.stream("GET", ...)`, awaited `Response.aread()` or bounded `aiter_bytes()`, awaited `Response.aclose()`, and the reviewed context exit. |
| `httpx2==2.16.2` | Qualify the corresponding public operations on HTTPX2's own response and transport artifacts. |
| `sqlalchemy==2.0.58` | Direct awaited `AsyncSession.stream()` and `stream_scalars()` with a fixed select. Track the actual `AsyncResult` or `AsyncScalarResult`, bounded async iteration and awaited result `close()`. Compare with synchronous consumption of buffered `execute` and `scalars` results. |
| `sqlmodel==0.0.44` | Independently resolve the SQLModel `AsyncSession` and its inherited `stream` and `stream_scalars` using the pinned SQLAlchemy backend. Contrast their async result families with the SQLModel `exec` adapter's buffered row and scalar results. |

Track body availability, response closure and client cleanup independently.
Track SQL result closure, transaction settlement, session cleanup and engine
disposal independently. Full iteration or a context exit can discharge a
duty when the exact provider path establishes that effect. Early break and
interrupted iteration preserve the result or stream duty until a modeled
cleanup path discharges it. Apply the AS07 lease to live stream use of the
client or session, including suspended iteration. Begin with task-local
streaming. Buffered request sharing retains its 0.9 evidence. Overlapping
streaming claims require their own qualification. Preserve the existing
session-exclusivity and generic `BorrowedMut` suspension checks.

Exercise cancellation before acquisition, during iteration and during
cleanup. Record direct close and context exit independently. For a
provider-created cleanup task, discharge resource cleanup only when that
task's success is established. Preserve the pinned SQLAlchemy/SQLModel
session exit behavior for a close task that completes successfully before
cancellation propagates. Cover close failure, cancellation at completion and
an interrupted wait whose cleanup effect is still unknown. Check observable
resource release alongside any status flag, including after a failed close.

Both pinned HTTP providers set `Response.is_closed` before awaiting the
transport close. If that await raises or is cancelled after the flag is set,
a later `Response.aclose()` skips the transport close. This also applies
when the stream context calls `aclose()` on exit. Keep the transport release
duty pending when release is known to be outstanding and record incomplete
coverage when the first attempt's release effect is unknown. Discharge the
duty only when the provider path establishes release.
[HTTPX async support](https://www.python-httpx.org/async/#streaming-responses) ·
[Pinned HTTPX response close](https://github.com/encode/httpx/blob/0.28.1/httpx/_models.py#L988-L999) ·
[HTTPX2 async support](https://pydantic.dev/docs/httpx2/guides/async/) ·
[SQLAlchemy async results](https://docs.sqlalchemy.org/en/20/orm/extensions/asyncio.html#result-set-api-documentation) ·
[SQLAlchemy cleanup helper](https://github.com/sqlalchemy/sqlalchemy/blob/rel_2_0_58/lib/sqlalchemy/ext/asyncio/base.py#L39-L63) ·
[SQLModel adapter](https://github.com/fastapi/sqlmodel/blob/0.0.44/sqlmodel/ext/asyncio/session.py)

## MP06 worker shutdown

Use the 0.10 explicit-spawn entry, plain arguments, local IPC copies and
receiver identity. Qualify a parent with a fixed cohort of source-resolved
`Process` workers, plus the existing local `Pool` with fixed `apply_async`
jobs. Create parent duties as local resources are acquired and track each
worker's start separately. When a later start is rejected, keep the earlier
started worker and its IPC duties. If startup raises after an uncertain side
effect, keep the first worker's known facts and mark the second worker's
startup effect incomplete. A process can start successfully and fail before
or during the worker body.

For orderly shutdown, follow a fixed stop message or Pool `close()` with
proven receive/drain order where applicable, then qualify `join()` and local
handle cleanup. Treat timeout as an interrupted wait unless there is
independent exit evidence. Retrieve a tracked reply or Pool result to observe
the work. A bare process exit status establishes the modeled termination
status. Worker-local cleanup needs evidence on the worker path. Preserve
Phase 0.10's independent queue accounting, feeder completion and endpoint
copy duties through each exit.

Cover `Process.terminate()`, `kill()`, and Pool `terminate()` with
subsequent join. After a confirmed forced exit, settle only the local wait
and handle effects justified by that exit. Track abandoned or uncertain
delivery, receipt, queue accounting and worker cleanup independently. A
forced stop during IPC can leave a partial message or uncertain channel
state. Leave incomplete coverage when drain or cleanup depends on an
unproved peer, a blocked feeder or unresolved ordering.
[Python multiprocessing](https://docs.python.org/3.13/library/multiprocessing.html#multiprocessing.Process)

### Shared memory

Start with one `multiprocessing.shared_memory.SharedMemory(create=True,
size=N)` using a positive literal size and a generated name, and one known
spawned worker that attaches by the creator's exact `.name` passed as a
plain string. Use the default or literal `track=True` in the common ancestor
tracker profile. The creator owns the explicit unlink duty. Each handle owns
its process-local close duty. Tracker registration supplies runtime fallback
evidence. Tracker settings, independent trackers, manager-owned blocks and
Windows semantics require separate profiles.

Keep allocation and name identity, each local mapping, the base `.buf` view
and a bounded derived slice as distinct facts. Track local aliases and direct
view `release()`. A successful close releases its local resources. A known
`BufferError` at mapping close with a live exported slice can follow release
of the base view. Preserve that partial effect, the outstanding local close
duty and a retry through release of the exported slice and a second close.
When the failure point is unknown, report incomplete effects and keep known
facts. Unknown exporters, casts and third-party buffer consumers leave a
boundary for local close.

A successful POSIX unlink removes the name for later attachment. Existing
mappings still have local close duties. Establish premature unlink when it
precedes a known required future attachment. An attach failure after proven
name removal creates no new mapping duty. Track close and unlink in either
order when all required attachments have happened. Promote only bounded
local view and mapping lifetime claims.

An attachment rejected by `shm_open` has no unlink effect. If opening the
existing name succeeds and `fstat` or `mmap` then raises an `OSError`, the
pinned POSIX constructor invokes `unlink()` even on the attachment path.
Record established name removal and the tracker unregistration request
separately. Clear the name-removal duty once removal is established. Retain
the original unlink owner and the cause of removal for diagnostics, including
premature unlink before a required later attachment. Existing mappings and
their local close duties survive name removal. The failed constructor delivers
no new mapping handle. If the failure stage or unlink effect is unknown,
mark the affected name or tracker state incomplete and retain established
mapping facts.
[Python shared memory](https://docs.python.org/3.13/library/multiprocessing.shared_memory.html) ·
[Pinned CPython attachment error path](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/shared_memory.py#L95-L113)

## BR02 executor and event-loop bridges

Reuse the 0.9 work and result facts. Link wrapper handles and aliases to one
underlying work identity. Preserve argument evaluation and captures when an
operation creates a coroutine, and commit submission effects at dispatch.
Keep an outstanding caller-visible observation duty after a wrapper loses
delivery access to known work. A supported result or exception retrieval
through either linked handle can observe delivery. A delivered marked thread
or coroutine outcome retains one acknowledgment duty across aliases and
repeat retrieval. Process results are reconstructed plain values.

| Bridge | Dispatch and qualification |
| --- | --- |
| `asyncio.to_thread` | A call makes a coroutine and captures known arguments. Running the coroutine copies the current context and submits to the known default thread executor. Compare cancellation before coroutine execution with cancellation after the worker has begun. Keep captures across a cancelled wait while the worker can still use them. |
| `loop.run_in_executor` | On a known running loop, the call submits immediately to the default thread executor, an exact local `ThreadPoolExecutor` or a local `ProcessPoolExecutor` with an explicit spawn `mp_context`. Apply the direct-script entry and bounded plain serialization controls to the process executor. Process work past its cancellation point may still be queued for execution. |
| `asyncio.wrap_future` | Wrap an exact thread or explicit-spawn process executor future on the same loop. Preserve its existing submission and work identity. Cancelling the asyncio wrapper can request cancellation of the original future. Establish pre-start cancellation from the original work's effect or a qualified shutdown, independently of the wrapper state. |
| `asyncio.run_coroutine_threadsafe` | From one known non-loop thread, submit a source-resolved coroutine to one known live loop. Distinguish callback admission, loop task creation, task exit and delivery through the returned concurrent future. A cancelled result future can precede task startup or completed task finalization. Qualify blocking `result` or `exception` retrieval from the submitting thread. Same-loop blocking waits and unresolved loop or callback lifetimes remain boundaries. |

For failed dispatch distinguish a rejected submission, a worker exception
delivered through a future and an executor-level failure. A delivered
`BrokenProcessPool` error supplies caller-visible failure evidence. Keep
worker exit and cleanup incomplete until those effects are established.

For both executors, qualify literal `shutdown(wait=..., cancel_futures=...)`
and normal context exit under the exact profile. Count a confirmed queued
cancellation as settled without execution. A shutdown that joins the relevant
worker threads or process-pool manager can establish completion of its
modeled cohort. In the pinned `ProcessPoolExecutor`, `shutdown(wait=False)`
clears the manager-thread reference, so a later `shutdown(wait=True)` or
context exit can return while a job is still running. Retain completion and
capture duties until independent evidence establishes completion or
pre-start cancellation. A later waiting `ThreadPoolExecutor.shutdown()` can
still join its worker threads.
Worker exceptions, result observation and cleanup retain independent
effects. Check normal and exceptional dispatch, including pre-admission
rejection and uncertain executor failure. Qualify `to_thread` context capture
at execution and the lack of implicit context copying in the selected
`run_in_executor` path.
Arbitrary user callbacks, executor replacement, custom reducers and general
cross-loop access remain explicit boundaries.
[Python thread and coroutine bridges](https://docs.python.org/3.13/library/asyncio-task.html#running-in-threads) ·
[Python executor contracts](https://docs.python.org/3.13/library/concurrent.futures.html#concurrent.futures.Executor.shutdown) ·
[CPython future chaining](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/futures.py#L330-L379) ·
[CPython process executor shutdown](https://github.com/python/cpython/blob/v3.13.16/Lib/concurrent/futures/process.py#L795-L811) ·
[CPython thread executor shutdown](https://github.com/python/cpython/blob/v3.13.16/Lib/concurrent/futures/thread.py#L201-L221)

## PC07 composition, reporting and review

Check two end-to-end controls. An asyncio queue consumer owes accounting
after receipt even when cancelled while a bridge worker still retains a
resource. Its owner needs underlying completion evidence before cleanup and
must separately observe delivered work and acknowledge a marked result. A
spawned worker cohort retains distinct duties for receipt, accounting, reply,
exit, local close and shared-memory unlink when startup or shutdown fails.
Use the established finding and coverage output, with the origin, affected
actor or handle, cleanup site and model evidence. Diagnose proven missing
duties or premature cleanup. Report unresolved paths with the existing
certainty and coverage rules.

Use stable operation correspondence from 0.10 even on clean sites. Keep
selected model, ownership assignment and wrapper-to-work relation as
comparison facts. A changed contract at a uniquely matched clean site calls
for review. Ambiguous correspondence or lost model applicability yields
incomplete evidence under the existing verdict rules. Invalidate affected
reuse when source, model, provider, target or runtime inputs change. Repairs
that add a join, shielding, cancellation suppression, termination, move,
unlink or close require the corresponding verified behavioral preconditions.
[0.10 review semantics](../0.10/specification.md#evidence-and-before-and-after-review) ·
[Change review v1](../../change-review.md)

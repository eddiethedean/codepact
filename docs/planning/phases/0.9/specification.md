# 0.9 specification

These are the CodePact 0.9.0 profiles for the [phase plan](../0.9-obligations-and-change-review.md).
The [validation matrix](validation.md) defines the evidence required to promote
these profiles. Use the published [0.8.0 baseline](../../../releases/0.8.0-evidence.md)
for inherited behavior and artifact checks.

## Persistent outcome and work duties

Implement the [MustHandle v1 semantics](../../persistent-outcomes.md) in the
shared analyzer. Use the existing resolved declaration identities and exact
CodePact variant facts. The helper surface is `MustHandle[T]`, an
`Annotated` alias for a callable's outer `Result` or `Option`, and
`discard_outcome(value, reason="…")`. A statically reviewable reason starts
with a nonempty literal string after trimming whitespace. The existing
`MustUse` contract keeps its immediate-use semantics. A persistent duty starts
at an opted-in delivery and survives assignment and aliasing. Published
contracts express the same outward duty through the selected provider and
resolved callable.

Use the flow evaluator's evaluation order and per-exit summaries for ownership
and obligation tracking. Match arms, helper effects and marked return delivery
apply at their established points, including after argument evaluation and
through the actual `finally` exit. Record stable identities for correspondence
at finding-bearing and clean sites. Report an unknown carrier as incomplete
evidence while tracking known aliases. Resource-bearing payloads require
OH08/TC03. Custom adapters and general field/container carriers remain later
work under OH09/OH10.

### Work profiles

Start with **CPython 3.13.16 on Linux x86_64**, source target Python 3.13,
stdlib asyncio with the default event-loop and non-eager task factory, and
explicit `spawn` for multiprocessing. Reuse the local task, Pool and executor
models. The selected operations below establish completion, result or exception
observation, outcome acknowledgment and cleanup independently.

| Gate | Initial profile | Required effect distinctions |
| --- | --- | --- |
| AS03 | Local tasks from `create_task` and `TaskGroup.create_task`, local `loop.create_future()` with supported setter paths, and direct `await`, `result`, `exception`, `done` and `cancel`. Fixed, source-resolved task/future members for `gather`, `wait` and Python 3.13 async iteration over `as_completed`. | Await or retrieval can observe the delivered value or raised worker exception. A status check, group exit, `wait`, or yielding a completed member leaves any unobserved result pending. Timeout, wrapper cancellation and dynamic collections preserve unresolved member work. |
| MP03 | Local explicit-spawn `Pool.apply_async` for a resolved importable worker with plain serializable inputs and results, without callbacks. Track the returned `AsyncResult` through `wait`, `ready`, `successful` and `get`. | Admission, delivery, completion and observation have separate evidence. `successful()` before readiness can raise `ValueError`. `get()` can observe a worker exception or deliver a reconstructed value. A timeout retains observation and any unestablished completion. Pool drain or forced exit supplies only its modeled lifecycle effects. |
| BR01 | Local `ThreadPoolExecutor.submit` under the existing bounded executor profile. Track its `Future` through `result`, `exception`, `done`, `running` and `cancel`. | Confirmed pre-admission failure creates no future duty. Other failed submissions retain any uncertainty about admitted work. Cancellation can be rejected. `running()` can establish that dispatch has passed its cancellation point. Shutdown or context exit can establish completion without retrieving results. |

Caller-visible retrieval of a worker exception discharges its observation duty.
On successful work, checking `exception()` alone leaves the result duty pending.
When a qualified asyncio or thread result delivers a marked outcome, preserve
one pending outcome identity through aliases and repeat retrieval. Then check
its acknowledgment separately. For Pool, qualify plain reconstructed results
and worker-error observation. Cross-process outcome or resource ownership
transfer requires its own model. Callbacks, dynamic task graphs, custom
reducers, other start methods and executor bridges retain coverage boundaries.
[CPython Pool source](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/pool.py) ·
[CPython Future source](https://github.com/python/cpython/blob/v3.13.16/Lib/concurrent/futures/_base.py)

Default `gather` observes the first worker exception it propagates to the
caller. Siblings retain outstanding observation duties until a supported
caller operation receives their results or exceptions, even if `gather`
retrieves a sibling's exception internally. For distinct resolved members in
a fixed positional argument list, qualify direct unpacking of the awaited
result into local names, including with `return_exceptions=True`. A value or
exception delivered in a matched slot observes that member's work. A marked
value carries the same pending outcome duty as direct retrieval through the
original handle and still needs acknowledgment. Other uses of the result list
retain known outcome duties and an explicit boundary for unqualified carriers
or member attribution. [CPython `gather` source](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/tasks.py#L732-L813)

Track up to 64 work identities per path. Fixed `gather` and `wait` collections
accept up to eight distinct members. Async iteration over `as_completed`
accepts up to three so the analyzer can check each completion order. A
repeated production site or exceeded bound retains incomplete coverage.

## Installed provider profiles

Use the same CPython and target profile for the first library matrix. These
versions are the pins for the fixtures. Capture wheel hashes, transitive
dependency artifacts, backend and effective configuration in the reviewed model
and evidence. Apply facts through the 0.8 provider resolution and provenance
ledger. Bind each profile to the selected distribution, canonical symbol and
applicable configuration. A substituted, stale or shadowed provider exposes
incomplete evidence at affected sites.

| Provider and gate | Proposed pins and scope |
| --- | --- |
| HTTPX sync, LC05 | `httpx==0.28.1`. Local `Client`, buffered `get`/`request`, basic `with` and `close`, plus lexical synchronous `client.stream(...)` with supported body read and context exit. |
| HTTPX2 sync, LC05 | `httpx2==2.16.2`. Qualify the same selected operation families on its own transport stack and public symbols. |
| HTTPX async, LC07 / AS07 | `httpx==0.28.1`. `AsyncClient`, directly awaited buffered `get`/`request`, `async with` and awaited `aclose`. Qualify two known child requests on one open client and cleanup after their use ends. |
| HTTPX2 async, LC07 / AS07 | `httpx2==2.16.2`. Qualify the same bounded async operations and sharing with separate artifacts and probes. |
| SQLAlchemy sync, LC09 | `sqlalchemy==2.0.58` with SQLite and stdlib `sqlite3`. Use a local `Engine` or explicit scoped `Connection`, local `Session`, `begin`/`commit`/`rollback` and `close`, and simple `execute`/`scalars` with supported result consumption. Use the reset-on-close session profile and explicit `Engine.dispose()` when owned by the fixture. |
| SQLModel sync, LC10 | `sqlmodel==0.0.44` on the pinned SQLAlchemy/SQLite stack. Qualify `Session.exec` for both a scalar and a row result, session cleanup, and the adapter's own selected identity. |
| ETLantic `session_scope`, LC10 | `etlantic-sqlmodel==0.59.0` with `etlantic==0.59.0`, the `dpcs==0.13.1` Linux x86_64 wheel (glibc 2.34 or later) and the pinned SQLModel stack. Consume an external reviewed summary of `etlantic_sqlmodel.control_plane.session.session_scope`. It yields a scoped session, commits on normal body exit, attempts rollback on `Exception` including a commit error, and attempts close in `finally`. A non-`Exception` exit bypasses the explicit rollback handler. Record the effects actually established when commit, rollback or close raises. |
| SQLAlchemy async, LC12 / AS07 | `sqlalchemy==2.0.58` with `aiosqlite==0.22.1`. Use an `AsyncEngine` and local `AsyncSession`, direct awaited `execute`/`scalars`, supported transaction operations, `async with` and awaited `close`/engine disposal. The buffered result is consumed synchronously. Distinguish `stream`'s async result family and retain a boundary for its lifetime/consumption protocol. |
| SQLModel async, LC12 / AS07 | `sqlmodel==0.0.44` on the pinned async SQLAlchemy stack. Qualify direct `await` of `sqlmodel.ext.asyncio.session.AsyncSession.exec` with scalar and row results and the adapter's own operation identity, task exclusivity and cleanup. |

Use HTTP/1.1 built-in transports against a controlled loopback server as the
primary HTTP control. Run with `trust_env=False`, no redirects and no optional
protocol extras. Provider-specific mock transports can exercise deterministic
exception paths. Client cleanup, response stream closure and body availability
are separate. In particular a buffered body can remain readable after the
client closes, while an unread stream closed on exit cannot be newly read.
Async streaming, custom transports outside the fixture, HTTP/2, proxies and
advanced cancellation/stream cleanup require separate evidence. [HTTPX
transports](https://www.python-httpx.org/advanced/transports/) · [HTTPX2
transports](https://pydantic.dev/docs/httpx2/advanced/transports/)

Use a temporary file-backed SQLite database with a fixed schema and explicit
transaction controls. Record the SQLite runtime version. Treat settlement,
result consumption, connection release and session reuse as distinct facts.
Default reset-on-close permits supported session reuse. New use can acquire
another cleanup duty. A session context's close effect supplies no commit
claim. Use the actual `Session.exec` and `AsyncSession.exec` adapter and compare
against the qualified SQLAlchemy controls. The external ETLantic summary is
qualified separately from generator-source and framework dependency analysis.
[SQLAlchemy session behavior](https://docs.sqlalchemy.org/en/20/orm/session_basics.html) ·
[SQLAlchemy asyncio](https://docs.sqlalchemy.org/en/20/orm/extensions/asyncio.html) ·
[SQLModel async adapter](https://github.com/fastapi/sqlmodel/blob/0.0.44/sqlmodel/ext/asyncio/session.py) ·
[ETLantic SQLModel publication](https://pypi.org/project/etlantic-sqlmodel/0.59.0/)

Distinguish cancellation during direct `await session.close()` from cancellation
during `async with` exit. When a direct close is interrupted before release is
established, keep cleanup pending or report an unknown effect. In the pinned
context-manager path, the provider shields its close task and waits for it.
When the close task succeeds after cancellation of the waiting caller, record
cleanup completion before propagating cancellation. Preserve unknown effects
when close raises before cleanup is established. Qualify both paths separately
for SQLAlchemy and SQLModel. [SQLAlchemy session context exit](https://github.com/sqlalchemy/sqlalchemy/blob/rel_2_0_58/lib/sqlalchemy/ext/asyncio/session.py#L982-L987) ·
[SQLAlchemy cleanup helper](https://github.com/sqlalchemy/sqlalchemy/blob/rel_2_0_58/lib/sqlalchemy/ext/asyncio/base.py#L39-L63)

Apply [AS07](../../concurrency.md#bounded-async-operation-sharing-and-as07)
leases only to the named provider operations and supported task shapes. An
async call captures the resource when it can retain it. Active use begins on
modeled execution and ends on a qualified normal, exceptional or completed
cancellation path. Accept independent task-local SQL sessions. Diagnose known
overlapping operations through aliases of one session and closing any selected
resource during known active use. An interrupted wait or cancellation request
alone cannot grant close permission. Unresolved aliases or operations retain
incomplete coverage. Preserve the generic `BorrowedMut` across-suspension rule.

## Declaration policy, snapshots and author comparison

**E12 declaration policy.** Select exact synchronous APIs or modules and opt in
to independent requirements for resource-parameter access, supported ownership
handoffs and immediate-use declarations. Report a definite missing declaration
on the signature when the symbol and obligation are established. Exercise
preview, enforcement and baseline matching through the existing configuration
and report policy. Baseline disposition leaves the raw evidence available.
Strict mode still fails when required coverage is incomplete. Report rule and
contract origins, including project provenance and selected provider version.
Stale or ambiguous contracts cause incomplete coverage. Malformed or conflicting
configuration exits `2`. Version the effective policy and contract compatibility
inputs so incompatible baselines cannot mask changed agreements.

**Source review.** Reuse the 0.8 snapshot, scope, settings and model inputs.
Keep source content hashes in snapshot identity and record a separate versioned
comparison descriptor for engine, rule, target, model and matching semantics.
Include raw findings, disposition, clean-site anchors and required coverage in
the completed evidence. A pure comparator can produce a historical conclusion
from two captures. A current-workspace claim requires verification of the after
snapshot against current inputs. Apply the v1 verdict precedence and retain all
blocking reasons. The `codepact review` command accepts two completed
captures, supports text and versioned JSON, and uses exit `0` for pass, `1` for a
completed nonpass and `2` for input or execution failure. The CI recipe pins the
checker and obtains the reference and policy from the trusted host workflow.
[Change review v1 prerequisites](../../change-review.md#preservation-pass-requirements) ·
[Verdicts and exit status](../../change-review.md#verdicts-and-exit-status)

**Author artifact comparison.** The installed `codepact contracts
diff` command compares two actual local wheel publications using the bounded
E10-A reader and validator. Compare normalized public import paths, applicable
caller obligations and supplier guarantees. Report compatible differences,
new/stronger caller obligations, weakened guarantees and insufficient evidence.
Deterministic text and versioned JSON retain the per-symbol facts that support
the result. Invalid archives use the author command's input-error path. Valid
publications with missing applicability or evidence report insufficiency. Source
review may reuse the normalized delta logic. Each workflow earns its own
qualification. [Published contract comparison](../../published-contracts.md#contract-compatibility-and-upgrades)

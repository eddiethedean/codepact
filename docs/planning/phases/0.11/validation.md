# 0.11 validation matrix

Use published [CodePact 0.10.0](../../../releases/0.10.0-evidence.md) as the
comparison baseline. Expected effects and applicability come from the
[0.11 specification](specification.md). The
[implementation evidence](implementation-evidence.md) records the candidate
checks. The release evidence records the final gate decisions.

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V11-01 | AS06, MP06, BR02, E04 | Resolve direct and aliased constructors, calls, handles and models. Keep caller source fixed while changing runtime, source target, launch declaration, start method or provider artifact. Include a shadowed callable and missing applicability. Record the selected model and explicit boundary at affected calls. Exercise 3.11/3.12 and an alternate platform as boundary controls for the new models. |
| V11-02 | AS06, PC03 | Compare construction without execution, closing before first iteration, capture of an existing resource, acquisition at first iteration, exhaustion, early break, unawaited close, awaited close and `aclosing`. Pair a clean finalizer with a cancelled or failing close and an escaped generator. Verify body-local acquisition, capture and cleanup separately, including the actual effect of a supported cleanup retry. |
| V11-03 | AS06, PC03 | Exercise a resolved single-yield `asynccontextmanager` with failure before acquisition, failure after acquisition before yield, successful entry, body exception and cancellation during exit. Assert the effects of its exact `finally` block. Pair a yielding cleanup context with an unresolved handoff, multiple yields and suppression as boundaries. |
| V11-04 | AS06 | Compare writer cleanup with close only, unawaited or interrupted `wait_closed`, successful wait and a surfaced transport error. Gate peer reads to establish when receipt is known. For a one-client server, exercise listener close while the connection remains active, graceful client closing and abort, then successful and interrupted server waits. Keep a live handler pending after transport closure when its result or completion is unresolved. |
| V11-05 | AS06 | Pair subprocess `wait` without output pipes and `communicate` with output pipes. Verify normal exit, a nonzero status, startup rejection, known termination request with and without a subsequently observed exit, and interruption during drain or wait. Exercise undrained piped output and an unresolved executable as boundaries. |
| V11-06 | AS06, LC07, E04 | Qualify actual HTTPX and HTTPX2 installations independently. Compare a buffered response with a lexical async stream, full read/exhaustion with early break, known close with use after close, and response closure with client cleanup. Run the [provider cancellation controls](#provider-cancellation-controls) for each. Include swapped artifacts and escaped responses. |
| V11-07 | AS06, LC12, E04 | On the pinned SQLAlchemy/aiosqlite stack, check buffered row/scalar results against `AsyncResult` and `AsyncScalarResult`. Exercise full consumption, early exit, awaited close, a missing or unawaited close, and session cleanup during known live use. Run the [provider cancellation controls](#provider-cancellation-controls). |
| V11-08 | AS06, LC12, E04 | Repeat with the actual SQLModel adapter, selecting its inherited stream operations and contrasting `exec` on fixed row and scalar selects. Assert SQLModel provenance and the backend artifact identity. Run the [provider cancellation controls](#provider-cancellation-controls) independently and exercise a substituted backend. |
| V11-09 | MP06 | Start two known workers. Compare both starts succeeding, second-start rejection before admission, and an unresolved partial startup effect. Compare with a child that starts and then crashes. Verify surviving first-worker obligations and parent cleanup of local IPC copies on each path. Qualify orderly stop-message, receive/drain, reply, join and handle close. |
| V11-10 | MP06 | Compare Process terminate or kill followed by a known exit with stop request alone and a timed-out join. Compare Pool close/drain/join and result retrieval with terminate/join and unresolved job results. Exercise forced stop after an IPC admission and before an acknowledged receipt. Keep feeder, delivery, accounting, result observation and worker-local cleanup distinct. |
| V11-11 | MP06 | Create one shared allocation and attach in a spawned worker by exact name. Exercise aliases and distinguish creator and child mapping close, view release and the creator's unlink duty. Close before unlink after required attachments. Unlink with an existing attachment and verify local duties survive, then contrast a later failed attachment and a missing unlink. Run the [attachment failure controls](#attachment-failure-controls). |
| V11-12 | MP06, E04 | Hold an exported slice while closing a local mapping. Establish the known partial close and retry after releasing the slice. Pair with a missing release, unknown exporter and missing retry. Exercise nondefault tracking, unrelated trackers, the manager-owned lifecycle and Windows as applicability boundaries. Where runtime probes demonstrate differences, record each profile as a contrast to the promoted Linux spawn model. |
| V11-13 | BR02 | Evaluate side-effecting arguments to `to_thread`, then compare an unstarted coroutine, cancellation before first execution, normal dispatch, pre-admission rejection and cancellation after worker entry. Use a context variable set to different values at construction and execution. Verify capture lifetime and close-before-completion versus close-after-completion. |
| V11-14 | BR02 | Verify immediate `run_in_executor` submission for the default thread executor, an exact local thread pool and an exact explicit-spawn process pool. Assert context propagation separately from `to_thread`. Exercise rejected submission, a delivered worker exception, a broken process pool and unsupported serialization or bootstrap. Keep executor-level error delivery separate from worker exit and cleanup. Check queued versus running work with the [bridge ordering controls](#bridge-ordering-controls). |
| V11-15 | BR02 | Wrap already-submitted thread and process futures and verify the same underlying identity through aliases. Exercise duplicate retrieval, timeout, pre-start cancellation, rejected cancellation and an unresolved user callback. Cover both executor shutdown flags with controlled pending and active work, including the repeated-shutdown cases in the [bridge ordering controls](#bridge-ordering-controls). Verify caller-visible observation and marked thread-outcome acknowledgment independently. |
| V11-16 | BR02 | Submit from a known non-loop thread to a known running loop. Compare caller-visible normal and exceptional delivery, timeout, rejection on a closed loop, cancellation before the loop callback and cancellation while the coroutine finalizer is suspended. Keep completion and cleanup pending where only the result future is cancelled. Include same-loop blocking retrieval and an unknown loop as boundaries. |
| V11-17 | PC07, AS06, MP06, BR02 | Compose the two end-to-end cases in the specification. Pair balanced accounting and cleanup with missing acknowledgment, early close, undelivered reply, forced worker exit and premature unlink. Include an unknown callback or alias. Assert that proven admission, result and cleanup facts survive a later unresolved effect. |
| V11-18 | G2, G4 | Apply the [source review control](#source-review-control), then replay the matrix through normalized installed CLI and Rust results. Compare fresh and reused sessions across source, model, provider, target and runtime edits. Check actual affected-query invalidation, reuse on unchanged and unrelated edits, cancellation or budget exhaustion, and subsequent recovery. |
| V11-19 | E04, E05, G2–G3, G5–G6 | Capture the exact candidate, interpreter build, upstream artifacts and applicability. Replay representative valid, invalid and boundary fixtures with the packaged command. Preserve the inherited helper, CLI extra, standalone/native wheel, source archive and Cargo checks. Verify that static analysis never imports or executes analyzed sources or workers. Check examples, diagnostics, limitations and migration notes against qualified effects. |
| V11-20 | G4 | Compare candidate and published 0.10.0 on the same host for inherited work, queue, IPC and review workloads. Add async resource, shutdown, bridge and shared-memory workloads. Record cold and unchanged-session time, affected and unrelated edits, model/profile invalidation, review capture and comparison cost, and peak RSS. Exercise actor, primitive, message and existing flow/work limits independently. Record deterministic coverage, preserved facts, recovery and the disposition of regressions. |

## Provider cancellation controls

Run the following controls independently for each selected provider using the
pinned transport or backend. Record the operation, result family and
observable cleanup. Keep the buffered and streaming profiles identifiable.

- Gate acquisition and iteration separately. Cancel before result delivery
  and while a live stream or result is suspended. Assert which duties were
  created and which survive.
- Gate cleanup and cancel during direct close and during lexical exit where
  the provider offers that path. Include close failure and success before
  cancellation propagates where the implementation establishes it. For
  SQLAlchemy and SQLModel, test direct result close separately from direct
  session close and session context exit. Test cancellation racing with the
  session close task's success or failure.
- For HTTPX and HTTPX2, gate the transport close after `is_closed` is set.
  Inject a failure or cancellation before release, catch it inside the
  lexical stream, and await `response.aclose()` again before leaving the
  context. Assert that the retry and context exit return without another
  transport-close attempt. Check that the release duty survives both returns,
  with incomplete coverage when the release effect is unknown. Pair this
  with a first close that successfully releases the transport.
- Observe release at the transport or backend in addition to the public status
  flag. Keep partial release and unknown effects visible. Pair AS07 live-use
  violations with clean task-local controls and rerun the buffered sharing and
  ordinary `BorrowedMut` suspension controls.

## Attachment failure controls

With a creator mapping live and its name registered, inject a rejection at
`shm_open` for a worker's attachment and verify that the name and tracker
registration survive. In separate cases, let the open succeed and inject an
`OSError` at `fstat` or `mmap`. Verify that the constructor delivers no
handle, removes the name and requests tracker unregistration when unlink
succeeds. Keep the creator mapping usable and its close duty pending, and
verify that a subsequent required attachment fails. Cover an unresolved
failure stage or unlink effect and check that incomplete name or tracker
evidence preserves the creator's known mapping and ownership facts.

## Bridge ordering controls

Use explicit readiness and release signals in repository-owned probes. For
thread and process pools, distinguish a job still eligible for cancellation,
a job past the cancellation point and a worker that has entered user code.
Record evidence from the original future or an applicable shutdown effect
before settling a cancelled queued job. A wait cancelled after worker entry
leaves the worker live until release and independent completion. Verify that
waiting shutdown can settle a completion cohort while an undelivered result
remains unobserved. For a gated process-pool job, call `shutdown(wait=False)`
and follow it with `shutdown(wait=True)` in one case and executor context
exit in another. Establish that each returns before the job is released.
Keep completion pending until separate evidence establishes it. Pair with
a first waiting process-pool shutdown and with a waiting thread-pool shutdown
after `wait=False`. Verify that those waits stay blocked until the job can
finish. For cross-thread coroutine submission, gate the loop task's
finalizer and verify that cancellation of the concurrent future can be
observed first. Bound probe waits with deadlines and use ordering signals to
establish the tested states.

## Source review control

Keep a shared-memory attachment or bridge operation at a stable source anchor.
Change the required cleanup relationship at that site, for example the
underlying job that borrows a resource, and update its caller to satisfy the
new relationship. Require complete comparable evidence and zero findings on
both sides. The verdict is `review-required` with the changed contract
fact. Pair this with an edit that repairs premature cleanup under the same
contract. Include ambiguous matching and a now-inapplicable model, which
yield inconclusive evidence absent a conclusive regression. Apply the
published [verdict precedence](../../change-review.md#verdicts-and-exit-status).

## Evidence capture

For a supported valid case, require complete coverage and the expected
settled duties. For an invalid case, assert finding identity, certainty and
the relevant origin, operation and cleanup locations. For a boundary,
assert incomplete coverage at the affected operation alongside all known
effects and duties. Static fixtures are source inputs. Execute only the
repository-owned conformance fixtures. Use explicit synchronization to
control ordering and isolate probe resources so a failing probe still
reclaims its processes, sockets and shared-memory blocks.

Record commands, fixture IDs, versions, hashes, raw outcomes, applicability
and normalized reports against the exact candidate. Keep runtime conformance
and static expectations paired by case. Record the result for every
required row and retain inherited release evidence under the common reuse
rules. [Release evidence template](../../release-evidence-template.md) ·
[Common release gates](../../release-gates.md)

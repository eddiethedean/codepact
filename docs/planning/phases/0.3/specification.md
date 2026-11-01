# 0.3 cleanup specification

Status: **Bounded implementation is in the candidate; maintainer semantic acceptance and release qualification remain pending.** This is the review oracle for [CL01–CL08 and C01–C05/C07/CS01](../0.3-cleanup.md). The [V03 matrix](validation.md) defines required evidence. Preserve the [0.2 released semantics](../0.2/specification.md) except where this proposal explicitly extends them.

## Supported slice

Check synchronous functions and bounded async bodies, with local resource construction, exact/uncertain aliases, resolved calls, direct owning returns, `if`, ordinary `try` handlers, `else`, `finally`, `while`, `for`, `break`, `continue`, and reviewed `with`/`async with` models. Ordered subexpression effects must survive abrupt exits. Unsupported expressions, iterator effects or handler matching produce an explicit boundary and weaken affected facts.

Async execution is limited to directly awaited, resolved coroutine bodies and reviewed awaitable summaries within one task. Local coroutine variables may retain captures until a direct await. Task scheduling/joins, arbitrary awaitables, async iteration, generators, exception groups/`except*`, dynamic handlers, arbitrary decorated context-manager bodies, callback/container/field ownership and runtime finalizers remain unsupported. Detect these boundaries without inventing execution, purity or cleanup.

The default scope includes obligations for resource acquisitions inside supported functions, even if the function has no annotated parameters. Module/global resource lifetime, interpreter shutdown and external entry callers are boundaries unless a reviewed model supplies an explicit scope; module completion must not imply process shutdown or automatic cleanup. Keep 0.2 module lifecycle diagnostics.

## User-facing example

This is proposed 0.3 behavior, using existing declaration spellings:

```python
from typepact import resource
from typepact.rust import Borrowed, Owned

@resource(closed_by="close")
class Connection:
    def close(self): pass  # Declared transition, not runtime cleanup evidence.
    def status(self): return "ready"

def leak():
    connection = Connection()
    return  # Proposed TP1002: owned local remains open on this exit.

def safe():
    connection = Connection()
    try:
        connection.status()
    finally:
        connection.close()

def make() -> Owned[Connection]:
    return Connection()  # Hand off the duty only on successful return.

def read(connection: Borrowed[Connection]):
    connection.status()  # No duty inherited; closing here is TP2001.
```

`safe` discharges on successful close. A resolved, supported close body contributes its checked raising paths: `pass` has no modeled raising path, while an explicit raise preserves the pre-transition open duty. An unknown method or unsupported operation retains incomplete coverage. The declaration remains `declared-unverified`; it does not prove runtime cleanup. Equivalent `Annotated`/explicit metadata must lower to the same duty/effect semantics.

## Resource and obligation domain

Track resource identity, lifecycle state, duty holder, obligation status and provenance separately. A resource can be open with a duty held by a caller, or have unknown state with an unresolved duty. Closing an object and completing work that captured it are different facts.

| Event | Proposed duty rule |
| --- | --- |
| Successful local resource construction or modeled acquisition | Create one duty in the current function scope, after the acquisition succeeds |
| Failed acquisition | No returned-object duty; preserve effects/duties created earlier in evaluation; partially constructed internal resources require a model or boundary |
| Explicit `Owned[T]` parameter entry | Callee is responsible for the abstract incoming duty; this asserts no alias uniqueness |
| `Borrowed[T]` parameter entry | No inherited duty and no permission to close or return as owned |
| Ordinary unqualified resource parameter | Preserve 0.2 access/effects; no implicit incoming cleanup duty or consuming-call contract |
| Local alias/rebind | Share the same duty; rebinding does not discharge it or create another duty |
| Successful supported close | Discharge for that object with the close postcondition's trust |
| Checked `Owned[T]` return | Hand off the same duty on actual normal function exit; preserve result identity at the caller |
| Storage, opaque return, unknown call or handoff | Retain unresolved/escaped duty and incomplete coverage; do not claim successful transfer |

Use a finite status domain distinguishing pending, discharged, handed-off and unknown, with joined alternatives and presence information. Absence on a path is not evidence that an existing duty was discharged. Mixed pending/discharged paths retain a possible finding. Unknown alternatives cannot become definite safety or a definite leak; retain independent known witnesses and their certainty.

Losing the last local alias does not destroy the obligation. Keep acquisition identity and exit evidence so a rebind cannot erase a leak. Uncertain alias targets receive weak updates; a close of one unknown target cannot discharge all candidate duties. `BorrowedMut` does not gain ownership or qualified exclusivity in this phase; retain its existing boundary unless a separate decision defines supported behavior.

## Calls and duty handoff

Summaries distinguish argument evaluation, entry acceptance, normal return, raise and cancellation. Preserve alias relationships when the same object occupies multiple parameter positions. Ordered argument/default/receiver effects happen before callee entry; failure before entry cannot transfer a duty.

A resolved `Owned` parameter delegates the duty at supported entry. Apply its checked outcome summary back to the same object: successful cleanup discharges, an owning return restores the duty with its result identity, and exceptional/unknown outcomes retain their specified obligation state. Report a callee's known unfulfilled duty at its exit, with the caller origin where available; do not independently report the same delegated duty as a second caller leak. Opaque declarations supply no verified cleanup guarantee. Callee body checks and call-site instantiations must share a deterministic reporting policy.

For async callees, creation retains arguments/captures but does not enter the body or transfer the caller duty; direct await uses the reviewed execution-entry/cancellation model. Cancellation before body entry cannot make an owning argument disappear. Calling a borrowed helper never delegates the owner duty. Independently acquired resources in any helper remain that helper's responsibility.

Returning a resource as `Owned[T]` transfers its duty only after return-value evaluation and all pending cleanup complete. Retain a pending return value while `finally` runs. A replacing return/raise cancels the earlier transfer; an object left behind still needs cleanup or a diagnostic. Closing the pending returned object makes the owned-open return invalid; diagnose a contract violation rather than grant a valid open result. CL01 must select the exact code for that violation. Unknown result aliases and borrowed-to-owned returns cannot establish a handoff.

This is cleanup-accounting transfer only. General use-after-move, double transfer, unique handles and borrow escape enforcement remain 0.5 work; retained aliases prevent any uniqueness claim.

## Exit and cleanup edges

Carry a pending completion reason and resource state through cleanup. Route evaluation failure before the next operation; code after an unconditional return or raise is unreachable. Candidate lowering must conform to the [Python compound-statement reference](https://docs.python.org/3.13/reference/compound_stmts.html#the-try-statement), with exact supported interpreter versions recorded at qualification.

| Completion | Required cleanup routing |
| --- | --- |
| Fallthrough / return | Unwind active contexts/finally before actual scope exit and any return handoff |
| Raise | Preserve exceptional effects and route to applicable handlers/outer cleanup; no normal close postcondition on this edge |
| Break / continue | Unwind regions left by the jump, then reach the loop target; do not exit the containing function |
| Cancellation | Preserve it as a distinct completion class and unwind modeled cleanup; keep interrupted cleanup outcomes separate |
| Abrupt exit inside cleanup | Replace the pending completion as required; keep effects and obligations accumulated before replacement |

Ordinary calls have exceptional edges unless checked inference or a reviewed model establishes otherwise. Evaluate `raise` operands and handler expressions with their effects. Handler routing preserves source order and sends uncertain matches to feasible outcomes with visible incompleteness; never discard an unmatched exceptional edge. `else` is reached only from normal completion of the try body. Exceptions from handlers/else still cross the enclosing finally. Nested cleanup runs in stack order.

Do not treat `except Exception` as a universal cancellation/process-control handler. Bare and explicit broader handlers require distinct matching; unknown exception types cannot prove all exceptional exits handled. Catching an error does not itself close a resource. Unsupported exception-group routing keeps the affected region incomplete.

## Close outcomes and policies

| Close outcome | Required lifecycle and duty result |
| --- | --- |
| Normal successful close | Closed/discharged, with declared-unverified or reviewed-model provenance as applicable |
| Raise with a reviewed closed postcondition | Closed/discharged even though control remains exceptional |
| Raise from a checked source close body | Preserve the lifecycle state at that raise; the declared normal transition is not applied to the exceptional edge |
| Raise from an unresolved/external close | Unknown lifecycle/unresolved duty and a boundary |
| Cancelled awaited close | Use its explicit cancellation postcondition; otherwise unknown/unresolved with a boundary |
| Async close coroutine merely created | No close execution or discharge; retain its capture |

Checking a local close body preserves its effects and explicit exceptional paths before applying the declared successful transition. A declaration does not imply a no-raise or atomic cleanup policy. An external or unresolved close has no inferred exceptional state. Repeat close is accepted only with a supported idempotency policy; a requires-open policy can reject it, and no policy retains the 0.2 unknown boundary. A model may permit named post-close operations without restoring open state or creating/discharging a duty; other accesses keep existing preconditions.

No automatic fix may add close, suppress cancellation, shield, move ownership or insert task joins based solely on an outstanding duty. Explain the failing exit and model assumption; rechecking must retain unknown coverage.

## Context models

Separate context-manager identity, entered resource identity and cleanup responsibility. A reviewed model supplies enter success/failure, result aliases, exit effects for normal/exceptional/cancelled outcomes, suppression and trust. Failure to enter does not invoke that manager's exit; previously entered managers still unwind. Multiple managers enter in order and exit in reverse order. Unknown suppression keeps both propagation and continuation feasible.

Minimum catalog: source-only, version-bounded summaries for `contextlib.closing` and `contextlib.aclosing`. They delegate to the wrapped object's resolved `close`/`aclose` effects, including checked exceptional paths and cancellation; the wrapper supplies no stronger postcondition. The [contextlib reference](https://docs.python.org/3.13/library/contextlib.html#contextlib.closing) provides the starting behavior; exact source revisions and supported Python versions still need conformance evidence. Async-generator finalization remains outside this slice even when a wrapper is recognized.

Unknown/shadowed/lookalike managers retain incomplete coverage and cannot discharge a duty. `__del__`, garbage collection and process exit are never deterministic cleanup. Borrowing a resource into a wrapper that closes it is still TP2001. Inferring arbitrary `@contextmanager`/`@asynccontextmanager` generator implementations is deferred.

## Loops and bounded analysis

For supported `while` and `for`, evaluate condition/iterator effects, include zero iterations when feasible, route break/continue and preserve loop-else behavior. Finite tuples of resolved names and scalar literals are unrolled exactly up to 16 items, using values captured before the first body iteration; unresolved names and larger static tuples retain incomplete coverage. Other iterator operations keep exceptional/effect boundaries. Compute a monotone fixed point over aliases, lifecycle and duty states; widening may lose precision but cannot turn pending/unknown into discharged. Nontermination alone is not a function-exit leak witness.

Allocation identity must distinguish repeated outstanding resources. Use bounded recent-instance identities plus a summary carrying conservative outstanding multiplicity; closing the most recent object cannot discharge earlier instances merged at the same allocation site. A different finite representation is acceptable only with equivalent counterexamples. Include acquire/close each iteration, overwrite without close, break before cleanup, cleanup on continue and cleanup after the loop.

The candidate retains the 0.2 flow limits and uses up to 16 loop fixed-point passes, exactly unrolls up to 16 entries for supported static tuples, and retains at most 128 pending coroutine records. These values are implementation defaults, not accepted release budgets. CL01/CL05/CL06 still need to measure and accept maximum loop iterations, exit partitions, live obligations, pending coroutines/captures and total work. On exhaustion, widen affected state/duties to unknown, add a reason and keep the session usable. No truncated graph may establish successful cleanup. Recursive-call components retain conservative bounded recovery; no new recursive fixed-point support is promised.

## Intratask async and CS01

Use separate coroutine creation, execution, suspension and completion facts. Calling `async def` evaluates arguments and retains captures but applies no body/close postcondition; direct `await` applies the resolved execution summary. Copying a coroutine alias does not create independent execution or cleanup. Repeat await/unknown awaitable behavior is a boundary unless modeled. At most 128 coroutine records are retained per flow state; budget exhaustion weakens captured resources and records an incomplete-coverage reason.

At each modeled suspension include normal continuation, raise and cancellation, including during `__aenter__`, `__aexit__` and awaited close. Entry failure/cancellation cannot assume the body ran or exit was installed. Preserve cancellation through cleanup unless a reviewed handler/model changes it. The [asyncio reference](https://docs.python.org/3.13/library/asyncio-task.html#task-cancellation) is the starting point; qualify exact Python versions rather than importing all asyncio behavior into this subset.

Cancellation before execution is a separate case: the body has not acquired its locals, body finally blocks have not run, and captured caller duties remain unresolved. Cancellation during use may enter finally; cancellation during awaited cleanup must retain possible interruption or unknown effects. An initial cancellation request is neither task completion nor proof that subsequent cleanup succeeds. Repeated cancellation cannot be silently excluded from a claimed all-exit result.

At suspension, preserve facts justified by local ownership/effects and weaken facts invalidated by escaped aliases or unknown concurrent actors. Pending coroutine captures are visible even when never awaited or returned outside the resource scope. Closing a captured resource does not complete the coroutine or validate future use. V03 requires incomplete coverage for these escapes; general borrow-lifetime diagnostics wait for CS03.

TaskGroup/create_task, detached tasks, gather/shield/timeouts, futures, thread offload and process submission remain explicit boundaries. They cannot prove join, transfer, cleanup or isolation. Keep analyzer request cancellation separate from target-program cancellation; a cancelled analysis request publishes no stale result.

## Models and query inputs

Resource close-effect records use a stable ID and exact qualified resource/method identity, source revision/URL, trust, reviewer and exact Python-version bounds. They retain platform assumptions, argument/result alias facts, duty handoff, suppression, valid/invalid/unknown conformance cases, and separate normal, exceptional and cancelled lifecycle postconditions, async status, repeat-close policy and a named post-close operation allowlist. The close-effect record binds its outcome to the receiver identity; it requires no argument/result alias, duty transfer or manager suppression. Overlapping records for the same symbol/method/version are rejected; missing or unsupported versions do not match. These TypePact-owned records enter the incremental checker as a tracked input and are scoped to each file's resolved resource symbols, so a model edit refreshes affected flow queries while unrelated file checks can be reused. The close-effect record does not add public annotation syntax. Context-manager models separately define wrapper/result aliases, duty handoff and suppression. Keep supported-version and trust claims tied to conformance evidence. Do not import installed targets to discover model versions or execute model hooks; unresolved host-supplied version inputs remain a boundary.

Model content, selection/version inputs, target Python, root/module map and budgets are tracked inputs. Separate semantic effects from origin locations. Body checks depend on applicable exceptional/cancellation/cleanup/return summaries, not only normal lifecycle state. Location edits must refresh related spans even when semantic summaries compare equal. Fresh/reused results agree on duties, aliases, findings, certainty, provenance, coverage and revisions; unrelated checks reuse work, proven by query events.

## Diagnostics and compatibility

Proposed TP1002 reports an outstanding cleanup duty in the existing lifecycle family; TP3xxx remains reserved for must-use/variants. Primary location is the actual exit or escaping operation; related locations include acquisition/owning annotation, conditional cleanup and model origin as applicable. Include completion kind, certainty, trust, coverage reason and repair explanation in structured results as well as text. CL01 must freeze codes and a versioned checking schema before snapshots; preserve inspection schema/assurance.

Group by rule, duty acquisition context, source exit and completion kind. Separate explicit returns/raises retain separate findings; paths reaching the same source exit and outcome group join certainty. Bound related origins deterministically. Implicit exceptional exits are anchored to their originating operation; finalizer replacement updates the actual exit without erasing the earlier origin. Aliases, callee checks and call-site instantiations must not multiply the same witness. V03-06/09/25 must freeze this policy with exact counts.

Definite means a pending duty on every feasible state within the reported modeled exit group. Possible means known pending and discharged/absent alternatives. Unknown effects yield a boundary and unresolved duty, with independent known leak witnesses retained without overstating certainty. This is a static contract finding, not proof of a physical runtime leak.

Retain check exits: 1 for definite violations/declaration errors, 0 for no definite violation (including visible possible/boundary-only reports), and 2 for command/input failure, request cancellation or unrecoverable failure. Bounded incomplete analysis remains a visible boundary. New default cleanup diagnostics change CI behavior and require release/migration notes; strict policies stay in 0.4. Keep text/JSON/one-shot/session results aligned and retain lightweight helper typing/runtime identity.

## Decision record required before acceptance

All checkpoints in the [phase plan](../0.3-cleanup.md#decision-checkpoints) remain pending. Record maintainer/date, accepted option and alternative, V03 evidence, limitation and reconsideration trigger for duty origins/delegation, transfer commit timing, exception matching, model versions/trust, loop multiplicity/widening, async entry/cancellation, diagnostics/schema/grouping and numerical budgets. The maintainer may implement, validate and review these decisions and models alone under the [maintainer authority policy](../../release-gates.md#maintainer-authority); an additional reviewer is optional. A proposed default is not a release decision or model support claim.

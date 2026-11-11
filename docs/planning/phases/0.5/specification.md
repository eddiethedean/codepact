# 0.5 ownership specification

Status **Implemented in the 0.5.0 development candidate**. Maintainer acceptance and publication qualification are pending. [Phase plan](../0.5-ownership.md) · [Validation matrix](validation.md)

## Opt-in and syntax

Activation is `experimental-ownership = true` under `[tool.codepact]`, with `--experimental-ownership` and `--no-experimental-ownership` overrides for `check` and `baseline update`. The Rust check and session APIs accept the equivalent `Ownership` option. Default checking retains 0.4 behavior, including close-through-borrow and owned-return effects. Strict mode is a policy over the selected semantic scope. Enabling it alone leaves experimental ownership off. Inspection continues to expose declared facts.

Reuse `Owned[T]` and `Borrowed[T]` and their equivalent `Annotated[T, OwnedAccess()]` and `Annotated[T, SharedBorrow()]` spellings with either resource declaration interface. Use `from codepact import move` and `move(value: T) -> T`, a typed identity helper that returns the same Python object. Resolve calls by declaration identity, including supported aliases, module qualification and acyclic re-exports. A local lookalike or rebound name follows ordinary call analysis. Move recognition and enforcement belong to the checker.

Initially check modeled resources in function bodies with tracked local names, supported assignments, ordinary control-flow edges, and resolved calls and returns. Accept `dst = move(src)`, `take(move(src))`, a supported keyword equivalent, and `return move(src)` from an `Owned` return. A uniquely held, open local acquisition may still be returned directly under the existing `Owned` return contract. Starred unpacking, attribute or subscript destinations, unsupported descriptors, and unresolved signature or import binding are boundaries when relevant to a tracked resource. An exact move whose destination is unsupported still has its already-established evaluation effects.

## Transfer, alias and duty rules

Track a local binding's validity and ownership role, the referenced resource instance and lifecycle, the cleanup duty and its holder, and outstanding borrow or escape facts. A supported transfer requires a valid owning binding, one known open instance, and sufficient evidence that there are no conflicting aliases, captures, or active cleanup frames. Distinguish a definite conflicting alias from unresolved aliasing. Retain the original acquisition span and the transfer chain with the duty.

| Situation | Required modeled result |
| --- | --- |
| `dst = src` | Both names alias the same instance. The original owning binding keeps its role. Normal alias-based access and cleanup keep their existing semantics, subject to borrow restrictions. |
| `dst = move(src)` with a supported destination | The source binding becomes moved when the helper evaluates. The destination receives the same instance and duty. The resource stays open. |
| An operation reads a moved binding, including a second move, a close, or assignment from it | Report reuse with the move site. Rebinding the name to a fresh resource creates a valid new binding. |
| A known alias, active borrow, cleanup frame or earlier evaluated argument retains the instance | Reject the conflicting transfer and leave the source's ownership in place. An unsupported escape leaves transfer proof incomplete even after the visible aliases are rebound. |
| A branch moves on only some paths | Keep valid and moved alternatives at the join. Report possible reuse on a known mixed path. Widen unresolved facts to unknown with a boundary. |
| A valid move is evaluated for a resolved synchronous `Owned` parameter | The cleanup duty is carried by the evaluated argument until the callee accepts the binding at entry. The callee owns the duty across return, raise and further supported handoffs. A bare tracked argument at that parameter is a missing-transfer finding. |
| A call fails before its move argument is evaluated | The source remains valid and retains its duty. |
| A later argument raises, or signature binding fails after the move | The source stays moved. A known abandoned in-flight duty remains outstanding on the failure edge. No callee effects apply. |
| The callee raises after entry | The source stays moved. Use the callee's exceptional effects to determine whether the duty was discharged, still pending or unresolved. |
| A moved value is discarded or sent to a resolved non-owning parameter | The source stays moved. A known unaccepted duty remains outstanding. A checked borrow effect does not accept the duty. |
| The destination or receiver effect is unresolved | Preserve an already-committed move. Keep the affected duty unresolved, retain relevant known violations, and report incomplete coverage. |

Preserve evaluation order and the identity of each evaluated argument through argument binding. For example, a supported call that reads `src` after evaluating `move(src)` sees a moved binding. A call that first captures `src` for another parameter may conflict with a later move. Evaluate failure edges before entering a callee. Apply existing return and cleanup rules to the actual exit after `finally` and context unwinding, including a replaced return. [Python call semantics](https://docs.python.org/3.13/reference/expressions.html#calls) · [0.3 cleanup specification](../0.3/specification.md)

## Lexical shared borrows and escapes

A resolved synchronous `Borrowed` parameter grants access from callee entry through its modeled exits. Local aliases inherit the borrow and end within that region. The owner retains the cleanup duty. Checked bodies may read or call supported operations and pass the view into a verified nested shared borrow. Supported operations can have modeled effects.

Reject close, consumption, or a known escape through the borrowed view. Known escapes include returning the view, writing it into a field, global or container, and returning a closure that captures it. Unknown/native calls, unresolved closure captures and unsupported callbacks produce relevant coverage boundaries and weaken facts for the objects they may affect. Preserve unrelated facts and independent known findings. An unsupported boundary leaves effects unknown even if the declaration advertises a borrow. A known move or close that conflicts with a live borrow remains a violation.

## Interprocedural checking and budgets

Build effects from checked source or a selected reviewed model, using exact symbol and signature identity. An annotation supplies the proposed precondition. Apply verified effects only to matching actual bindings, including argument alias relationships. Preserve per-parameter access, capture and escape, resource and result aliasing, transfer commit points and normal, exceptional and cancelled cleanup outcomes. Track declared, checked-source and reviewed-model provenance with source locations and model revision where applicable. A borrowed body that violates its contract cannot establish a verified borrow for callers.

Start with resolved calls in the existing local and acyclic module graph. Within a resolved recursive component, iterate finite effect sets in deterministic order from unreachable exits, incorporating feasible base exits and propagating alternatives across recursive edges. The domain must distinguish an undiscovered exit from a verified effect. Recursive call edges with unresolved targets, effects that have not converged, and components with no usable exit summary retain visible incomplete coverage. Widen to unknown for the affected effects when budgets are exhausted. Never widen to valid, unique, completed, or discharged. The fixed-point limit is 16 passes, charged to the existing per-file share of the flow-work budget. The checker limits tracked ownership handles to 4,096, simultaneous task groups to 32, and retained coroutine records and scheduled captures to 128. A safe cutoff keeps subsequent requests usable.

Track semantic inputs for source effects, model selection and versions, target, budgets and the ownership setting. Preserve location evidence as a dependency so spans refresh when a semantically equal body moves. Reuse existing per-file dependency boundaries where sufficient. Compare fresh and reused reports and record query events for dependent refresh and unrelated reuse. [Analyzer query specification](../../analyzer-migration.md)

## Structured tasks and CS03

The candidate model starts with exact `asyncio.TaskGroup` identity, `async with TaskGroup() as group`, direct `group.create_task(worker(...))` calls to resolved coroutine functions, and bounded group/task/coroutine identities. Support nesting and multiple borrowers. Start with the default CPython event loop and non-eager task factory. Require the explicit project setting `assume-default-task-factory = true` or Rust option `Ownership::AssumeDefaultTaskFactory`. The reported model metadata identifies the assumption and CPython source pin. A known custom or eager factory or an unresolved configuration without a declared assumption remains a boundary. Direct task joins, task-result recovery, detached tasks, `gather`, `shield`, custom group subclasses, escaping task/group handles, dynamic registries, and thread/process handoffs lack completion or transfer guarantees in this slice. A resolved `task.cancel()` on a tracked local task handle records a request while the borrow remains outstanding.

| Event | Required modeled result |
| --- | --- |
| Create a coroutine with a tracked borrow | Capture the view at argument evaluation. Body execution has yet to begin. Pending coroutine captures retain a borrow even before scheduling. |
| Schedule a qualified child | Associate captures with the modeled group and task. Keep each borrow outstanding until proven completion. Closing the owner during a known live borrow is a contract finding. |
| Normal group exit with all children completed | End the corresponding borrows and allow owner cleanup. An inner group exit releases only the borrowers it establishes as completed. |
| Exceptional or cancelled group exit | Track sibling cancellation and completion separately. Release borrows only on edges where the model establishes completion, including exceptional edges that propagate after waiting. If completion is unknown, retain the affected outstanding facts and coverage reason. |
| `group.create_task(worker(move(src)))` | The move commits at evaluation. Until worker entry the coroutine carries the duty. At worker entry the resolved `Owned` parameter assumes it. Track cleanup separately from task completion on each edge. |
| Scheduling is rejected or cancellation prevents worker entry | Retain the committed move. Account for the abandoned or unresolved duty even if a version closes the unstarted coroutine. Worker cleanup applies only after entry. |
| A task, coroutine, group or borrowed capture escapes | Report the known invalid borrowed escape or the unresolved boundary, as applicable. A local group exit cannot discharge unrelated or escaped borrows. |

Direct `await` follows the existing intratask coroutine model with the same ownership commit rule. A worker may acquire and transfer a resource after entry, so cancellation before entry acquires no duty for that resource. An outer-to-worker move is covered edge by edge. An all-exit cleanup claim requires evidence for submission failure and cancellation before start as well as worker exits. The initial syntax has no implicit rollback or recovery operation.

The model uses CPython pins v3.11.15, v3.12.13 and v3.13.16 for their corresponding source targets. Python 3.13 added closing the coroutine on inactive-group submission and changed handling of simultaneous cancellation. Qualify those version differences before applying the associated facts. Run conformance on controlled fixtures and keep project analysis source-only. [Python TaskGroup reference](https://docs.python.org/3.13/library/asyncio-task.html#task-groups) · [CPython 3.13.0 TaskGroup source](https://github.com/python/cpython/blob/v3.13.0/Lib/asyncio/taskgroups.py) · [CS03 shared design](../../concurrency.md)

## Reporting and adoption

The rule assignments are TP2002 for moved-handle reuse, TP2003 for a conflicting transfer, TP2004 for consuming or escaping a borrow, TP2005 for a missing explicit move, and TP2006 for closing with a known outstanding task borrow. Retain TP2001 for close through a borrowed view, TP1002 for known outstanding cleanup, and the existing incomplete-coverage mechanism for unresolved effects. A finding is definite when its modeled alternatives all violate the relevant contract, possible for a known mixture, and a coverage boundary when evidence is insufficient. Preserve separate known violations when uncertainty affects other paths.

Anchor findings at the operation or actual exit and include relevant acquisition, transfer, alias, capture, completion and model locations. Text, concise and JSON should agree on certainty, trust, rule selection and coverage. Record the opt-in as semantic scope in effective settings, snapshots, capabilities and baseline matching. A baseline generated under the prior semantic scope cannot hide an ownership finding. Rule filters, baselines and suppressions follow 0.4 adoption policy, including visible coverage and strict failure for unresolved relevant boundaries. Configuration schema 1 and report schema 2 accept the expanded scope. Readers must accept the new rule tier and assurance value to consume opt-in results. [0.4 reporting specification](../0.4/specification.md)

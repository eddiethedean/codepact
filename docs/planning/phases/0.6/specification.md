# 0.6 advanced contracts specification

Status **Preview implementation complete; exact-candidate distribution requalification and maintainer acceptance remain pending**. This file remains the release acceptance specification. Local evidence now includes the advanced synthetic preview corpus, integrated TaskGroup/coroutine budget and Salsa event controls, and the budget stress benchmark. A prior cross-platform workflow run applies to an earlier candidate; the [candidate release record](../../../releases/0.6.0-evidence.md) tracks qualification for the frozen candidate. See also the [phase plan](../0.6-advanced-contracts.md) and [V06 matrix](validation.md).

### Preview implementation choices

- Advanced checks use `experimental-advanced-contracts`, layered on
  `experimental-ownership`. The setting is available in project config, check
  and baseline CLI overrides, and the Rust `Ownership` API. The preview adds
  TP2101 and TP2102 while preserving the default and ownership-only scopes.
- The Python declarations are `@state_machine(states=..., initial=...)`,
  `@transition(from_state=..., to_state=..., on_raise=..., on_cancel=...)`,
  `State[T, Literal[...]]`, and
  `Annotated[T, StateAccess[Literal[...]]]`. State names are literal Python
  non-keyword Python identifiers; a machine is capped at 32 states.
- Transitions are **declared-unverified** assertions. The analyzer validates
  identity and state names, seeds successful local construction, and applies
  declared outcomes. It does not prove that method bodies implement those
  transitions. Omitted exceptional or cancelled outcomes become unknown.
- The first executor catalog entry is the source-pinned
  `ThreadPoolExecutor` model for CPython 3.13.16. Controlled runtime
  conformance probes and imported source hashes are recorded in
  [pinned model evidence](evidence/models-2026-11-14.md). The existing
  TaskGroup source pins and explicit default-task-factory assumption remain in
  force. The shutdown-state regression now reports the known rejected submit
  without adding an unrelated unknown worker-effect boundary.
- The preview reuses the existing flow and Salsa query infrastructure,
  100,000 per-file flow-step budget, 256 exit partitions, 4,096 tracked object
  limit, 32 protocol states, and 32 local handles each for TaskGroups and
  executors. TaskGroup scheduling retains up to 128 distinct coroutine records;
  the 129th creation is an incomplete-coverage boundary. There is no second
  scheduled-task counter because scheduled entries refer to those same
  retained coroutine records. The integrated budget test confirms 128
  scheduled children complete, the next child reports the coroutine boundary,
  and an edit back under the limit recovers fresh/reused parity. See
  [task/query evidence](evidence/task-query-events.json) and the
  [budget stress measurements](performance.md#taskgroup-coroutine-budget).

## Activation and declarations

The preview implements a separate `experimental-advanced-contracts` opt-in on top of experimental ownership, through project config, check and baseline CLI overrides, and the Rust `Ownership` API. An explicit advanced opt-in with ownership disabled is a configuration error. Defaults and ownership-only checks retain their 0.5 semantic scope. Strict mode applies the existing policy to the selected scope. V06-01 now compares canonical diagnostics, contracts, coverage and model metadata between the Rust API and a config-driven CLI run; it also checks CLI override precedence and rejects contradictory configuration. The candidate acceptance record tracks final report review.

The preview reuses existing `BorrowedMut[T]` and `Annotated[T, ExclusiveBorrow()]` metadata. It resolves these and new declarations by exact symbol identity through supported imports, aliases and direct re-exports. Dynamic metadata, unresolved re-exports, lookalikes and rebound names follow the existing resolution rules. Python helpers preserve ordinary typing, object identity and signatures, and remain independent of the native analyzer.

The preview implements `@state_machine(states=..., initial=...)`,
`@transition(from_state=..., to_state=..., on_raise=..., on_cancel=...)`,
`State[T, Literal[...]]`, and the equivalent
`Annotated[T, StateAccess[Literal[...]]]` form. Both operation forms carry the
same receiver precondition and per-exit receiver state. A receiver annotation
by itself declares an entry requirement and supplies no transition. The
spellings are recorded as preview choices; paired checker, typing and runtime
fixtures are retained in V06-02 and V06-15 for final gate review.

| Canonical input | Initial contract |
| --- | --- |
| State machine | Resolved class identity, nonempty finite set of literal names and one declared initial state. Successful construction of a resolved local instance establishes its initial state only when the constructor contract is trusted or verified. |
| Operation | Resolved method and receiver, allowed input-state set, required access and an outcome record for each established exit. Initial user-defined transitions affect only the receiver. |
| Exit record | Explicit preserve, transition to one named state, or unknown. Nonexistent exits require a checked or reviewed summary. Omitted postconditions on feasible exits are unknown unless the summary independently establishes preservation. |
| Effect record | Per-parameter mutation, consumption and escape, plus known result aliases, cleanup duties and normal, exceptional and cancelled completion. Track unknown operations and unknown exception behavior explicitly. |

Identical metadata normalizes once. Reject undeclared states, invalid literal sets, incompatible access and contradictory declarations for the same operation and outcome before flow uses the affected contract. A user-defined protocol state belongs to the tracked object identity, so aliases observe the same transition. Lifecycle, ownership and protocol state remain separate facts. For example, choosing the protocol name `closed` supplies no resource cleanup effect.

## Exclusive access and suspension

The initial checked region binds one tracked object to a `BorrowedMut` parameter of a resolved synchronous call. Evaluate arguments and bind the signature before opening the region. Preserve ordinary evaluation effects and 0.5 move commit points on argument and binding failures. Check all actual bindings together. If two parameters alias the same object and one requires exclusivity, report overlap even when the callee happens to leave the other parameter unused.

| Situation | Required result |
| --- | --- |
| Two sequential exclusive calls or calls on distinct objects | Accept when each region meets its own entry requirements. End the region on the actual modeled exit after handlers and cleanup, including a propagated raise. |
| An existing shared or exclusive region or a known active task/coroutine capture overlaps the new exclusive region | Report a conflict where the overlap is established. An idle local owner alias alone is compatible when its use is confined outside the region. |
| The body accesses the object through a local alias of the exclusive view | Preserve the region identity. Apply supported receiver effects through that view. Independent reads, writes, closes or moves through a competing alias conflict with the region. |
| The view is closed, consumed or escapes by return or known storage/capture | Apply the existing borrow restrictions and retain ownership and cleanup duty. Initial support for nested access covers modeled operations through the current view. New nested borrow regions over that view remain boundaries. |
| Aliases, callbacks or effects are unresolved | Report the relevant boundary and weaken affected exclusivity, protocol and ownership facts. Preserve unrelated facts and independently established violations. |

Check `await`, implicit async context entry/exit, and `yield` when a relevant exclusive view is live. The initial proposal reports such suspension as unsupported coverage. An alleged region reaching a suspension via an async or generator parameter is surfaced at the tracked capture or suspension site. Creating the coroutine or generator does not apply body effects. Retained unresolved captures remain conservative; a suspension or a cancellation request supplies no release or completion proof. Async calls with no relevant live exclusive view continue under the inherited 0.3 and 0.5 rules. Unsupported generator boundaries do not require modeling iteration or finalization.

## Calls, joins and completion

Check a call against all feasible known input states. For an operation requiring `Ready`, `{Ready}` satisfies it, `{Empty}` gives a definite violation, and `{Empty, Ready}` gives a possible violation. An unknown state yields incomplete coverage. Retain an independently known violating path as a possible finding when other feasible paths are unresolved. Unreachable paths contribute no state, and unknown paths cannot turn a finding into a definite one.

Apply a preconditioned postcondition only on alternatives that satisfy its entry contract. Where a named model establishes the outcome of a rejected operation, use that failure effect. Otherwise a violated or unproved precondition leaves the affected call outcome unknown and checking continues with the remaining obligations.

Apply effects to the bound objects on each feasible completion path. For example, a verified operation requiring `Empty` can set `Ready` on return and preserve `Empty` on a particular raise. A raise before callee entry keeps the state established by argument evaluation. A raise after entry with unresolved receiver effects leaves state unknown. Apply the observed or modeled exit after finally and context unwinding. Cancellation gets its own effect record, including interrupted cleanup. Restore neither a consumed handle nor a discharged duty from a protocol state name or a different completion edge.

A declaration supplies a precondition and a proposed postcondition. Check wrapper bodies against tracked source effects and reviewed primitives where feasible. An arbitrary field assignment cannot prove a named state transition. For primitive user declarations, AC01 must explicitly accept a trust policy before they can establish new states. The proposed option exposes accepted primitive transitions as trusted declarations, keeps unsupported body effects unknown, and distinguishes them in reports from checked source and reviewed models. A contradicted declaration never serves as checked evidence for callers.

Checked source and reviewed models combine effects through actual bound aliases. An empty observed effect set for unresolved code gives no purity, no-raise, non-escape or cleanup guarantee. Weaken only the reachable objects and effect dimensions the unresolved operation can affect. Keep unsupported dynamic dispatch, attributes, containers, decorators and task handoffs visible where relevant.

## Named protocol models and CS04

### TaskGroup

Start from the released CPython pins and the 0.5 default-task-factory assumptions. Add local binding of exact `asyncio.TaskGroup()` and a resolved `async with group` form alongside the inherited inline form. Keep group protocol state separate from task completion, capture lifetimes and resource duties. No support for scheduling during shutdown from a child or escaped group handle is proposed.

| Operation or event | Proposed state and effects |
| --- | --- |
| Successful construction and entry | Construction establishes `New`. Successful first entry establishes `Active`. Unsupported entry failure yields an unknown group outcome unless the selected model identifies the failure. |
| Direct `create_task(worker(...))` | An active group can schedule under the 0.5 assumptions. A group is inactive for submission before entry and after a completed exit. The checked inactive call raises and applies only that pin's failed-submission effects, including whether the coroutine is closed. Re-entry into an already entered group is invalid. |
| Child failure or cancellation during exit | Model aborting as a non-submittable state where established. Track cancellation requests separately from joins. An active body after unresolved concurrent failure may have an unknown submit outcome. |
| Group exit | A model-proven completion edge establishes `Finished`, with its matching normal, raise or cancellation outcome. Release only captures whose completion was established. An unresolved exit retains outstanding or unknown captures and duties. |

Exercise the inherited version differences and exact defaults using controlled fixtures for every claimed pin. Use the pinned model metadata for provenance. [0.5 structured-task specification](../0.5/specification.md#structured-tasks-and-cs03) · [CPython 3.13.16 TaskGroup source](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/taskgroups.py)

### Local pool

Propose exact `concurrent.futures.ThreadPoolExecutor` with CPython v3.13.16 as the first conformance target. Start with a local constructor using no custom initializer, direct `submit` of a resolved callable with no tracked captures or resource arguments, and direct `shutdown` with default or literal supported arguments. Check local protocol state. Escaped executor handles, dynamic configuration, future operations, worker-side effects and thread/process resource handoffs remain coverage boundaries.

| Operation or event | Proposed state and effects |
| --- | --- |
| Construction | Normal return establishes `Accepting`. Constructor raise establishes no instance. |
| Submit | A call in `Accepting` can return normally and preserves the state in the qualified local subset. A call in `Shutdown` violates the precondition and raises. A failed submit can enqueue before worker startup fails, so it supplies no proof that work was absent, completed or reclaimed. Apply a verified state effect for the specific failure or use unknown. |
| Shutdown | Normal return establishes `Shutdown` with either supported wait setting. Repeated shutdown is accepted for the qualified source pin if conformance confirms it. An exception after the shutdown flag is set preserves `Shutdown` where modeled. Other unresolved failures leave state unknown. |
| Worker completion and cleanup | Keep these separate from pool state. `wait=False` proves no completion. Even a successful waiting shutdown establishes no resource cleanup or worker-effect summary. |

Capture source pin, interpreter/platform assumptions and known and unknown cases with the model. Do not generalize the result to other executors or interpreter versions. [CPython 3.13.16 executor source](https://github.com/python/cpython/blob/v3.13.16/Lib/concurrent/futures/thread.py)

## Bounds, incremental checking and reporting

Extend the existing Ruff declaration index, flow and Salsa queries. Track canonical machine/effect definitions, model IDs and revisions, target and relevant assumptions, analysis settings and limits, and provenance/spans. Reuse existing dependency granularity where it refreshes dependent semantic facts and related locations while preserving unrelated work. [Shared architecture](../../architecture.md) · [Query specification](../../analyzer-migration.md)

Use finite state sets and finite per-outcome effects. Propose reusing the existing 16-pass limit per recursive fixed point and per-file flow work accounting. AC01 sets caps for state names, outcome partitions and region tracking. Distinguish absent exits during iteration from established unreachable exits and unknown effects. On unresolved recursion or exhaustion of a flow, state, or capture budget, name the boundary and widen affected facts. Preserve independently established violations and cleanup obligations. Test recovery after analyzer request cancellation independently of source cancellation.

Use the existing advisory/strict exit policy. A known conflict or precondition violation is a finding subject to the selected rule policy. Unsupported suspension or unresolved effects alone permit an advisory check to pass and cause strict failure. Rule filtering, baselines, suppression and detail caps leave coverage intact. Diagnostics identify the operation and related region origin, conflicting access, transition, completion or model where known. Reports carry active scope, model provenance and certainty. Scope/model changes invalidate matching against stale baselines. Review JSON, text and concise parity and schema compatibility alongside Python helper introspection and checker support. [Reporting policy](../0.4/specification.md#policy-and-exit-calculation)

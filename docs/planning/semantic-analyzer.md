# Shared semantic analyzer

## Canonical model

A contract combines an ordinary Python type with independent dimensions:

~~~text
Contract {
  base_type, resource_kind,
  access: owned | shared | exclusive | unspecified,
  state_set, required_state,
  must_use,
  effects, provenance
}
~~~

A resource identity represents a tracked abstract object; a place identifies a local, parameter or supported field. Handles can be valid, moved, maybe-moved or unknown. Object lifecycle can be open, closed, a set of possible states or unknown. Closing updates the object state shared by known aliases; moving invalidates the transferred handle. These operations must remain distinct.

Effects include state transition, consumption, mutation, escape and possible exceptions. Summaries state parameter preconditions, normal and exceptional postconditions, returned alias relationships and borrow-region constraints. Each fact carries declared, inferred or trusted-external provenance.

## Pipeline

1. Parse source and stubs with the pinned Ruff AST adapter through a Salsa query, preserving spans and target-version diagnostics.
2. Index scopes, declarations and ordered binding events; resolve imports, captured aliases and supported decorators through tracked queries.
3. Lower both API surfaces into canonical contracts.
4. Reject incompatible declarations before flow checking.
5. Build CFGs including exception and cleanup edges.
6. Infer bounded function summaries, iterating recursive components.
7. Propagate abstract state and check obligations at uses and exits.
8. Render stable diagnostics and report coverage boundaries.

Equivalent frontend declarations must produce equivalent semantic fields after removing source-specific provenance.

## Transfer and join rules

| Event | Required transfer |
| --- | --- |
| Successful construction | Allocate abstract identity with declared initial state |
| Assignment | Add tracked alias; preserve ordinary Python behavior |
| Close on normal return | Set object state to terminal state for every known alias |
| Close on exception | Apply exceptional postcondition, otherwise unknown |
| Explicit move | Validate transfer; invalidate source handle and transfer obligations |
| Shared borrow | Require valid object; forbid incompatible exclusive access |
| Exclusive borrow | Require modeled exclusivity for entire region |
| Unknown call | Widen affected reachable objects and mark possible escape |
| Branch join | Union possible states and alias targets; preserve uncertainty |
| Function exit | Check resource and must-use obligations on every feasible modeled path |

A join of open and closed is possibly closed, never definitely open. A use with only closed incoming states is a definite violation; mixed incoming states produce a possible violation. Loops use fixed points with widening. Dead paths are removed only when supported conditions justify removal.

Basic alias analysis handles locals and a bounded set of fields. Containers, dynamic attributes and external calls require conservative summaries or boundary diagnostics. Avoid treating lack of discovered aliases as proof that none exist.

## Borrow regions and lifetimes

Start with lexical call-scoped borrows. A returned reference needs an explicit relationship to input regions before acceptance. Prevent borrow escape through storage or closures within the supported subset. Await and yield are suspension boundaries: exclusive borrows spanning them are unsupported initially. Research non-lexical lifetimes after lexical behavior has a stable specification.

## Diagnostics

Proposed families: TP1xxx lifecycle; TP2xxx ownership and borrows; TP3xxx must-use and variants; TP4xxx contract declarations; TP9xxx unknown or incomplete analysis.

A diagnostic includes primary span, related origin/transition spans, rule code, explanation, severity, certainty and repair guidance. Suggested edits require a verified semantic precondition; inserting close automatically may change behavior and should not be a default fix.

Default mode reports definite violations and summarizes unsupported boundaries. Strict mode also fails on possible violations and relevant unknown boundaries. Suppressions require rule codes and support reasons; they suppress reporting rather than creating proof.

Proposed CLI exit codes: 0 means no findings above the configured threshold; 1 means such findings exist; 2 means configuration or analysis failure. A zero exit code is not a universal safety guarantee.

## Declaration semantics and incremental queries

The [migration specification](analyzer-migration.md) defines the semantic index and Salsa query graph. Ruff supplies parsed structure through an adapter; CodePact owns declaration, binding and contract identities. Aliases capture resolved declarations and metadata at the appropriate evaluation point. Rebinding a name cannot rewrite earlier captured views. Deferred annotations and unsupported scope forms produce explicit unknowns until specified.

Tracked parsing/index/export/contract queries begin in 0.1. From 0.2 add CFG, summary and body-check queries, with callers depending on relevant callee summaries. Exceptions, effects, ownership and typestate extend those summaries in later phases. Query caching must preserve diagnostics, certainty, provenance and coverage. Separate source-location changes from semantic export changes and test both invalidation paths.

Use explicit cycle recovery: unresolved import cycles are bounded incomplete boundaries in 0.1; recursive summary inference requires a designed fixed point and widening later. Salsa tracks dependencies but does not define Python evaluation, aliasing, termination or soundness.

## Planned concurrent flow and summaries

Follow [CS01–CS07](concurrency.md). Add coroutine creation, capture, schedule, await/suspension, cancellation, completion/join and async cleanup events. Track task identities, resource identities, handle validity and outstanding borrows separately. Calling an async function does not apply body/close postconditions. A cancellation request does not discharge a task borrow, and completion does not discharge resource cleanup without a verified effect.

Summaries specify transfer commit points and normal, exceptional and cancellation outcomes; failed scheduling cannot silently restore a moved handle. Modeled group completion can end borrow regions only for all relevant work. At suspension or unresolved handoff, weaken facts affected by escaped/concurrent aliases without fabricating uniqueness, open state or completion. Reject or report unsupported exclusive regions spanning suspension according to mode. Task/alias graphs, recursive effects and joins use bounded domains with visible exhaustion.

Process/thread submission, queues, shared memory and custom serialization are conservative escape boundaries until qualified models distinguish local handles, receiver objects and shared backing resources. No schedule enumeration, lock proof or process isolation inference is part of this flow engine.

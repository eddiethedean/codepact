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

1. Parse source and stubs with spans.
2. Resolve scopes, symbols, imports, aliases and supported decorators.
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

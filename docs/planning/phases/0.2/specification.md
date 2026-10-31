# 0.2 lifecycle specification

Status: **Implemented in the unreleased source preview; release and LC01 acceptance remain pending.** This document defines the 0.2 acceptance scope and supported normal-flow model. The [phase plan](../0.2-lifecycle.md) and [validation matrix](validation.md) define the required review and evidence. Preserve the published 0.1 inspector while adding separate checking results and assurance.

## Executable acceptance example

```python
from typepact import resource
from typepact.rust import Borrowed

@resource(closed_by="close")
class Connection:
    def status(self) -> str:
        return "ready"

    def close(self) -> None:
        pass

def inspect(conn: Borrowed[Connection]) -> str:
    return conn.status()

conn = Connection()
inspect(conn)
conn.close()
inspect(conn)
```

The first inspect call succeeds. The last produces exactly one definite TP1001 with `conn.close()` as a related location. Do not duplicate the finding at both the argument and the callee's symbolic parameter use. The no-op close is a declared model: successful return is modeled as closed because of `closed_by`, not because its body proves external cleanup.

The explicit equivalent uses an undecorated Connection, `ConnectionResource = Annotated[Connection, Resource(closed_by="close")]`, construction through that captured resource view, and `Annotated[ConnectionResource, SharedBorrow()]` at the parameter. Test decorator/explicit lifecycle crossed with shorthand/verbose shared access. Compare shared semantic dimensions while preserving source provenance. Alias metadata must not attach a lifecycle to every reference to the undecorated base class.

## Supported-pattern boundary

| Pattern | 0.2 checking behavior |
| --- | --- |
| Recognized lifecycle and access declarations | Reuse 0.1 canonical resolution; reject conflicts before flow checking |
| Direct construction of resolved resource class/view | Fresh abstract object, open on successful return; custom construction effects need a supported summary or boundary |
| Local assignment and reassignment | Preserve Python aliasing; detach rebound places |
| If/else, ordinary return, straight-line calls | Source-order evaluation; join live normal paths; return terminates its path |
| Direct synchronous local/imported functions | Apply checked summaries and actual parameter alias relationships |
| Direct resource methods | Ordinary methods require open; infer supported body effects; designated close has declared normal transition |
| Local absolute imports and acyclic named re-exports | Resolve within root/module snapshot, preserving captured views |
| Loops, try/except/finally, with, async/await, yield | Mark relevant region incomplete; weaken affected resources before any modeled continuation |
| Resource fields/containers, globals/nonlocals, closures, returned resource aliases | Escape/alias boundary; no local alias-completeness claim |
| Dynamic dispatch, inheritance overrides, monkeypatching, star/relative/dynamic imports | Relevant boundary; no guessed callee, purity or resource identity |
| Recursive call component | Unknown summary and bounded incomplete coverage; no recursive safety claim |

Do not skip an unsupported region while retaining precise earlier facts. Weaken resources it can affect; when reachability is unresolved, weaken all tracked resources in that region. Report span and reason. Independent checked bodies can remain precise, but aggregate coverage retains every relevant boundary.

## Flow facts and transfers

Keep declaration identity, callable identity, abstract object and local place distinct. A place maps to possible objects plus a possible unbound/unknown target. Object state is open, closed, open-or-closed, or unknown. Unknown includes unmodeled effects and is not a proved finite state set. Retain state/alias origins for explanations.

| Operation | Transfer |
| --- | --- |
| Successful `x = Connection()` | New construction-context object, bound to x, open on normal return |
| `y = x` | Copy target set; exact aliases share object state |
| Rebind x | Replace x's targets; preserve old object state for remaining aliases |
| Close with exact singleton receiver | Set object to closed on normal return; preserve close location |
| Close with multiple possible targets | Weakly join closed into each target; do not mark all definitely closed |
| Branch join | Union targets/states of reachable normal predecessors; preserve possible unbound/unknown |
| Known call | Check preconditions; instantiate effects on actual objects; apply normal effects only on normal continuation |
| Unknown call | Weaken reachable receiver/argument objects to unknown; record potential escape and boundary |
| Unknown call after prior escape | Also weaken objects reachable through escaped references, even when not passed again |
| Return | End path; exclude it from the join for following statements |

For a branch choosing either a or b into x, `x.close()` makes later uses of a and b possibly closed. If both alternatives refer to the same exact object, close can be strong. An unknown alternative prevents strong updates. Reassignment must not alter objects still referenced elsewhere.

When two callee parameters receive the same actual object, their effects share identity and execution order. Do not combine independent postconditions that erase a close/use violation. Use alias-aware summaries or report that call pattern as incomplete. Allocation-site abstraction must not merge distinct straight-line constructions into an exact singleton. Repeated/recursive allocation outside this model is a boundary.

## Summaries and Borrowed bodies

A summary contains callable identity, resource/access preconditions, ordered or alias-aware effects, normal postconditions, exceptional uncertainty, escape/return relationships, model dependencies and provenance. Supported ordinary method bodies infer non-consuming or closing effects. Missing contracts or unmodeled bodies do not imply purity.

Borrowed requires open state at entry and forbids consuming/closing the parameter through direct calls, aliases or resolved helpers. Check supported bodies once with symbolic parameter objects; instantiate preconditions at calls. Report forbidden close at the operation, with the Borrowed annotation as a related span. An invalid body cannot export a successful non-consuming summary; dependent callers retain invalid/incomplete status without repeating the body finding.

The designated close supplies a declared normal transition with declared-unverified provenance. It does not certify runtime cleanup or make other effects in that body pure. Check other supported effects or retain their boundaries. Repeat close without a repeat-close policy is a boundary, not a new definite double-close error. Ordinary methods require open; this phase adds no post-close permission API.

A Borrowed annotation on an unknown helper does not verify its implementation. Retain unknown effects and a boundary. Reviewed external model catalogs and version constraints belong in 0.3.

## Normal and exceptional outcomes

Open construction and closed close postconditions apply only after successful return. Ordinary calls may raise unless modeled otherwise. On exceptional close the receiver becomes unknown unless an explicit supported postcondition defines its state.

0.2 follows normal continuation without claiming all-path cleanup. An unhandled exception ends execution; it does not make the normal successor spuriously unknown. An applicable handler/finally creates an incomplete region pending 0.3. Never propagate normal closed state into a handler or invent an open exceptional state. Report this boundary when relevant to resource checking.

## Discovery and incremental inputs

Proposed CLI: `typepact check --root DIR [--python-version VERSION] FILE...`. Discover local dependencies of entry files; check supported bodies once and executable module statements in dependency order. Never import or execute Python. Arbitrary import-order behavior is outside this model.

The adapter records source, module map, missing modules, root/search precedence, target version, models and budgets as explicit session inputs. Sort traversal/output deterministically. Report duplicate logical modules and file/package collisions rather than silently choosing. Keep canonical paths within root, deduplicate symlink targets and bound files/bytes/import edges. Cycles yield bounded incomplete results.

Track CFG, summary and body checks separately from rendering. Summary semantic equality excludes offsets; location-only edits still refresh explanations through current origin mappings. Changes to effects/preconditions/trust invalidate callers. Coverage and recovery belong in query results, not just CLI messages.

Identify recursive components before summary evaluation. Initially return unknown effects plus a boundary for the component without entering uncontrolled Salsa cycles. Accepting recursive effects requires a separately reviewed finite domain, iteration budget, monotone fixed point and widening tests. Bound CFG size, objects/alias sets, call graph and summary work. Exhaustion weakens facts, exposes incompleteness and preserves session usability. Truncating an alias set must never leave an exact-object claim.

The source preview currently uses provisional flow defaults of 128 nested `if`
regions, 64 possible objects per local alias at a join, 4,096 tracked objects
per function or module body, and 8 nested call/effect evaluations. These values
are implementation limits for exercising bounded recovery, not accepted LC01
decisions. The [preliminary performance record](performance.md) maps each
default to its exhaustion fixture and current measurements. Accept the rationale
and measured impact in LC01 before marking budget qualification complete.

## Diagnostic and command contract

- TP1001: closed-resource use. All incoming states closed means definite; known open/closed mixture means possible. Unknown state produces a TP9xxx boundary; retain independently known violations without inventing certainty.
- Proposed TP2001: close-through-Borrowed. Keep TP4xxx declarations and TP9xxx unknown/incomplete families. Freeze exact codes under LC01.
- Include primary span, related transition/annotation spans, certainty, explanation and repair guidance. Choose a deterministic bounded set of related origins at joins. Deduplicate by rule/source operation/object context, preserving distinct use sites.
- Proposed default exits: 1 for definite violations or declaration errors; 0 when neither exists, including possible/boundary-only results; 2 for invalid command/configuration, input failure, cancellation or unrecoverable analysis failure. Ordinary bounded incomplete analysis remains a boundary. Show possible findings and incomplete coverage even on exit 0. Strict policy configuration remains 0.4 work.
- Checking assurance states the normal-flow subset, summary trust and incomplete coverage. A clean result does not establish runtime behavior, leak freedom or lifetime safety.

Preserve inspection schema, exits and inspection-only assurance. A separate structured checking API returns contracts, findings, coverage and revision metadata. Accept its public names/versioning in LC01 before implementation.

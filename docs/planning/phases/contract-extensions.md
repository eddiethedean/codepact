# Follow-on phases for CodePact contracts

Status **Proposed**. The phases below turn four catalog-inspired ideas into
implementation and qualification work. All acceptance gates are pending.
Select release versions at or after the 0.9 readiness decision. Prototypes
may start as their prerequisites become available. [Root roadmap](../../../ROADMAP.md) · [0.9 readiness plan](0.9-stabilization.md)

## Sequence

| Phase | Outcome | Technical entry point |
| --- | --- | --- |
| 1. Validated values | Check integer bounds at a resolved API boundary using evidence from the caller's flow | Existing declaration, flow, and reporting infrastructure |
| 2. Relationships between values | Preserve a declared ordering between two arguments across guards and calls | Accepted scalar validation semantics and reporting from Phase 1 |
| 3. Field guarantees | Check that entering a declared state establishes a field guarantee and that reads respect it | Qualified 0.6 typestate, alias, and effect semantics |
| 4. Dependency boundaries | Check declared import directions and re-exports against a project policy | Existing module graph, symbol identities, project settings, and reporting |

Start the value checks in the order shown. Field guarantees can proceed once
typestate is ready. Dependency boundaries can be prototyped independently of
all three flow extensions. Each release proposal inherits the applicable
[common gates](../release-gates.md) and records evidence for its exact candidate.

## Phase 1 — Validated values

**User outcome.** A reservation API can require a seat count between 1 and 8.
The checker accepts a supported guard or a checked validator that establishes
the range on the path to the call. It reports an out-of-range literal, a
reachable path that bypasses validation, or a change to the value that makes
the previous check insufficient. [Primitive Obsession](https://www.codesmells.org/smells/primitive-obsession)

**Implementation order.** First write the CLI oracles using identical ordinary
Python signatures. Then add inclusive integer bounds to the canonical contract
and track them for exact built-in integer literals and resolved local values.
Start with direct comparisons, branch joins, assignments, and one supported
literal-offset expression. Once caller checking is qualified, verify a resolved
validator's promise on every modeled normal exit. Keep raising paths separate.
Treat opaque validators, unresolved operand semantics, and exhausted budgets
as incomplete for the affected obligation.

- [ ] **VV01 — Call preconditions.** Pass literals at 1 and 8, reject 0 and 9, and accept a guard that proves both bounds on every path to the call. A modeled branch that admits a violating value keeps that witness at the join.
- [ ] **VV02 — Value identity.** A supported assignment preserves facts about the copied integer. Rebinding and arithmetic use the new value's facts. A `+ 1` that can produce 9 invalidates the old proof for a range of 1 through 8.
- [ ] **VV03 — Validator evidence.** Compare a source-verified validator, a validator that returns an invalid value on one path, and an opaque validator. Only a verified normal postcondition supplies a caller fact. Verify callable identity through an import alias and a rebinding counterexample.

Public annotation syntax follows the initial oracle. Reuse evidence from
ordinary typing when available and keep value provenance in the core analyzer.
General predicate verification remains research. [Type-informed branch plan](../typepact.md#tc02--use-branch-narrowing-without-losing-resource-state)

## Phase 2 — Relationships between values

**User outcome.** An export API can require `start <= end` for two integer
arguments. The checker reports a reversed pair and accepts independently
validated values only when the ordering itself is established.
[Data Clump](https://www.codesmells.org/smells/data-clump)

**Implementation order.** Extend the scalar analysis with a bounded relation
between two tracked local integer values. Start with one declared `<=`
relationship per resolved signature. Bind actual arguments before checking
the relationship. Derive evidence from literals or a supported comparison and
retain it at joins only when every reachable predecessor establishes it.
Rebinding either operand requires evidence for the new pair. Defer
parameter-object invariants, arithmetic relations, and arbitrary predicates.

- [ ] **VR01 — Pairing and calls.** Check ordered, equal, and reversed endpoints. Cover positional and keyword calls and one resolved cross-module forwarding call. Reject a known reversed pair even when both arguments satisfy their individual bounds.
- [ ] **VR02 — Correlation.** Accept a guard on the actual pair. Retain a known counterexample when one path bypasses the guard. Exercise reordered arguments and a reassignment after validation. An opaque operand or a dropped fact at a budget boundary leaves coverage incomplete.

## Phase 3 — Field guarantees tied to typestate

**User outcome.** A report promises a checksum in `Finalized`. The checker
verifies that every modeled successful transition into that state establishes
the checksum, and checks a subsequent read against the actual state and
field evidence. [Temporary Field](https://www.codesmells.org/smells/temporary-field) · [0.6 typestate plan](0.6-advanced-contracts.md)

**Implementation order.** Add one declared field guarantee to a finite state
on a resolved class. Start with direct field assignments and reads on a
tracked receiver. Distinguish an uninitialized field, a field set to `None`,
and one established as non-None. Verify normal and exceptional transition
effects separately. Reuse the existing alias and effect analysis to handle a
modeled write through an alias and to weaken facts across unresolved effects.
Dynamic attributes, properties, and inherited storage remain outside this slice.

- [ ] **FG01 — Transition guarantee.** Accept successful finalization that assigns the checksum on every normal exit. Report a transition that can return successfully with the checksum unset or `None`.
- [ ] **FG02 — Reads and failure.** Accept a read after a verified transition. Exercise a caught exception before initialization, an exception after a modeled write, and a known alias write that breaks the guarantee. Report an unresolved descriptor or alias effect as incomplete for the affected field.

## Phase 4 — Declared dependency boundaries

**User outcome.** A project can allow `core` to import an adapter interface
and forbid it from importing concrete adapters. A violating re-export
produces a finding with the importing site, the origin, and the route between
them. [Insider Trading](https://www.codesmells.org/smells/insider-trading) · [Base Class Depends on Subclass](https://www.codesmells.org/smells/base-class-depends-on-subclass)

**Implementation order.** Define opt-in package or module rules in project
configuration. Start with resolved absolute and relative imports between
selected project modules. Check direct imports and follow resolved re-exports
to the imported symbol's origin. Distinguish runtime imports from imports
under a resolved `typing.TYPE_CHECKING` guard. Let the declared policy
specify which kinds it governs. Use bounded graph traversal for re-exports
and cycles. Deferred work includes dynamic imports, per-member access rules,
and inferred architectural layers.

- [ ] **DB01 — Policy and attribution.** Accept an allowed interface import and the permitted adapter-to-core direction. Report a forbidden direct import, an alias of that import, and a forbidden origin reached through a re-export. Use an unrelated same-named symbol as a negative control.
- [ ] **DB02 — Scope and graph limits.** Cover runtime and type-only edges, a re-export cycle, a dynamic import and an excluded dependency. Report only relevant unresolved edges as incomplete. Invalid policy is a configuration error. Changing a rule or re-export refreshes dependent findings.

## Qualification across phases

Before selecting public syntax or policy keys, freeze a buggy example, its
source repair, and a boundary example for each claimed guarantee. Extend the
[catalog-inspired corpus](../../../crates/codepact/tests/projects/code_smells/README.md)
with expected rule, certainty, and source evidence. Keep the ordinary types
and analysis settings equal between bug and repair where practical. Report
definite and possible violations from supported paths, with incomplete
coverage for unresolved obligations. Reuse the CLI policy, schema, and
incremental infrastructure. Compare fresh and reused results after edits to
a contract, an applicable effect, and a source location. Record time and
memory near the agreed limits before proposing a release.

The related **Refused Bequest** feature stays on the TypePact behavioral
compatibility track after its selected type-informed stages. Its proposed
first slice compares resolved overrides and protocol implementations that
already have compatible Python signatures. Check for stronger preconditions,
weaker postconditions, and wider declared effects, such as an implementation
that consumes an argument the interface promises only to borrow. Use the
qualified ownership and typestate summaries when those promises apply.
[Refused Bequest](https://www.codesmells.org/smells/refused-bequest) · [TypePact plan](../typepact.md#type-informed-acceptance-gates)

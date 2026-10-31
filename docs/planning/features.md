# Feature specifications

Every feature requires a mixed-interface fixture, a valid case, an invalid case and an unsupported-boundary case before its support claim expands.

| Feature | Required behavior | Acceptance gate |
| --- | --- | --- |
| Resource lifecycle | Track open/closed state through locals and known aliases | Diagnose post-close use through Borrowed parameters; accept pre-close use |
| Cleanup obligations | Track must-close ownership to modeled exits | Detect leaks on return and exceptions; accept modeled finally/context cleanup |
| Must-use | Attach obligation to returned value | Diagnose discarded expression; define explicit discard and propagation |
| Owned transfer | Consume a valid tracked handle through move | Diagnose reuse, double move and known conflicting aliases |
| Shared borrow | Prohibit consume/close through borrow | Reject invalid callee bodies and caller preconditions |
| Exclusive borrow | Require a unique modeled access region | Reject overlap and alias mutation; surface unknown escapes |
| Typestate | Validate operations against declared state sets | Cover transition, join, invalid transition and exception outcome |
| Result / Option | Provide typed tagged runtime values | Runtime, typing and pattern-match fixtures agree |
| Exhaustiveness | Check supported finite variants | Missing variants diagnosed; guards do not count as unconditional coverage |
| Protocol / trait bridge | Reuse structural signatures where possible | No claim of Rust coherence or specialization without a separate design |
| Effect contracts | Describe mutation, escape and raises | Verify supported bodies; retain unknown effects at external boundaries |
| Structured async lifetimes | Track coroutine captures, task completion and resource owner/borrow regions | CS01–CS03: cleanup/cancellation paths, join-before-close and borrow escape fixtures |
| Suspension-aware access | Retain only justified facts across suspension and concurrent effects | CS04: overlap and await/yield boundaries are rejected or visibly unsupported |
| Concurrent protocol models | Check finite task-group/pool states with explicit effects | CS04/CS06: normal, failure and shutdown paths for named versioned models |
| Process/thread handoff boundaries | Distinguish escape from proven transfer; preserve unknowns | CS03/CS06: unresolved queues, executors, serialization and shared memory never establish isolation or cleanup |
| Lifetime relations | Relate returned borrows to inputs | Research gate; no general lifetime syntax before soundness scope is defined |

## Resource modeling

Require an explicit model for construction, close behavior, idempotency, allowed post-close operations, context-manager entry/exit, and exceptional outcomes. Models can live in source contracts or reviewed external summaries. Context managers do not universally imply resource closure; model the concrete class.

Cleanup checks must distinguish owning and borrowing functions. A borrowing function has no obligation to close the resource and may be prohibited from doing so. Returning ownership transfers cleanup obligations. Finalizers are not accepted as deterministic cleanup.

## Must-use semantics

The initial rule detects a bare discarded return expression. Assignment satisfies immediate handling but does not establish eventual use; tracking abandoned assigned values is a later dataflow rule. An explicit discard helper documents intentional abandonment. Decorating a Result-producing function and annotating its return MustUse must lower to the same obligation.

## Typestate and effects

Begin with finite named states and explicit method transitions. Define whether mutation occurs on normal return or before an exception. Generic state parameters describe the same state machine, not a second engine. Illegal or contradictory state declarations are errors.

Do not infer “no exceptions” or “pure” from an empty observed effect set when calls are unresolved. Effect inference is partial and must retain unknowns.

## Research boundary

Rust-style trait coherence, associated types, general higher-ranked lifetimes, Send/Sync analogues, lock proofs and whole-program ownership remain research. Each needs an RFC, counterexamples, compatibility evidence and an explicit analysis scope before entering a release milestone.

## Shared analyzer prerequisites

Every analyzer feature uses the [Ruff AST and Salsa architecture](analyzer-migration.md). Before expanding lifecycle or ownership claims, qualify declaration identities, captured aliases, uniform annotation conflict checks and explicit unknown boundaries in 0.1. Each later feature extends tracked semantic summaries and adds fresh/updated parity and dependency-invalidation cases. Parser support, query caching and ordinary typing compatibility do not by themselves establish contract enforcement.

## Concurrency model policy

[CS01–CS07](concurrency.md) define staged acceptance. Creating a coroutine does not establish body execution or consumption; requesting cancellation does not establish completion; joining a task does not by itself establish resource cleanup. Borrowed is not deep immutability; Owned is not serialization/send permission. Cancellation and exceptional cleanup require their own postconditions. Structured task support begins with reviewed TaskGroup models; general asynchronous ownership, process sharing, lock proofs and scheduler analysis remain outside the supported subset.

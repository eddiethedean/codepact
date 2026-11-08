# Static analysis and ownership limitations

## Assurance model

CodePact proposes checks over a documented subset of Python with declared and inferred contracts. A finding can establish a modeled violation. Absence of findings establishes only that configured checks found no violations within analyzed paths and assumptions.

Python execution retains its normal object model. Ownership annotations do not change reference counting, garbage collection, aliasing, memory layout or interpreter behavior. Rust's compiler ownership system governs a different execution model; CodePact must not advertise an equivalent memory-safety guarantee.

## Boundaries that must be visible

| Boundary | Consequence | Required behavior |
| --- | --- | --- |
| Any or missing stubs | Call and alias effects may be unknown | Propagate unknown facts and report coverage |
| Dynamic imports, eval, exec | Behavior cannot be resolved statically | Never execute; mark affected scope unsupported |
| Monkey patching and dynamic attributes | Resolved method identities can change | State closed-world assumptions or weaken results |
| Reflection, descriptors and metaclasses | Attribute access can execute arbitrary behavior | Use explicit models or conservative effects |
| Native extensions | Memory and lifetime behavior is external | Trust reviewed summaries only; no memory-safety claim |
| Containers, globals and closures | Hidden aliases can escape | Reject within strict ownership subset or mark unknown |
| Async tasks and suspension | Captures can outlive owners; interleaving can invalidate local facts | Qualify structured task models; preserve pending borrows until modeled completion; weaken affected facts |
| Threads and process handoffs | Aliases, serialization, inherited handles or shared memory cross the modeled region | Conservative escape/unknown boundary without reviewed models; no race, isolation or transfer guarantee |
| Exceptions and cancellation | Cleanup or transfer can be interrupted | Model exceptional edges and suspension explicitly |
| User contract assertions | Declarations may be false | Verify supported bodies; label trusted contracts |

## Ownership scope

Owned is an intended transfer and access contract. It is not proof that no Python reference exists elsewhere. Copying a reference does not duplicate a unique resource. Deep copy, serialization and custom cloning require separate models; they cannot be inferred as safe duplication.

Shared borrowing does not make the underlying object deeply immutable. Properties and methods can mutate internal state. Exclusive borrowing applies only to a modeled region and known alias relationships. The initial engine must reject or downgrade claims when access leaves that scope.

Lifetime checks concern permitted access to modeled resources, not raw pointer validity. A Python reference can remain alive after the underlying resource closes. A moved handle may still exist at runtime while its static contract forbids reuse.

## Reporting policy

Distinguish definite violation, possible violation, unknown boundary and trusted assumption. Show incomplete analysis in both human and machine-readable output. Strict mode may reject unknown boundaries, but that still does not prove arbitrary Python safe.

Optional runtime checks can enforce selected transitions on observed operations. They may miss bypasses and impose overhead. Runtime checking must have its own documented coverage and cannot retroactively justify stronger static claims.

## Risk reduction

Keep 0.1 limited to contract inspection with bounded declaration resolution; introduce intraprocedural flow checks and explicit summaries in 0.2. Expand scope with counterexample-driven fixtures, pilot feedback and documented assumptions. Treat unsound acceptance as a correctness bug; treat unsupported analysis as a visible product limitation.

## Frontend and query assurance

Parsing with Ruff does not imply support for every Python construct. Salsa reuse does not imply semantic correctness or convergence. Mark unsupported binding/evaluation forms, dynamic imports and exhausted budgets as incomplete. Preserve unknowns and provenance across cached results. Declaration identities captured by aliases and runtime resource instances are distinct. The [migration plan](analyzer-migration.md) defines the supported boundaries and their qualification requirements.

## Concurrency assurance

The [planned concurrency subset](concurrency.md) targets resource lifetime and protocol violations, not race freedom, deadlock freedom or thread safety. Current 0.2 analysis remains synchronous. A coroutine call does not execute its body; cancellation requested does not mean task completed; task completion does not establish owned-resource cleanup. Shared borrow is not immutability, and Owned does not establish process transferability. Unknown scheduling, detached captures, serialization, start methods and shared-memory effects must remain visible boundaries. Analyzer request cancellation is distinct from cancellation in the target program.

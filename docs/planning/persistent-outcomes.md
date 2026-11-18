# Persistent outcome acknowledgment

Specification revision **1**. This specifies the `MustHandle` contract released
in [CodePact 0.9.0](phases/0.9-obligations-and-change-review.md) under OH01–OH07.

## Initial declaration and compatibility boundary

`MustHandle` is an opt-in outcome-acknowledgment contract. The initial supported
declaration is a callable return annotation whose resolved outer outcome is
an exact CodePact `Result` or `Option` with ordinary payloads. A supported
published contract may express that same outward requirement. A transparent
`Annotated` alias is the Python spelling.

Existing code acquires the stronger requirement only through a new declaration
or applicable published contract. Constructing an unmarked `Ok`, `Err`, `Some`
or `Nothing`, assigning it, or returning it from an unmarked API does not
globally create `MustHandle` semantics. A known duty already acquired from a
marked producer cannot be erased by removing a later declaration.

Preserve existing `MustUse`, `@must_use` and immediate-use `discard` behavior.
Assignment may satisfy the existing immediate-use rule while a separately
declared `MustHandle` duty remains pending. Existing `discard` does not silently
become a persistent-outcome escape hatch.

Parameter, local-variable, field and nested-payload declarations are outside
the initial supported declaration positions. Relevant unsupported uses produce
an explicit unsupported-declaration or coverage finding. Ordinary helper
parameters do not implicitly transfer a duty. Custom outcome classes, database
row results and resource-bearing or nested outcomes do not gain this contract
from their names or shape. OH08/TC03, OH09 and OH10 require separate qualification.

## Production and obligation identity

A marked synchronous call creates a caller outcome duty only when it
successfully returns. Raising before return creates no caller outcome token.
For each successful marked return:

1. Forward an existing pending duty on the returned identity without duplication.
2. Otherwise create one outward consumer duty, including when returning a new,
   previously unmarked, acknowledged or intentionally discarded outcome.
3. Keep internal acknowledgment, discard and diagnostic suppression from
   exempting the consumer from the marked API's outward requirement.

Identity follows the production or API-delivery event, not a variable name or
value equality. Aliases share one duty; independent productions and qualified
renewals create distinct duties even when their values compare equal.
An outward handoff is tentative until `finally` completes; a replacement
return or exception follows the actual outgoing path.

## Acknowledgment and forwarding

The requirement is explicit acknowledgment of supported outcome alternatives,
a qualified outward handoff, or intentional discard with a recorded reason.
It does not establish business success, recovery, transaction settlement or
resource cleanup. Repeated acknowledgment is allowed.

| Operation | Required effect |
| --- | --- |
| Assignment, underscore binding, alias, logging, comparison, serialization or field read | No acknowledgment by itself; preserve the duty. |
| Selected exact variant or explicit variant OR arm | Acknowledge the selected alternative after pattern and guard succeed, before its body; a `pass` body still acknowledges it. |
| Wildcard with one qualified remaining variant | Acknowledge that single alternative and report the inference basis. |
| Wildcard spanning multiple alternatives | Preserve the pending alternatives. |
| False or raising guard | The unentered arm adds no acknowledgment; preserve effects already established by evaluating the guard. |
| Source-resolved helper | Apply exit-specific acknowledgment to the same identity only after required argument evaluation and call entry. |
| Unknown helper or carrier | Retain affected uncertainty and known identity; infer neither acknowledgment nor handoff. |
| Qualified marked return | Forward a pending duty once; apply the renewal rules when no pending duty remains. |

Acknowledgment may accumulate across several matches. Branch joins retain
conditional presence and per-variant pending state. Finite-match diagnostics
remain a separate rule from eventual acknowledgment.

A helper may return an alias of its caller's input without erasing the caller's
duty. A helper exporting a newly acquired pending result must declare its
outward `MustHandle` agreement. A plain return annotation on that newly acquired
result is an undeclared obligation escape; source-resolved analysis retains the
known duty and reports the missing outward contract.

## Loss, escapes and exceptional exits

Report abandonment when a modeled overwrite, deletion, discarded temporary or
scope exit removes the final tracked carrier without a valid terminal event.
Evaluate the right-hand side before replacing an old binding; if it raises,
the old binding and duty remain available on the exceptional path.

Unsupported field, global, container or wrapper storage retains a coverage or
escape boundary and known local aliases. It prevents a definite last-carrier
loss conclusion unless later supported evidence establishes that loss.
Supported later acknowledgment can establish its own effect while unrelated
uncertainty remains visible. Strict policy may fail on that uncertainty.

Pending duties follow modeled normal and exceptional exits. Acknowledgment in
a handler or `finally` applies to the actual path. Suppression changes a
finding's disposition, not the underlying obligation facts.

## Direct awaits and intentional discard

OH06 separately qualifies directly awaited producers and supported intratask
cancellation paths in 0.9. Coroutine creation creates no produced outcome duty
and does not execute an observer. A successful direct await can deliver the
outcome and its duty. Cancellation before execution cannot count as handling.
Task/future handoffs and deferred callbacks need their own AS/MP/BR models;
completion, exception observation, acknowledgment and cleanup remain separate.

The proposed `discard_outcome(value, reason=...)` evaluates its value once,
requires a nonempty statically reviewable reason in the initial supported form,
and records intentional abandonment separately from acknowledgment. Project
policy may reject or require review of the waiver. It does not close or move a
payload, join work or suppress cancellation. Forced termination, interpreter
shutdown and garbage collection do not establish handling.

## Required acceptance cases

Every case is required for the named bounded 0.9 scope, with the relevant
valid, invalid and unsupported controls retained in release evidence.

| Work | Required controls |
| --- | --- |
| OH01 compatibility | Unmarked constructions and returns create no new duty; existing `@must_use` assignments and immediate-use discard retain their behavior. Same-named non-CodePact types and SQL row results acquire no implicit duty. |
| OH01 declaration boundary | Supported exact `Result`/`Option` callable returns qualify; parameter, local, field, nested and resource-payload declarations retain explicit unsupported boundaries. |
| OH01 production and renewal | Successful marked return creates one duty; raising creates none; forwarding preserves one existing pending duty; returning an acknowledged, discarded or unmarked value renews one outward duty. |
| OH02 carriers and exits | Aliases prevent premature loss; equal independent values stay distinct; overwrite/deletion and exceptional RHS evaluation preserve evaluation order; unknown carriers prevent false definite last-carrier findings. |
| OH03 acknowledgment | Exact variants, OR arms, singleton/multiple-variant wildcards, partial matches, false/raising guards and established guard effects have path-specific expectations. |
| OH04 forwarding | Argument failure prevents callee effects; helper normal/exceptional exits, plain-return escapes and `finally` replacement preserve the actual duty and outgoing path. |
| OH05 waiver and disposition | Reasoned discard, acknowledgment and suppression remain distinguishable; each leaves independent resource and work duties unchanged. |
| OH06 asynchronous production | Successful direct await produces the duty; coroutine creation, failed await, cancellation before start and interrupted observation cannot fabricate delivery or acknowledgment. |
| OH07 integration | Applicable published contracts preserve the same boundary; contract removal, unknown helper effects and new waivers remain visible in [change review](change-review.md). |

Resource-payload composition remains conditional on qualified OH08/TC03.
Selected custom adapters and field carriers remain later OH09/OH10 work under
the [workstream allocation](contract-and-portfolio-workstreams.md).

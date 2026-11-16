# 0.7 typed outcomes specification

Status **Implemented; qualification pending**. The finalized API and preview
boundaries below are subject to the [phase gates](../0.7-typed-outcomes.md#required-exit-gates)
and [V07 matrix](validation.md). Implementation alone does not satisfy those
gates.

## Python API

Export the same outcome classes, aliases and adapters from `codepact` and
`codepact.rust`. They are pure Python and available on Python 3.11–3.14.

| API | Proposed contract |
| --- | --- |
| `Ok(value)`, `Some(value)` | Generic in the type of the read-only `value` field, which also supports positional matching. |
| `Err(error)` | Generic in the type of the read-only `error` field, which also supports positional matching. The payload can be any type, including an exception. |
| `Nothing()` | A fieldless value. All exact `Nothing` instances compare equal. Construction may return distinct objects. |
| `Result[T, E]`, `Option[T]` | Typing aliases for `Ok[T] \| Err[E]` and `Some[T] \| Nothing`. Payload type parameters are covariant. The aliases are for annotations and variant matching uses the concrete classes. |

Use frozen dataclass semantics for representation, equality and hashing.
Equality compares the same concrete variant and its fields. Hashability
depends on the payload's hashability. Preserve payload identity and allow
ordinary mutation of a mutable payload through its own API. Mark the variants
final for type checkers. Exact constructor resolution and the supported
static type contract determine match coverage.

```python
from codepact import Err, Ok, Result, must_use


@must_use
def parse_port(text: str) -> Result[int, ValueError]:
    try:
        return Ok(int(text))
    except ValueError as error:
        return Err(error)


def port_or_raise(text: str) -> int:
    match parse_port(text):
        case Ok(value):
            return value
        case Err(error):
            raise error
```

Use variant patterns, `isinstance` and explicit payload access or `raise`.
Keep result-use obligations on the callable declarations already understood by
[TP3001](../../../rules.md#tp3001-discarded-must-use-result). The result type
alone creates no must-use declaration. With the finite-match preview enabled,
a direct supported match of a marked call handles the immediate result.
Assignment and forwarding keep their existing result-use meaning.

## Explicit conversions

Use `Option[T]` for nullable inputs of type `T | None` after conversion and
`T | None` for the reverse direction. The capture helpers return
`Result[T, E]` for a callable yielding `T` and a tuple of exception classes
typed as `tuple[type[E], ...]`. The async callable yields an awaitable of
`T`. Preserve the callable's parameters with `ParamSpec`.

| Helper | Proposed behavior |
| --- | --- |
| `from_nullable(value)` | Convert `None` to `Nothing()` and every other value to `Some(value)`. |
| `to_nullable(option)` | Return a `Some` payload as-is and convert `Nothing()` to `None`. `Some(None)` and absence collapse to the same nullable value. |
| `capture(function, exceptions, /, *args, **kwargs)` | Call a synchronous function once. Return `Ok(value)` on success or `Err(error)` with the same exception object for a matching captured exception. Preserve the callable's parameter types. |
| `capture_async(function, exceptions, /, *args, **kwargs)` | When awaited, call the awaitable-producing function once and await its result in the current task. Apply the same conversion to an eligible exception from the call or await. |

Both capture helpers require a nonempty tuple of `Exception` subclasses.
Validate the tuple before calling the function. Invalid declarations raise
`TypeError`. Capture follows subclass matching. Unmatched exceptions,
`asyncio.CancelledError`, `concurrent.futures.CancelledError`,
`KeyboardInterrupt`, `SystemExit`, `GeneratorExit` and exception groups
propagate unchanged, including with `(Exception,)`. The preview treats groups
as a unit. A synchronous capture of an awaitable only wraps the returned
object. Use `capture_async` to observe its eventual completion.

The async adapter creates no background task. Flow may rely on independently
modeled completion or cleanup effects of the underlying operation. Where
underlying effects are unresolved, retain incomplete coverage at the
affected operation. An `Ok` or `Err` tag supplies no ownership, cleanup,
join or release effect. [Concurrency gate](../../concurrency.md#release-mapping-and-required-gates)

## Finite-match preview

Use `experimental-finite-matches` in CLI configuration and the corresponding
check/baseline flags and `CheckOptions.experimental_finite_matches` in Rust.
The preview is off by default. TP5001 is a warning and is selected only when
the preview is enabled. The CLI adoption report is schema 3; `CheckResult`
remains schema 1.

1. Resolve constructors, aliases and pattern classes by canonical identity.
   Track exact variant values through simple local assignment and feasible
   joins. Seed both variants for a resolved in-scope `Result` or `Option`
   parameter or synchronous call return when its signature supplies the
   supported closed type contract. This relies on the caller honoring the
   typed contract. Known contradictory values, untyped inputs, ambiguous
   rebinding and possible external subtypes leave the affected fact unknown.
2. Evaluate the scrutinee once and visit cases in source order. A resolved
   variant pattern without constraints, or with positional or keyword
   captures or wildcards on declared match fields, covers its variant. A
   simple OR of supported variant patterns covers their union. An unguarded
   wildcard or capture covers the rest of a known set. Model possible guard
   and body effects. For exhaustiveness, a guarded case always leaves a
   fallthrough alternative. Ignore variants excluded by supported flow and
   cases already covered by an earlier arm.
3. For each known reachable variant with an established fallthrough path,
   report a missing variant at the scrutinee and name it in the finding.
   A reachable unsupported pattern that could determine coverage leaves that
   variant unresolved unless another arm guarantees coverage. Preserve a
   missing-variant finding for any independently established case and qualify
   its certainty under the existing feasible-path policy. Attribute relevant
   incomplete coverage to the unresolved scrutinee or pattern.

Nested value and structural payload tests, dynamic patterns and general
Python union inference remain outside the preview. Preserve effects of
scrutinee evaluation, possible guards, reachable arms, and fallthrough using
the existing flow model. If path or flow limits are reached, use the existing
coverage mechanism and keep independent findings. Match coverage does not
establish cleanup. [Resource payload boundary](../0.7-typed-outcomes.md#resource-payloads)

## Installed provider and adoption previews

The first provider preview handles resolved synchronous module functions in
regular pure-Python packages from wheels. Accept `@must_use`, `MustUse[T]`
and supported `Annotated[T, MustUseResult()]` declarations, including static
aliases and same-distribution re-exports. Use the ordered roots, marker,
distribution ownership, bounded static discovery and provenance rules in the
[published contracts design](../../published-contracts.md#resolving-the-consumer-environment).
CLI discovery hands resolved inputs to the Rust analyzer. Checking the same
inputs through either entry point produces the same caller finding and
coverage, including with baseline policy. The import spelling and canonical
declaration travel with the provider's distribution, version and related
location. Local and unmarked shadowing follow ordinary provider selection.
Versioned manifests and dependency resource effects enter the 0.8 plan.

The ownership diagnostic uses the return annotation when present and the
function signature when absent. Relate each implicated return and its
acquisition, subject to existing detail limits. Suggest `Owned[T]` only when
every feasible modeled normal completion hands off an open owned resource of
the same type and the handoff remains valid after `finally`. A borrowed,
closed, mixed, or unresolved return gets guidance based on the evidence.

Preview two informational-only review hints using resolved identities and
source-backed evidence. TP6001 points at an undeclared local class with a
supported `close` method when it is passed to resolved `contextlib.closing`.
TP6002 points at a synchronous wrapper that directly returns a resolved
must-use call. These hints are not selectable rules and cannot affect policy,
baseline disposition or coverage.
Suggest `@resource(closed_by="close")` once at the undeclared local class in
supported `contextlib.closing(LocalType(...))` calls when a supported `close`
method resolves. Relate the qualifying call sites. Suggest `@must_use` at an
unmarked synchronous wrapper with the direct form `return marked_call(...)`
and a resolved must-use callee. Both hints leave diagnostic disposition,
coverage and analyzer facts unchanged. [Strict mode roadmap](../../../../ROADMAP.md#strict-mode-and-contract-adoption-07)

# 0.7 validation matrix

Status **Pending**. Run against the exact release candidate. Use the published
[0.6.0 evidence](../../../releases/0.6.0-evidence.md) for inherited settings,
fixtures and performance comparisons. The [phase plan](../0.7-typed-outcomes.md)
defines the gates and the [specification](specification.md) defines the expected
preview behavior.

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V07-01 | R01, R05, G2, G3 | Import each public name through both namespaces in either order and compare identities. Exercise construction, frozen fields, payload identity, representation, same-variant equality, distinct-variant inequality, and hashable and unhashable payloads. Check positional and keyword matching. |
| V07-02 | R01, R05, G2, G3 | Using the repository-pinned Mypy, Pyright and Basedpyright, check construction and inferred payload types, positive covariant assignments and rejected reverse assignments for both type parameters and both families, `isinstance` and match narrowing, and rejected field reassignment or subclassing. Type-check against the installed helper. |
| V07-03 | R02 | Round-trip a non-null payload, falsey values and absence. Show the intentional loss on `Some(None)`. Verify both directions preserve identity whenever they carry a payload. |
| V07-04 | R03, R05 | Across sync and async adapters, exercise success, matching subclasses, unmatched exceptions, invalid tuples and broad `(Exception,)`. Assert exactly-once invocation, parameter forwarding, exception identity, and propagation of cancellation, process-control exceptions and groups. Exercise exceptions during the async callable invocation and await. Keep a native `try`/`except` or explicit re-raise example. |
| V07-05 | R03, R06, CS05 | Use a controlled task cancelled before adapter execution, during an await and during awaited cleanup. Assert that wrapping a coroutine, task or resource cannot establish a join or cleanup. Compare with a supported explicit join or close. Keep a pending capture, an unresolved async effect and a retained independent violation as controls. |
| V07-06 | R04 | For both outcome families, pair a missing variant with its repair. Exercise an exact constructor, flow join, supported signature, positional and keyword captures, a variant OR, wildcard/capture, a guarded arm and unguarded fallback. Assert the missing variant and exact scrutinee span. Include singleton flow and an arm after coverage as unreachable controls. Use a marked direct scrutinee and compare with a bare discarded call. |
| V07-07 | R04, R06, G1 | Exercise rebound pattern and constructor identities, a local lookalike, an untyped or open scrutinee, an unsupported payload test after an unguarded variant arm, and an unsupported payload test where coverage depends on it. Pair an unresolved variant with a known missing one. Exercise limits and recovery, and verify guard and arm effects through joins and fallthrough. |
| V07-08 | R06 | Put a modeled resource in each payload variant. Exercise wrapping, assignment, match/extraction, return/argument forwarding and bare or intentional discard. Assert the affected incomplete-coverage location, independent violations and unrelated duties. Repeat with ordinary payloads, a marked call and an unmarked call. Re-run the inherited lifecycle, ownership and advanced-contract controls. |
| V07-09 | R08, G3, G5 | Use isolated wheel and source installs for helper-only, helper plus CLI extra and standalone CLI. Inspect metadata, prove the exact extra pin, assert helper imports and presence or absence of the command, and check each retained CLI platform wheel on its target. Replay migration from 0.6-dependent command invocations in a clean environment, including developer setup and documented commands. |
| V07-10 | R09, G1, G2 | Build and install a marked provider wheel outside its source tree. Use direct, aliased and explicit re-export imports from a separate caller, with all supported declaration forms and an ordinary return type. Check the primary call, related declaration, public and canonical names, installed distribution and version. Compare handled and explicitly discarded calls, disabled checking, an unmarked or local shadow, a different provider version and a relevant unresolved provider. Use a sentinel for accidental execution. Compare CLI and Rust results, including baseline disposition and underlying coverage. |
| V07-11 | R10, G1 | Assert the primary return annotation or function signature, related return and acquisition locations, and `Owned[T]` guidance on a uniform handoff. Contrast with borrowed, closed, mixed, unresolved, and `finally` cleanup paths. Run the resolved closing and forwarding hints with repeats, declared classes, already marked wrappers, shadowed calls and unrelated callees. Confirm hint-only cases exit `0` in advisory and strict. |
| V07-12 | R07, R09, R10, CS05, G4 | Compare fresh and reused results after edits to a constructor use, match guard, outcome signature, adapter or cancellation summary, provider marker/version/root order, and hint evidence. Include a span-only edit, unrelated edit and request cancellation or budget exhaustion followed by a successful retry. Compare coverage, rule certainty, evidence, locations and baseline dispositions. Record scoped query refresh and unrelated reuse. |
| V07-13 | G1–G8 | Retain an optimized candidate comparison to the 0.6 baseline and measure the added match/provider workloads with cold, unchanged, affected and unrelated runs. Record time, peak memory and discovered files. Verify selected and disabled scopes, CLI text/JSON consistency, schema compatibility or migration, documentation replay and the candidate release evidence. |

For each behavior, record the expected finding, certainty, primary and
related locations, coverage and exit status before running the fixture.
Use the existing policy for a supported violation at the selected threshold
and a relevant incomplete-coverage case. Under advisory defaults a
coverage-only case exits `0`. Strict exits `1` for the same case. A
baseline changes eligible finding disposition and preserves underlying
coverage. Keep Python runtime fixtures under test control and retain
versioned results separately from analyzer inputs.

Record commands, exact candidate revision, settings, Python and checker
versions, platform, fixture inputs and raw results in the
[release evidence record](../../release-evidence-template.md). A checked box
requires candidate evidence.

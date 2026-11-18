<!-- release-recipes -->

# Diagnostic rule catalog

Use the code from your report to find the contract involved and the first
repair to consider. Run `codepact explain RULE --json` for the explanation
shipped with your CLI.

## Run a bug and its repair

The tagged synthetic corpus contains complete projects with identical checker
settings for each bug and repair. Install [uv](install.md#standalone-command),
then clone the examples and check both versions of the project.

```sh
git clone --branch v0.9.0 --depth 1 https://github.com/eddiethedean/codepact.git
cd codepact
cases=crates/codepact/tests/projects/code_smells
uv tool run --from codepact-cli==0.9.0 codepact check \
  --config "$cases/batch_teardown/buggy/pyproject.toml" --mode strict
uv tool run --from codepact-cli==0.9.0 codepact check \
  --config "$cases/batch_teardown/fixed/pyproject.toml" --mode strict
```

Expect exit `1` with TP1001, TP1002, and TP1003 on the buggy project. The
repaired project exits `0`. Replace `batch_teardown` with another pair below.

| Pair directory | Finding to look for | Repair to compare |
| --- | --- | --- |
| `batch_teardown` | TP1001, TP1002, TP1003 | Use before close, close on exits, and preserve an open owned return. |
| `preview_side_effects` | TP1001, TP2001 | Keep cleanup with the owner. |
| `delivery_status` | TP3001 | Handle the status result. |
| `aliased_upload` | TP2002, TP2003 | Use the live handle and remove conflicting aliases before transfer. |
| `global_receipt` or `exposed_cursor` | TP2004 | Keep the borrowed value inside its access scope. |
| `forwarded_buffer` | TP2005 | Transfer the handle explicitly to the owning call. |
| `async_index` | TP2006 | Join the task group before cleanup. |

The ownership cases include the opt-in in their project files. To see a
coverage boundary, run the same command with
`--config "$cases/opaque_factory/boundary/pyproject.toml" --mode strict`.
Expect exit `1` with TP9001 and incomplete coverage for opaque effects.
For a comparison, use
`--config "$cases/unmarked_status/control/pyproject.toml" --mode strict`.
The unmarked status has no must-use obligation and the check exits `0`.
[Corpus source and expected findings](https://github.com/eddiethedean/codepact/tree/v0.9.0/crates/codepact/tests/projects/code_smells)

## Published rules

```{include} _generated/published-rules.inc
```

Selection describes the scope in which the binary exposes a rule. A selected rule still needs applicable input and can report a different certainty.
The CLI uses TP3001 at warning severity by default. Its result-use
contract accepts `@must_use`, `MustUse[T]`, and
`Annotated[T, MustUseResult()]` on resolved synchronous returns. Assignment and
direct argument/return forwarding count as immediate handling only; they do not
prove eventual consumption. The rule does not infer resource ownership.

Use of a marked synchronous function inside an unsupported container,
composite expression, dynamic target or async consumption remains a TP9001
boundary. Async return declarations are not treated as synchronous TP3001
callables. `discard` preserves evaluation effects and does not clear cleanup
duties. [Adoption examples](adopting.md)

## Experimental finite outcome matches

TP5001 is an opt-in warning. Enable it with
`--experimental-finite-matches` or `experimental-finite-matches = true`. It
reports known reachable `Ok`/`Err` and `Some`/`Nothing` variants that can
fall through a supported `match`. The preview resolves exact CodePact
identities and supported closed outcome annotations; it does not infer
arbitrary unions, nested payload tests, or dynamic patterns. Guards do not
provide unconditional coverage. Resource payloads do not establish cleanup,
ownership transfer, or task completion. See the
[outcome API and preview limits](contracts.md#typed-outcomes).

## Persistent outcome duties

These rules apply when a callable opts in with a `MustHandle` return or an
applicable published outcome contract.

| Code | Meaning |
| --- | --- |
| `TP5002` | A pending outcome loses its last tracked carrier. |
| `TP5003` | A declaration or discard reason falls outside the supported form. |
| `TP5004` | A pending outcome escapes through a return lacking a `MustHandle` contract. |
| `TP5005` | A pending outcome was intentionally discarded with the recorded reason. |

Handle the outcome with a supported variant match, forward it through a marked
return, or record a deliberate discard. The informational TP5005 finding keeps
the reason visible during review. [Example](contracts.md#acknowledge-a-persistent-outcome)

## Queue, IPC and synchronization rules

**Requires CodePact 0.10.** These rules require the applicable runtime
profile and the ownership or advanced-contract settings for that model.

```{include} _generated/candidate-rules.inc
```

[Run the protocol examples](concurrency.md)

## Informational review hints

TP6001 suggests a `@resource(closed_by="close")` declaration for a local
class with a supported `close` method passed to resolved `contextlib.closing`.
TP6002 suggests `@must_use` on a synchronous wrapper that directly returns a
resolved must-use call. TP6003 records the sole remaining variant acknowledged
by a wildcard and links its producer. The checker records these hints as
`info` review evidence outside the configurable rule set.

(tp3001)=
## TP3001: Discarded must-use result

TP3001 reports a bare expression statement whose resolved synchronous function
or method declares that its result should be handled. The finding points to the
call and relates the declaration that marked it. It does not infer runtime
discard behavior or track a result after assignment.

## Example: conflicting lifecycle metadata

The following alias declares a different close method from its resource class:

```python
from typing import Annotated

from codepact import resource
from codepact.contracts import Resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


ConnectionView = Annotated[Connection, Resource(closed_by="release")]
```

The inspector reports `TP4002` at the alias and relates it to the class
declaration. If the alias is meant to describe the same lifecycle, use
`Resource(closed_by="close")`. If it describes a different runtime type, declare
that type and its lifecycle separately. The valid source may then be rechecked
with `codepact inspect-contracts`.

## Experimental ownership rules

These rules require an explicit ownership opt-in.
[Try the experimental checker](ownership.md)

| Code | Meaning | First repair to consider |
| --- | --- | --- |
| `TP2002` | Use of a moved handle | Use the receiving handle or move after the last use of the source. |
| `TP2003` | Transfer conflicts with a live alias, borrow, or invalid owner state | Transfer an open resource after other uses of it have ended. |
| `TP2004` | Consumption or escape of a checked borrowed view | Keep the view local to the borrow and leave cleanup to the owner. |
| `TP2005` | A tracked resource enters an `Owned` parameter without an explicit move | Pass `move(value)` if the call is intended to transfer cleanup responsibility. |
| `TP2006` | Cleanup while a coroutine or task still borrows the resource | Close after the modeled borrower completes. A cancellation request alone leaves the borrow live. |

## Experimental advanced-contract rules

TP2101 and TP2102 require both `experimental-ownership = true` and
`experimental-advanced-contracts = true` (or their command-line equivalents).
They are enabled by default in that scope and remain subject to normal rule
selection, severity, baseline, and advisory/strict policy.

| Code | Meaning | First repair to consider |
| --- | --- | --- |
| `TP2101` | A known competing shared or exclusive access overlaps an active exclusive region. | Keep other aliases out of the region and pass exclusive views consistently. |
| `TP2102` | A modeled operation may or definitely does violate a finite protocol precondition, or a modeled TaskGroup operation is inactive. | Establish the required state or keep the operation inside its active protocol scope. |

The preview trusts resolved `@transition` declarations and does not verify
their implementation bodies. Unknown exits, dynamic effects, and unsupported
suspension appear in coverage rather than being treated as successful state
preservation. The experimental ThreadPoolExecutor model is limited to its
listed source pin and operations. See the
[advanced ownership guide](ownership.md#advanced-contract-preview-06) for the
exact supported declarations and boundaries.

## Certainty and coverage

`definite` and `possible` describe lifecycle findings under the modeled states;
`unknown` marks an unsupported effect or construct. Check the diagnostic's
`certainty`, `severity`, and related spans, plus the report's `coverage` object.
The [JSON output guide](output.md) documents the fields and
[report-handling guide](ci-reporting.md) shows a CI policy that fails on any
diagnostic or incomplete coverage.

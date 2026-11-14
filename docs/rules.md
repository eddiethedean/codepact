# Diagnostic rule catalog

Use the code from your report to find the contract involved and the first
repair to consider. Run `codepact explain RULE --json` for the explanation
shipped with your CLI.

## Run a bug and its repair

The tagged synthetic corpus contains complete projects with identical checker
settings for each bug and repair. Install [uv](install.md#standalone-command),
then clone the examples and check both versions of the project.

```sh
git clone --branch v0.5.1 --depth 1 https://github.com/eddiethedean/codepact.git
cd codepact
cases=crates/codepact/tests/projects/code_smells
uv tool run --from codepact-cli==0.5.1 codepact check \
  --config "$cases/batch_teardown/buggy/pyproject.toml" --mode strict
uv tool run --from codepact-cli==0.5.1 codepact check \
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
[Corpus source and expected findings](https://github.com/eddiethedean/codepact/tree/v0.5.1/crates/codepact/tests/projects/code_smells)

## Published rules

| Code | Available since | Meaning | First repair to consider |
| --- | --- | --- | --- |
| `TP1001` | 0.2.0 | A resource known or possibly closed is used | Move the use before the closing operation, or make the control-flow paths preserve a valid open resource before use. Review certainty and the related close location. |
| `TP1002` | 0.3.0 | A modeled function exit may leave an owned resource cleanup duty outstanding | Close it on every supported exit or return it through an `Owned[T]` contract. Review certainty, the exit and the acquisition location. |
| `TP1003` | 0.3.0 | An `Owned[T]` return is closed or attempts to return a `Borrowed` resource | Return an open owned resource, or change the declared return contract if ownership is not being transferred. |
| `TP2001` | 0.2.0 | A supported body closes a resource through a `Borrowed` parameter or alias | Keep the borrowed operation non-consuming. If the function truly owns and closes the resource, declare the ownership contract accurately and verify callers. |
| `TP3001` | 0.4.0 | A resolved synchronous must-use result is discarded as a bare expression | Handle or immediately pass/return the value, or use `codepact.discard(call())` when abandonment is intentional. |
| `TP4001` | 0.1.0 | The selected Python grammar could not parse the file | Fix the syntax error or select the source target matching the code. |
| `TP4002` | 0.1.0 | A declaration is invalid or metadata conflicts | Compare the primary and related spans; make the lifecycle/access declarations for that view consistent. |
| `TP9001` | 0.1.0 | An unsupported or unresolved construct leaves coverage incomplete | Supply a missing supported input or reduce the pilot to a documented subset. Do not treat the boundary as a pass. |
| `TP9002` | Rust inspection convenience API | Cancellation produced an incomplete report; cancellable APIs return `AnalysisCancelled` | Treat the result as cancelled and retry only if the host still needs the current revision. |

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

These rules require CodePact 0.5.0 and an explicit ownership opt-in.
[Try the experimental checker](ownership.md)

| Code | Meaning | First repair to consider |
| --- | --- | --- |
| `TP2002` | Use of a moved handle | Use the receiving handle or move after the last use of the source. |
| `TP2003` | Transfer conflicts with a live alias, borrow, or invalid owner state | Transfer an open resource after other uses of it have ended. |
| `TP2004` | Consumption or escape of a checked borrowed view | Keep the view local to the borrow and leave cleanup to the owner. |
| `TP2005` | A tracked resource enters an `Owned` parameter without an explicit move | Pass `move(value)` if the call is intended to transfer cleanup responsibility. |
| `TP2006` | Cleanup while a coroutine or task still borrows the resource | Close after the modeled borrower completes. A cancellation request alone leaves the borrow live. |

## Certainty and coverage

`definite` and `possible` describe lifecycle findings under the modeled states;
`unknown` marks an unsupported effect or construct. Check the diagnostic's
`certainty`, `severity`, and related spans, plus the report's `coverage` object.
The [JSON output guide](output.md) documents the fields and
[report-handling guide](ci-reporting.md) shows a CI policy that fails on any
diagnostic or incomplete coverage.

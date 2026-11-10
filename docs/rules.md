# Diagnostic rule catalog

This catalog describes diagnostics in the published CodePact 0.4.0 synthetic
preview. A diagnostic identifies a modeled problem or an analysis boundary.

| Code | Mode | Meaning | First repair to consider |
| --- | --- | --- | --- |
| `TP1001` | 0.2.0 | A resource known or possibly closed is used | Move the use before the closing operation, or make the control-flow paths preserve a valid open resource before use. Review certainty and the related close location. |
| `TP1002` | 0.3.0 | A modeled function exit may leave an owned resource cleanup duty outstanding | Close it on every supported exit or return it through an `Owned[T]` contract. Review certainty, the exit and the acquisition location. |
| `TP1003` | 0.3.0 | An `Owned[T]` return is closed or attempts to return a `Borrowed` resource | Return an open owned resource, or change the declared return contract if ownership is not being transferred. |
| `TP2001` | 0.2.0 | A supported body closes a resource through a `Borrowed` parameter or alias | Keep the borrowed operation non-consuming. If the function truly owns and closes the resource, declare the ownership contract accurately and verify callers. |
| `TP3001` | 0.4.0 | A resolved synchronous must-use result is discarded as a bare expression | Handle or immediately pass/return the value, or use `codepact.discard(call())` when abandonment is intentional. |
| `TP4001` | Both | The selected Python grammar could not parse the file | Fix the syntax error or select the source target matching the code. |
| `TP4002` | Both | A declaration is invalid or metadata conflicts | Compare the primary and related spans; make the lifecycle/access declarations for that view consistent. |
| `TP9001` | Both | An unsupported or unresolved construct leaves coverage incomplete | Supply a missing supported input or reduce the pilot to a documented subset. Do not treat the boundary as a pass. |
| `TP9002` | Rust inspection convenience API | Cancellation produced an incomplete report; cancellable APIs return `AnalysisCancelled` | Treat the result as cancelled and retry only if the host still needs the current revision. |

The 0.4 CLI uses TP3001 at warning severity by default. Its result-use
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

## Certainty and coverage

`definite` and `possible` describe lifecycle findings under the modeled states;
`unknown` marks an unsupported effect or construct. Check the diagnostic's
`certainty`, `severity`, and related spans, plus the report's `coverage` object.
The [JSON output guide](output.md) documents the fields and
[report-handling guide](ci-reporting.md) shows a CI policy that fails on any
diagnostic or incomplete coverage.

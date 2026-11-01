# Diagnostic rule catalog

This catalog describes the 0.2.0 inspector and lifecycle checker. Codes and
message text remain provisional during 0.x. A
diagnostic identifies a modeled problem or boundary; it does not make a runtime
safety claim.

| Code | Mode | Meaning | First repair to consider |
| --- | --- | --- | --- |
| `TP1001` | 0.2.0 | A resource known or possibly closed is used | Move the use before the closing operation, or make the control-flow paths preserve a valid open resource before use. Review certainty and the related close location. |
| `TP2001` | 0.2.0 | A supported body closes a resource through a `Borrowed` parameter or alias | Keep the borrowed operation non-consuming. If the function truly owns and closes the resource, declare the ownership contract accurately and verify callers. |
| `TP4001` | Both | The selected Python grammar could not parse the file | Fix the syntax error or select the source target matching the code. |
| `TP4002` | Both | A declaration is invalid or metadata conflicts | Compare the primary and related spans; make the lifecycle/access declarations for that view consistent. |
| `TP9001` | Both | An unsupported or unresolved construct leaves coverage incomplete | Supply a missing supported input or reduce the pilot to a documented subset. Do not treat the boundary as a pass. |
| `TP9002` | Rust inspection convenience API | Cancellation produced an incomplete report; cancellable APIs return `AnalysisCancelled` | Treat the result as cancelled and retry only if the host still needs the current revision. |

## Example: conflicting lifecycle metadata

The following alias declares a different close method from its resource class:

```python
from typing import Annotated

from typepact import resource
from typepact.contracts import Resource


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
with `typepact inspect-contracts`.

## Certainty and coverage

`definite` and `possible` describe lifecycle findings under the modeled states;
`unknown` marks an unsupported effect or construct. Check the diagnostic's
`certainty`, `severity`, and related spans, plus the report's `coverage` object.
The [JSON output guide](output.md) documents the fields and
[report-handling guide](ci-reporting.md) shows a CI policy that fails on any
diagnostic or incomplete coverage.

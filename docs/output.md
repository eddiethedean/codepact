# JSON output reference

`inspect-contracts` emits one JSON object. The source preview `check` emits the
same envelope when `--json` is supplied; readable diagnostics are its default.
The Rust library returns `Inspection`. Schema version 0 is provisional, and
checking JSON has not completed phase 0.2 release qualification.

## Top-level object

| Field | Meaning |
| --- | --- |
| `schema_version` | JSON schema version; currently `0` |
| `assurance` | `contract-inspection-only` for inspection; `lifecycle-checking-for-supported-normal-flow` for the checking preview |
| `contracts` | Normalized resource and access declarations |
| `diagnostics` | Syntax errors, declaration conflicts, lifecycle findings in check mode, and unsupported or unresolved boundaries |
| `coverage` | Coverage status, reasons, source byte count, and emitted declaration count |

The following complete result comes from the [quick-start source](quickstart.md), inspected as `connection.py` with the default Python 3.11 target:

```{literalinclude} _examples/inspection.json
:language: json
```

## Contract fields

| Field | Meaning |
| --- | --- |
| `base_symbol` | Normalized module-qualified class or alias target |
| `display_name` | Source-facing name, such as `Connection`, `ConnectionView`, or `read.conn` |
| `lifecycle` | `null` when no lifecycle method is declared; otherwise an object with `closed_by` |
| `access` | One of `unspecified`, `shared`, `owned`, or `exclusive` |
| `source` | Span of the declaration that produced this contract |
| `provenance` | Declaring source forms and their trust level |

Current provenance origins include `python-resource-decorator`, `python-annotation`, `annotated-resource-metadata`, `annotated-access-metadata`, and `explicit-access-alias`. Each emitted fact uses the trust value `declared-unverified`.

## Diagnostics

Each diagnostic has a code, severity, message, primary span, related spans, and certainty. Lifecycle diagnostics use the invalidating call as the primary span and the close operation as a related span when available. Common codes are:

| Code | Meaning |
| --- | --- |
| `TP4001` | Python syntax could not be parsed |
| `TP4002` | A contract declaration is invalid or conflicts with another declaration |
| `TP1001` | A resource that may be closed is used |
| `TP2001` | A `Borrowed` resource is closed within a supported body |
| `TP9001` | An unsupported or unresolved construct prevents complete coverage |
| `TP9002` | The Rust `inspect` convenience API reports a cancelled inspection; `inspect_cancellable` returns `AnalysisCancelled` instead |

Diagnostic codes and schema details are provisional during 0.x. Treat `coverage.status` as the machine-readable completeness indicator; unsupported constructs may produce warnings rather than errors.

### Checking certainty

| `certainty` | Interpretation |
| --- | --- |
| `definite` | The modeled operation violates the contract on the analyzed incoming states |
| `possible` | Supported alternatives include a violating state |
| `unknown` | Analysis encountered an effect or construct outside the modeled subset |

A possible lifecycle finding is a warning. Possible findings and coverage
boundaries alone can accompany exit `0`; errors cause exit `1`. Certainty refers
to the model and its supported normal flow, not to verification of runtime cleanup.
The [lifecycle tutorial](lifecycle.md) explains how to read a checking report.

## Coverage and locations

`coverage.status` is `complete-for-supported-subset` when all relevant source is within the implemented subset. Otherwise it is `incomplete`, and `reasons` describes the boundaries encountered. `source_bytes` counts supplied source text in UTF-8 bytes. `declarations` counts emitted contracts, not all Python statements.

Span line and column values are one-based. Columns count UTF-8 bytes, not Unicode code points or terminal display cells. Paths come from CLI logical paths: relative to `--root` when supplied; otherwise the file label is based on its name.

Spans start at `line`/`column` and end just before `end_line`/`end_column`. Convert byte columns appropriately when highlighting text in an editor. The JSON envelope and each contract carry their own `schema_version`; `assurance` and provenance `trust` describe the strength of the recorded facts.

To preserve a report in a script while handling failed inspections, see [automation and CI](automation.md#save-and-consume-json). For help interpreting a finding, see [troubleshooting](troubleshooting.md).

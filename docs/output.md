# JSON output reference

`inspect-contracts` emits one JSON object. The `check` command emits a distinct,
versioned checker result when `--json` is supplied; readable diagnostics are
its default. The Rust inspection API returns `Inspection`; `check_report`
returns `CheckResult`. Inspection schema version 0 remains unchanged, and the
checker result has its own `check_schema_version`.

## Top-level object

| Field | Meaning |
| --- | --- |
| `schema_version` | JSON schema version; currently `0` |
| `check_schema_version` | Present only for `check`; checker result schema version, currently `1` |
| `target_python_version` | Python grammar and built-in model target supplied to the checker |
| `model_catalog_version` | Version of the built-in model catalog used by this check result |
| `available_models` | Built-in source model identities, target selection rules, behavior summaries, and qualification status |
| `assurance` | `contract-inspection-only` for inspection; `lifecycle-checking-for-supported-normal-flow` in 0.2.0 and `lifecycle-checking-for-supported-exit-flow` in the 0.3 worktree |
| `contracts` | Normalized resource and access declarations |
| `diagnostics` | Syntax errors, declaration conflicts, lifecycle findings in check mode, and unsupported or unresolved boundaries |
| `coverage` | Coverage status, reasons, source byte count, and emitted declaration count |

Checker diagnostics retain the inspection diagnostic fields and add
`completion` (`fallthrough`, `return`, `raise`, `cancelled`, `break`,
`continue`, or `null`), `trust`, `coverage_reason` (or
`null`), and `repair` (or `null`). These are machine-readable descriptions of
the static finding and a suggested source action; they do not claim verified
runtime behavior. Trust is `declared-unverified` for findings based on declared
resource contracts, `unknown-boundary` for unresolved analysis, and
`static-source` for other source findings.

The built-in model catalog identifies `contextlib.closing` and
`contextlib.aclosing` by exact import identity. It records CPython source tags
v3.11.0, v3.12.0 and v3.13.0 in `source_revisions` and `source_urls`, and
states the modeled call/await behavior. A
matching name or shadowed import does not select a model. The catalog lists
models available for the selected Python target; it does not assert that a
source file selected one. Source review is
recorded separately from conformance and release qualification, which remain
pending. The implementation references the [CPython 3.11 source](https://github.com/python/cpython/blob/v3.11.0/Lib/contextlib.py),
[3.12 source](https://github.com/python/cpython/blob/v3.12.0/Lib/contextlib.py),
[3.13 source](https://github.com/python/cpython/blob/v3.13.0/Lib/contextlib.py),
and [Python 3.13 `contextlib` documentation](https://docs.python.org/3.13/library/contextlib.html).

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
| `TP1002` | An owned cleanup duty remains open on a modeled exit |
| `TP1003` | An `Owned[T]` return violates the open-resource handoff contract |
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
to the model and its supported exit flow, not to verification of runtime cleanup.
The [lifecycle tutorial](lifecycle.md) explains how to read a checking report.

## Coverage and locations

`coverage.status` is `complete-for-supported-subset` when all relevant source is within the implemented subset. Otherwise it is `incomplete`, and `reasons` describes the boundaries encountered. `source_bytes` counts supplied source text in UTF-8 bytes. `declarations` counts emitted contracts, not all Python statements.

Span line and column values are one-based. Columns count UTF-8 bytes, not Unicode code points or terminal display cells. Paths come from CLI logical paths: relative to `--root` when supplied; otherwise the file label is based on its name.

Spans start at `line`/`column` and end just before `end_line`/`end_column`. Convert byte columns appropriately when highlighting text in an editor. The JSON envelope and each contract carry their own `schema_version`; `assurance` and provenance `trust` describe the strength of the recorded facts.

To preserve a report in a script while handling failed inspections, see [automation and CI](automation.md#save-and-consume-inspection-json). For help interpreting a finding, see [troubleshooting](troubleshooting.md).

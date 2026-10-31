# JSON output reference

The CLI emits one JSON object. The Rust library returns the same `Inspection` data structure. Schema version 0 is provisional and can change during the 0.x releases.

## Top-level object

| Field | Meaning |
| --- | --- |
| `schema_version` | JSON schema version; currently `0` |
| `assurance` | Always `contract-inspection-only` in 0.1 |
| `contracts` | Normalized resource and access declarations |
| `diagnostics` | Syntax errors, declaration conflicts, and unsupported or unresolved boundaries |
| `coverage` | Coverage status, reasons, source byte count, and emitted declaration count |

This abbreviated excerpt uses the repository's example output:

```json
{
  "schema_version": 0,
  "assurance": "contract-inspection-only",
  "contracts": [
    {
      "schema_version": 0,
      "base_symbol": "examples.contracts.Connection",
      "display_name": "inspect_pythonic.conn",
      "lifecycle": { "closed_by": "close" },
      "access": "shared",
      "source": {
        "file": "examples/contracts.py",
        "line": 18,
        "column": 28,
        "end_line": 18,
        "end_column": 57
      },
      "provenance": [
        { "origin": "explicit-access-alias", "trust": "declared-unverified" },
        { "origin": "python-resource-decorator", "trust": "declared-unverified" }
      ]
    }
  ],
  "diagnostics": [],
  "coverage": {
    "status": "complete-for-supported-subset",
    "reasons": [],
    "source_bytes": 630,
    "declarations": 5
  }
}
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

Each diagnostic has a code, severity, message, primary span, related spans, and certainty. The primary span points to the source construct. Related spans are used when a conflict has an earlier declaration to show. Common 0.1 codes are:

| Code | Meaning |
| --- | --- |
| `TP4001` | Python syntax could not be parsed |
| `TP4002` | A contract declaration is invalid or conflicts with another declaration |
| `TP9001` | An unsupported or unresolved construct prevents complete coverage |
| `TP9002` | A cancellable Rust session stopped before publishing a result |

Diagnostic codes and schema details are provisional during 0.x. Treat `coverage.status` as the machine-readable completeness indicator; unsupported constructs may produce warnings rather than errors.

## Coverage and locations

`coverage.status` is `complete-for-supported-subset` when all relevant source is within the implemented subset. Otherwise it is `incomplete`, and `reasons` describes the boundaries encountered. `source_bytes` counts supplied source text in UTF-8 bytes. `declarations` counts emitted contracts, not all Python statements.

Span line and column values are one-based. Columns count UTF-8 bytes, not Unicode code points or terminal display cells. Paths come from CLI logical paths: relative to `--root` when supplied; otherwise the file label is based on its name.

For the complete example result, see the [snapshot in the repository](https://github.com/eddiethedean/typepact/blob/main/tests/snapshots/example-contracts.json).

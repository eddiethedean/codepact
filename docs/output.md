# JSON and concise output

CodePact has separate versioned envelopes for declaration inspection, the Rust
checker API, the CLI check report, baselines and static metadata. Package versions
do not imply schema versions.

| Output | Schema |
| --- | --- |
| `inspect-contracts --json` and Rust `Inspection` | [inspection schema 0](schemas/inspection-v0.schema.json) |
| Rust `CheckResult` from `check_report` | [checker result schema 1](schemas/check-result-v1.schema.json) |
| CLI `check --json` | [adoption report schema 2](schemas/check-report-v2.schema.json) |
| Baseline file | [baseline schema 1](schemas/baseline-v1.schema.json) |
| `capabilities --json` | [capabilities schema 1](schemas/capabilities-v1.schema.json) |
| `explain RULE --json` | [explanation schema 1](schemas/explanation-v1.schema.json) |
| `check --show-settings --json` | [settings schema 1](schemas/settings-v1.schema.json) |
| `baseline update --json` summary | [baseline-update schema 1](schemas/baseline-update-v1.schema.json) |
| CLI execution/configuration error in JSON mode | [failure envelope schema 2](schemas/failure-v2.schema.json) |

Downloadable schemas and fixture links are listed in the [schema reference](schemas.md).
The 0.4 CLI schema change does not alter inspection schema 0 or the Rust checker API's
schema 1 result. Consumers should reject unknown schema versions and treat a
failed execution envelope as an error, never as an empty completed report.

## CLI adoption report

`codepact check --json` writes one adoption report to stdout. A completed
report has `execution_status: "completed"`, report/check schema version 2,
findings, policy, coverage, source identities and a calculated `exit_code`.
The process status matches the report's exit code. A failure has
`execution_status: "failed"`, version identities, and an `error` object with
category and message; it has no partial findings or clean-check claim.

The main fields are:

| Group | Fields |
| --- | --- |
| Status | `execution_status`, `engine_version`, schema versions, `assurance`, `exit_code` |
| Project | `project_root` physical identity, `logical_root`, `config_file`, selected files, include/exclude patterns and effective settings origins |
| Identity | `snapshot_id`, semantic/policy/model fingerprints and a sorted manifest of logical paths with SHA-256 source hashes |
| Findings | Canonical diagnostic fields plus fingerprint, base/effective severity, disposition/reason, explanation ID, actionability and omitted-detail counts |
| Policy | Full counts by code, certainty and disposition; actionable/returned/audit totals; failures, warnings, and unused suppressions |
| Coverage | Original supported-subset status and reasons, independent of rule filtering, baselines, suppressions, or the detail limit |

`diagnostics` retains active findings and audit entries suppressed inline, by a
baseline, or through rule selection. An entry keeps its original severity,
certainty, trust and source spans; policy fields explain whether it can fail
the current check. `counts` and the other count maps cover the complete result
before `--max-diagnostics`. Returned details order actionable findings first.

`details_truncated` is true when the detail cap or per-finding detail bound
omitted data. `related_sites_omitted`, `message_omitted_bytes`, and
`repair_omitted_bytes` describe per-finding omissions. The whole JSON report is
capped at 32 MiB; exceeding the hard limit produces status 2, not a partial
report. Rerun without a detail cap and compare `snapshot_id` before applying
old locations to current source.

`snapshot_id` includes source, relevant policy, baseline, suppression and model
identities, but not output format, detail limit or wall-clock time. The separate
fingerprints identify semantic inputs, policy inputs and the model catalog.
`project_root` can vary between checkouts; logical paths and finding
fingerprints are relative to the project root.

## Experimental ownership scope

With ownership enabled, schema 2 reports use the assurance value
`experimental-ownership-checking-for-supported-exit-flow`. The selected
rules can include TP2002–TP2006. The settings and snapshot identify the
semantic scope. When the TaskGroup model is selected, `available_models`
includes its runtime assumption and CPython pin. Read its coverage alongside
the diagnostics. Baseline matching includes the ownership scope and selected
model revision. [Ownership guide](ownership.md) · [Schema downloads](schemas.md)

## Diagnostic fields and locations

Each CLI diagnostic includes the legacy checker fields:

| Field | Meaning |
| --- | --- |
| `code`, `severity`, `message` | Rule identifier and source-facing explanation |
| `primary`, `related` | Primary operation and related evidence spans |
| `certainty` | `definite`, `possible`, or `unknown` under the supported model |
| `completion` | Modeled completion kind when applicable, otherwise `null` |
| `trust` | Basis of the source facts, such as `declared-unverified`, `static-source`, or `unknown-boundary` |
| `coverage_reason`, `repair` | Boundary context and advisory source guidance, each nullable |

The CLI adds `fingerprint`, `base_severity`, `effective_severity`,
`disposition`, `disposition_reason`, `explanation_id`, `actionable`, and bounded
detail-omission counters. Dispositions are `active`, `rule-disabled`,
`suppressed-inline`, or `suppressed-baseline`. Suppression never deletes the
raw finding or its coverage effect.

Span lines and columns are one-based. Columns count UTF-8 bytes, not Unicode
code points or terminal cells. End positions are exclusive. Logical file paths
are root-relative and use `/` separators. Convert byte columns to code-point
offsets before highlighting in an editor.

Text and concise reports share the 32 MiB rendered-report cap across stdout
and report metadata on stderr. Overflow returns exit 2 before a partial report
is published; bounded evidence never implies a clean result. JSON serialization
checks its cap before writing stdout.

Reports selecting TP3001 carry
`assurance: lifecycle-and-must-use-checking-for-supported-exit-flow`.
Lifecycle-only Rust results cannot claim that rule was checked. Cancelled or
inspection-only inputs cannot be converted into completed check reports.
The snapshot includes the analyzed scope, semantic rule revision and baseline
fingerprint algorithm revision.

## Policy and exit interpretation

Advisory mode fails on active findings at or above `fail-on`; the default is
`error`. `TP3001` defaults to warning. Strict mode also fails on active
possible violations and incomplete coverage. Mandatory syntax/declaration
errors always fail and cannot be demoted. Mode, severity, filtering,
suppression, and baseline matching do not change a diagnostic's certainty.

Status 0 means the selected policy passed, not that all Python behavior was
proven. Status 1 retains a completed report that failed policy. Status 2 means
no completed check should be assumed. Coverage remains authoritative even when
all reported violation rules are disabled or every eligible finding is
suppressed.

## Baselines and capabilities

Baseline schema 1 contains checker/rule/model/target identity, a source
manifest, coverage state, a required update reason, and eligible finding
entries. Fingerprints use algorithm version 5. Resolved resource contracts and must-use
callable identities and canonical Python tokens participate in matching. Version 3/4 baselines must be
regenerated explicitly; unsupported versions fail with exit 2. A changed line/comment can keep
an unambiguous match; duplicate identical sites are treated as ambiguous so a
new finding is not hidden by an ordinal fallback. Incompatible or stale entries
remain unmatched and visible in the check summary.

`capabilities --json` lists supported target versions, rule tiers and semantic
revisions, report formats, configuration names, bounds and unavailable
services. `explain RULE --json` provides static valid/invalid/unknown examples,
preconditions, limitations and advisory repair guidance. Neither advertises
automatic fixes.

## Location-per-line format

`--output-format concise` emits one tab-separated record per actionable primary
or related location:

```text
JSON_PATH<TAB>LINE<TAB>COLUMN<TAB>SEVERITY<TAB>CODE<TAB>ROLE<TAB>JSON_MESSAGE
```

`JSON_PATH` and `JSON_MESSAGE` are JSON string literals. The remaining fields
are one-based line, UTF-8 byte column, effective severity, rule code, and
`primary` or `related`. Related records inherit the parent rule and severity
and immediately follow the primary record. The seven fields are safe to decode
with a TSV reader and `json.loads` for the first and last fields:

```python
import json
import sys

for raw_line in sys.stdin:
    row = raw_line.rstrip("\n").split("\t")
    if len(row) != 7:
        raise SystemExit("invalid CodePact concise record")
    path = json.loads(row[0])
    line, column = int(row[1]), int(row[2])
    severity, code, role = row[3:6]
    message = json.loads(row[6])
    print(path, line, column, severity, code, role, message)
```

Concise stdout contains only actionable location records. Counts and coverage
status go to stderr. Suppressed and non-actionable audit findings are available
in JSON. Use JSON when a consumer needs the complete policy/audit contract.

## Validate retained reports before using them

The source checkout includes a [reference consumer](https://github.com/eddiethedean/codepact/blob/main/scripts/consume_check_report.py)
for CLI agents and external tools. With the development dependencies installed,
run it against the current workspace and a trusted snapshot ID from the latest
completed analysis of that workspace:

```sh
uv run --locked python scripts/consume_check_report.py report.json \
  --root "$CURRENT_CODEPACT_ROOT" --expected-snapshot "$CURRENT_CODEPACT_SNAPSHOT"
```

`CURRENT_CODEPACT_ROOT` is the effective module root from the current settings;
it can be a configured source directory rather than the repository directory.
The expected identity must reflect the latest source, configuration, baseline,
model and scope. Recheck after changing any of them. Copying the ID from an old
report cannot establish freshness. The consumer independently checks current
source hashes and validates the report against the published local schemas.
Schema reference resolution uses local resources.

| Consumer result | Meaning |
| --- | --- |
| `policy-passed` | Current, complete for the supported subset, with a passing selected policy |
| `policy-failed` | Current completed analysis failed its selected policy |
| `incomplete-supported-scope` | Analysis contains unresolved boundaries |
| `limited-recheck-required` | Detail limits omit evidence; request sufficient output before repairing |
| `stale-recheck-required` | The expected snapshot or current source bytes differ |
| `execution-failed` | No completed analysis result exists |
| `invalid-or-unavailable-report` | Unsupported schema, inconsistent identity/counts or unreadable input |

Only `policy-passed` exits 0; the other recognized results exit 1, while invalid
input exits 2. A policy pass can include explicitly acknowledged findings and
never proves general runtime safety. The reference `editor_column` helper
converts one-based UTF-8 byte columns to code-point columns against the current
source line, rejecting offsets inside a code point. It supports Unicode and
CRLF input without treating byte columns as screen coordinates.

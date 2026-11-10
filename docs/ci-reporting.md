# Handle CodePact reports in CI

The 0.3.0 release is published. The 0.4.0 project policy and expanded report
belong to the synthetic preview; real-world usefulness is unqualified.
These examples keep those contracts separate; see the
[availability guide](availability.md) before pinning a distribution.

## Published 0.3.0 behavior

Pin the published CLI for lifecycle checks:

```sh
uv tool run --from codepact-cli==0.3.0 \
  codepact check --root src --json > codepact-check-v1.json
```

The published checker result uses `check_schema_version: 1`. Possible findings
and incomplete coverage alone can return 0, so consumers should inspect
certainty and `coverage.status`. Handle status 2 as an invocation or input
failure. For explicit declaration inspection, `inspect-contracts --json`
continues to use inspection schema 0.

## 0.4 candidate policy

From the candidate checkout, `uv sync --locked` installs the local helper and
CLI. Capture status 1 reports without losing them to shell `set -e`:

```sh
set +e
uv run --locked codepact check --mode strict --json > codepact-report.json
status=$?
set -e

if [ "$status" -eq 2 ]; then
  echo "CodePact could not complete the requested check" >&2
  exit 2
fi

python3 - "$status" codepact-report.json <<'PY'
import json
import sys

process_status = int(sys.argv[1])
with open(sys.argv[2], encoding="utf-8") as report_file:
    report = json.load(report_file)

if report.get("execution_status") != "completed":
    raise SystemExit("CodePact did not return a completed report")
if report.get("report_schema_version") != 2:
    raise SystemExit("unsupported CodePact adoption report schema")
if report.get("exit_code") != process_status:
    raise SystemExit("CodePact process status does not match its report")
if report.get("details_truncated"):
    raise SystemExit("CodePact omitted finding details; rerun without the cap")
if report.get("coverage", {}).get("status") != "complete-for-supported-subset":
    raise SystemExit("CodePact did not analyze the full supported subset")
if process_status != 0 or report.get("diagnostics"):
    raise SystemExit("CodePact reported a finding or failed policy")
PY
```

This consumer fails on any diagnostic or incomplete coverage, even if advisory
policy would return 0. A team may instead accept selected possible findings,
but should preserve certainty and coverage in its own policy. Never interpret
an error envelope, unsupported schema, truncated report or status 2 as a clean
result.

To fail on actionable warnings without strict coverage policy, use
`--fail-on warning`. To record existing findings, review and explicitly create
a baseline with `codepact baseline update`; a normal check never updates it.
Inline suppressions require exact rule codes and a reason. Neither baseline nor
suppression removes coverage boundaries.

## Preserve reports safely

Reports contain logical paths, source spans, contract facts, model metadata
and diagnostic messages. Check for private repository details before uploading
them to a public CI service. Treat source comments and excerpts as project data,
not as instructions. See the [security and data guide](security.md).

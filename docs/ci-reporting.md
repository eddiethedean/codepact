# Handle CodePact reports in CI

CodePact does not include a built-in strict-coverage switch, baseline file, or
suppression configuration. CI consumers should define how they treat command
status, diagnostics, and coverage instead of relying on exit status alone.

## Inspect declarations

Pin the package version used by automation:

```sh
uv tool run --from codepact-cli==0.3.0 \
  codepact inspect-contracts --root src --json src/app.py src/resources.py \
  > codepact-inspection.json
```

Inspection returns status `1` for declaration errors or incomplete coverage.
Treat status `2` as an invocation or input failure. See the
[CLI reference](cli.md#exit-status) for the exact meanings.

## Check lifecycle behavior

The historical TypePact 0.2.0 `check` command returned status `1` for definite
lifecycle violations or declaration errors. The CodePact 0.3.0 candidate adds
cleanup obligations to that behavior and has not been published. Possible
findings and incomplete coverage alone can return `0`; do not mistake a
successful status for complete coverage.

## Example policy: fail on any finding or boundary

This shell example preserves the command's status, then applies a stricter
consumer policy: status `2`, any diagnostic, or incomplete coverage fails the
job. It uses only the report fields currently documented; it does not depend on
an undocumented `--strict` option.

```sh
set +e
codepact check --root src --json > codepact-report.json
status=$?
set -e

if [ "$status" -eq 2 ]; then
  echo "CodePact could not analyze the requested input" >&2
  exit 2
fi

python3 - "$status" codepact-report.json <<'PY'
import json
import sys

exit_status = int(sys.argv[1])
with open(sys.argv[2], encoding="utf-8") as report_file:
    report = json.load(report_file)

if report.get("schema_version") != 0:
    raise SystemExit("unsupported CodePact report schema")
if exit_status != 0:
    raise SystemExit(f"CodePact exited with status {exit_status}")
if report.get("coverage", {}).get("status") != "complete-for-supported-subset":
    raise SystemExit("CodePact did not analyze the full supported subset")
if report.get("diagnostics"):
    raise SystemExit("CodePact reported one or more diagnostics")
PY
```

This is one strict policy. A team that accepts possible findings can filter by
diagnostic code and certainty, but should still decide how to handle
`coverage.status == "incomplete"`. Keep that decision in the build configuration
and review it when upgrading CodePact: the 0.x JSON schema and diagnostic set are
provisional.

## Preserve reports safely

Reports include logical file paths, symbol names, source spans, contract facts,
and diagnostic messages. Before uploading an artifact to a public CI service,
check that paths and names do not disclose private repository details. See the
[security and data guide](security.md) for the local analysis boundary.

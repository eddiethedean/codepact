# Handle TypePact reports in CI

TypePact does not include a built-in strict-coverage switch, baseline file, or
suppression configuration. CI consumers should define how they treat command
status, diagnostics, and coverage instead of relying on exit status alone.

## Published 0.1.0 inspection

Pin the package version used by automation:

```sh
uv tool run --from typepact-cli==0.1.0 \
  typepact inspect-contracts --root src --json src/app.py src/resources.py \
  > typepact-inspection.json
```

Inspection returns status `1` for declaration errors or incomplete coverage.
Treat status `2` as an invocation or input failure. See the
[CLI reference](cli.md#exit-status) for the exact meanings.

## Lifecycle-checking preview

The `check` command belongs to the upcoming 0.2.0 release and is currently
available only from source. The preview returns status `1` for definite
lifecycle violations or declaration errors. Possible findings and incomplete
coverage alone can return `0`. If evaluating the preview in a CI job, follow the
[pinned source instructions](availability.md#try-the-020-source-preview) and
record the source commit. Do not mistake a successful status for complete
coverage.

## Example policy: fail on any finding or boundary

This shell example preserves the command's status, then applies a stricter
consumer policy: status `2`, any diagnostic, or incomplete coverage fails the
job. It uses only the report fields currently documented; it does not depend on
an undocumented `--strict` option.

```sh
set +e
typepact check --root src --json > typepact-report.json
status=$?
set -e

if [ "$status" -eq 2 ]; then
  echo "TypePact could not analyze the requested input" >&2
  exit 2
fi

python3 - "$status" typepact-report.json <<'PY'
import json
import sys

exit_status = int(sys.argv[1])
with open(sys.argv[2], encoding="utf-8") as report_file:
    report = json.load(report_file)

if report.get("schema_version") != 0:
    raise SystemExit("unsupported TypePact report schema")
if exit_status != 0:
    raise SystemExit(f"TypePact exited with status {exit_status}")
if report.get("coverage", {}).get("status") != "complete-for-supported-subset":
    raise SystemExit("TypePact did not analyze the full supported subset")
if report.get("diagnostics"):
    raise SystemExit("TypePact reported one or more diagnostics")
PY
```

This is one strict policy. A team that accepts possible findings can filter by
diagnostic code and certainty, but should still decide how to handle
`coverage.status == "incomplete"`. Keep that decision in the build configuration
and review it when upgrading TypePact: the 0.x JSON schema and diagnostic set are
provisional.

## Preserve reports safely

Reports include logical file paths, symbol names, source spans, contract facts,
and diagnostic messages. Before uploading an artifact to a public CI service,
check that paths and names do not disclose private repository details. See the
[security and data guide](security.md) for the local analysis boundary.

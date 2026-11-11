#!/bin/sh
# Pass the complete CodePact check command, without --json.
if [ "$#" -eq 0 ]; then
    echo "Usage: sh check-report.sh codepact check [options]" >&2
    exit 2
fi

check_exit=0
"$@" --json > codepact-report.json || check_exit=$?
case "$check_exit" in
    0|1) ;;
    *) exit "$check_exit" ;;
esac

python3 - "$check_exit" codepact-report.json <<'PY'
import json
import sys
from pathlib import Path

status = int(sys.argv[1])
try:
    report = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
except (OSError, ValueError) as error:
    print(f"Cannot read CodePact report: {error}", file=sys.stderr)
    raise SystemExit(2) from error

if (
    not isinstance(report, dict)
    or report.get("execution_status") != "completed"
    or report.get("report_schema_version") != 2
    or report.get("exit_code") != status
):
    print("Invalid or unsupported completed CodePact report", file=sys.stderr)
    raise SystemExit(2)

raise SystemExit(status)
PY

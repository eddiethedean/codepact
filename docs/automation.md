# Automation and CI

Use `check` to gate supported lifecycle behavior or `inspect-contracts` to
review declarations in explicit files. The examples pin the 0.5.0
CLI. Preserve the original exit status when saving a JSON report.
[Release availability](availability.md) · [Report handling](ci-reporting.md)

## Run in a uv project

Commit your project's `pyproject.toml` and `uv.lock` after adding CodePact. A reproducible invocation is:

```sh
uv sync --locked
uv run --locked codepact inspect-contracts \
  --python-version 3.11 --root src src/models.py src/client.py
```

This command uses the layout in the [multi-file tutorial](project-guide.md). Include every provider module required by the annotations you want to resolve.

## Run the checker in GitHub Actions

This job pins the standalone CLI, runs strict checks, and keeps the JSON
report when the check fails. Set `src` to the directory that contains your
Python modules and their local contract providers.

```yaml
name: CodePact
on: [push, pull_request]

permissions:
  contents: read

jobs:
  codepact:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v7.0.1
        with:
          persist-credentials: false
      - uses: astral-sh/setup-uv@07a047e608db84b3d8c06cd9a7c7dcc9c355c18e # v10.3.2
        with:
          python-version: '3.11'
      - name: Check resource contracts
        run: >-
          uv tool run --from codepact-cli==0.5.0 codepact check
          --root src --python-version 3.11 --mode strict --json
          > codepact-report.json
      - name: Upload the report
        if: always()
        uses: actions/upload-artifact@v7.0.2
        with:
          name: codepact-report
          path: codepact-report.json
```

Strict mode fails the check on incomplete coverage or unsuppressed possible
violations. A failed invocation may produce a failure envelope or an empty
file. Read the report with its exit status. Review your repository's artifact
visibility before retaining reports with private paths or contract details.
[Report handling](ci-reporting.md) · [Security and data](security.md)

## Save and consume inspection JSON

Redirect standard output to preserve a report:

```sh
uv run --locked codepact inspect-contracts \
  --root src src/models.py src/client.py > inspection.json
```

Preserve the command's exit status when integrating it into a script. Exit `1` means the report contains declaration errors or incomplete coverage. Exit `2` means invocation or input failed; a usable JSON report may be absent. Avoid treating an empty file as a successful inspection.

A report consumer should read `schema_version` before interpreting fields, inspect `diagnostics` and `coverage.status`, and display `coverage.reasons` when analysis is incomplete. Diagnostics alone do not establish completeness: warnings can cause exit `1`, and the coverage status is the authoritative completeness field.

See the [JSON reference](output.md) for field definitions and the [troubleshooting guide](troubleshooting.md) for common failures. Passing this job establishes declaration inspection within the supported subset.

For a complete example that captures exit status and defines a strict coverage
policy, see [report handling](ci-reporting.md).

## Check lifecycle behavior

Set a narrow root for the project you want to analyze:

```sh
uv add codepact==0.5.0
uv run --locked codepact check --root path/to/source --json > checking.json
```

Replace the input path with your project's source directory. To build the
command from source, use Rust 1.96+ and record the checked-out Git SHA.

Use the installed CLI to inspect settings and compare advisory and strict reports:

```sh
uv run --locked codepact check --show-settings --json
uv run --locked codepact check --json > codepact-report.json
uv run --locked codepact check --mode strict --json > codepact-strict-report.json
```

Status `1` identifies a completed report that failed the
selected policy; status `2` is a failed invocation/check. Inspect
`execution_status`, `report_schema_version`, `coverage.status`,
`details_truncated` and `snapshot_id`. See the [adoption guide](adopting.md),
[CLI reference](cli.md) and [CI report example](ci-reporting.md).

# Run CodePact in CI

Start with the same version, root, Python source target, and settings that
you checked locally. The [multi-file tutorial](project-guide.md) uses a
`src/` root and a `[tool.codepact]` table. For an existing project, begin
with the [evaluation workflow](adopting.md#evaluate-one-package).

## Run the checker in GitHub Actions

Save this as `.github/workflows/codepact.yml`. The job runs a standalone
0.9.0 checker against `src/` and uploads the report even when the check
fails. The workflow inherits the check step's success or failure.

Strict mode fails on incomplete coverage and active possible findings. The
default threshold remains `error`, so a definite warning such as `TP3001`
can appear in a passing report. Add `--fail-on warning` if warnings should
also fail the job.

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
          uv tool run --from codepact-cli==0.9.0 codepact check
          --root src --python-version 3.11 --mode strict --json
          > codepact-report.json
      - name: Upload the report
        if: always()
        uses: actions/upload-artifact@v7.0.2
        with:
          name: codepact-report
          path: codepact-report.json
```

Set the root and target to match your local check. If your project already sets
both in `[tool.codepact]`, omit `--root src` and `--python-version 3.11` from
the workflow to inherit that policy. Exit `0` means the check passed the
selected policy. An actionable error or incomplete coverage fails this strict
job with exit `1`. Exit `2` means the check could not complete and the output
may be a failure envelope or empty. Review artifact access before enabling
uploads. [Interpret reports and validate JSON](ci-reporting.md)

## Use a locked uv project

Commit `pyproject.toml` and `uv.lock` after adding CodePact with its matching
CLI extra. A runner with uv can install and check the project.

```sh
uv add 'codepact[cli]==0.9.0'
uv sync --locked
uv run --locked codepact check --root src --mode strict --json > codepact-report.json
```

When the root and mode are already in `[tool.codepact]`, omit their
command-line overrides. Review the [exit-policy matrix](cli.md#policy-and-exits)
before enabling a required check.

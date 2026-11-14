# Run CodePact in CI

Start with the same version, root, Python source target, and settings that
you checked locally. The [multi-file tutorial](project-guide.md) uses a
`src/` root and a `[tool.codepact]` table. For an existing project, begin
with the [evaluation workflow](adopting.md#evaluate-one-package).

## Run the checker in GitHub Actions

Save this as `.github/workflows/codepact.yml`. The job runs a standalone
0.5.1 checker against `src/` and uploads the report even when the check
fails. The workflow inherits the check step's success or failure.

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
          uv tool run --from codepact-cli==0.5.1 codepact check
          --root src --python-version 3.11 --mode strict --json
          > codepact-report.json
      - name: Upload the report
        if: always()
        uses: actions/upload-artifact@v7.0.2
        with:
          name: codepact-report
          path: codepact-report.json
```

Set the root and target to match your project. Review artifact access before
enabling uploads. A failed invocation may leave an empty file or a failure
envelope. [Interpret reports and validate JSON](ci-reporting.md)

## Use a locked uv project

Commit `pyproject.toml` and `uv.lock` after adding CodePact. A runner with
uv can install and check the project.

```sh
uv sync --locked
uv run --locked codepact check --root src --mode strict --json > codepact-report.json
```

When the root and mode are already in `[tool.codepact]`, omit their
command-line overrides. Review the [exit-policy matrix](cli.md#policy-and-exits)
before enabling a required check.

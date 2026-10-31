# Automation and CI

Use `inspect-contracts` as a declaration check for an explicit set of supported source files. Pin version 0.1.0 so helper and analyzer versions stay aligned, and choose the analyzed Python target explicitly.

## Run in a uv project

Commit your project's `pyproject.toml` and `uv.lock` after adding TypePact. A reproducible invocation is:

```sh
uv sync --locked
uv run --locked typepact inspect-contracts \
  --python-version 3.11 --root src src/models.py src/client.py
```

This command uses the layout in the [multi-file tutorial](project-guide.md). Include every provider module required by the annotations you want to resolve.

## Run the standalone CLI in GitHub Actions

This job installs only the CLI tool and inspects that same source layout:

```yaml
name: TypePact declarations
on: [push, pull_request]

permissions:
  contents: read

jobs:
  contracts:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v7.0.1
        with:
          persist-credentials: false
      - uses: astral-sh/setup-uv@07a047e608db84b3d8c06cd9a7c7dcc9c355c18e # v10.3.2
        with:
          python-version: '3.11'
      - name: Inspect declarations
        run: >-
          uv tool run --from typepact-cli==0.1.0 typepact inspect-contracts
          --python-version 3.11 --root src src/models.py src/client.py
```

Adapt the root and file list to your repository. Projects that need the helpers for other steps can instead install their locked uv environment and use `uv run --locked typepact`.

## Save and consume JSON

Redirect standard output to preserve a report:

```sh
uv run --locked typepact inspect-contracts \
  --root src src/models.py src/client.py > inspection.json
```

Preserve the command's exit status when integrating it into a script. Exit `1` means the report contains declaration errors or incomplete coverage. Exit `2` means invocation or input failed; a usable JSON report may be absent. Avoid treating an empty file as a successful inspection.

A report consumer should read `schema_version` before interpreting fields, inspect `diagnostics` and `coverage.status`, and display `coverage.reasons` when analysis is incomplete. Diagnostics alone do not establish completeness: warnings can cause exit `1`, and the coverage status is the authoritative completeness field.

See the [JSON reference](output.md) for field definitions and the [troubleshooting guide](troubleshooting.md) for common failures. Passing this job establishes declaration inspection within the supported subset.

## Evaluate the lifecycle preview

The source checkout includes an unreleased `check` command. Build and run that
checkout explicitly, using a narrow root for the project to be analyzed:

```sh
cargo run --locked -- check --root path/to/source --json > checking.json
```

Run this from a TypePact checkout with Rust 1.96+; replace the input path with your
project's source directory. Pin the TypePact source revision for repeatable CI
runs. The published `typepact-cli==0.1.0` installation above does not provide this
command.

Checking differs from inspection: possible findings and incomplete coverage alone
return `0`. A report consumer must inspect certainty and coverage, preserve exit
`1` for definite errors, and handle exit `2` as invocation or input failure.
Checking JSON is provisional. Configuration files, strict policy switches,
baselines, and suppressions remain future work. Start with the
[lifecycle tutorial](lifecycle.md) before adopting this preview in a build.

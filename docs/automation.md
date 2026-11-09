# Automation and CI

Use `inspect-contracts` as a declaration check for an explicit set of supported source files. CodePact 0.3.0 is published; pin `codepact` to 0.3.0 so the helper and analyzer stay aligned. The release also provides bounded lifecycle and cleanup checking with `check`.

## Run in a uv project

Commit your project's `pyproject.toml` and `uv.lock` after adding CodePact. A reproducible invocation is:

```sh
uv sync --locked
uv run --locked codepact inspect-contracts \
  --python-version 3.11 --root src src/models.py src/client.py
```

This command uses the layout in the [multi-file tutorial](project-guide.md). Include every provider module required by the annotations you want to resolve.

## Run the standalone CLI in GitHub Actions

This job installs only the published CLI tool and inspects the same source
layout. To build from source instead, check out a specific revision and record
its Git SHA:

```yaml
name: CodePact declarations
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
          uv tool run --from codepact-cli==0.3.0 codepact inspect-contracts
          --python-version 3.11 --root src src/models.py src/client.py
```

Adapt the root and file list to your repository. Projects that need the helpers for other steps can instead install their locked uv environment and use `uv run --locked codepact`.

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

Install CodePact 0.3.0 and use a narrow root for the project to be analyzed:

```sh
uv add codepact==0.3.0
uv run --locked codepact check --root path/to/source --json > checking.json
```

Replace the input path with your project's source directory. To build the
command from source, use Rust 1.96+ and record the checked-out Git SHA.

Checking differs from inspection: possible findings and incomplete coverage alone
return `0`. A report consumer must inspect certainty and coverage, preserve exit
`1` for definite errors, and handle exit `2` as invocation or input failure.
Checking JSON is provisional. Configuration files, strict policy switches,
baselines, and suppressions are not available. Use the [CI report example](ci-reporting.md)
to define and test your own report policy. Start with the
[lifecycle tutorial](lifecycle.md) before adopting checking in a build.

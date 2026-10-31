# Automation and CI

Use `inspect-contracts` as a declaration check for an explicit set of supported source files. Pin version 0.1.0, the currently published release, so the helper and analyzer stay aligned. Version 0.2.0 is being prepared; its lifecycle checker is still a source preview.

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

## Save and consume inspection JSON

Redirect standard output to preserve a report:

```sh
uv run --locked typepact inspect-contracts \
  --root src src/models.py src/client.py > inspection.json
```

Preserve the command's exit status when integrating it into a script. Exit `1` means the report contains declaration errors or incomplete coverage. Exit `2` means invocation or input failed; a usable JSON report may be absent. Avoid treating an empty file as a successful inspection.

A report consumer should read `schema_version` before interpreting fields, inspect `diagnostics` and `coverage.status`, and display `coverage.reasons` when analysis is incomplete. Diagnostics alone do not establish completeness: warnings can cause exit `1`, and the coverage status is the authoritative completeness field.

See the [JSON reference](output.md) for field definitions and the [troubleshooting guide](troubleshooting.md) for common failures. Passing this job establishes declaration inspection within the supported subset.

For a complete example that captures exit status and defines a strict coverage
policy, see [report handling](ci-reporting.md).

## Evaluate the lifecycle preview

The upcoming 0.2.0 release adds `check`; the command is currently available only
from source. For a repeatable evaluation, pin the preview commit and use a
narrow root for the project to be analyzed:

```sh
cargo run --locked -- check --root path/to/source --json > checking.json
```

Run this from a TypePact checkout with Rust 1.96+; replace the input path with your
project's source directory. The [availability guide](availability.md#try-the-020-source-preview)
contains a pinned source commit. Record the SHA if you evaluate a newer revision.
The published `typepact-cli==0.1.0` installation above does not provide this
command.

Checking differs from inspection: possible findings and incomplete coverage alone
return `0`. A report consumer must inspect certainty and coverage, preserve exit
`1` for definite errors, and handle exit `2` as invocation or input failure.
Checking JSON is provisional. Configuration files, strict policy switches,
baselines, and suppressions are not available. Use the [CI report example](ci-reporting.md)
to define and test your own report policy. Start with the
[lifecycle tutorial](lifecycle.md) before adopting this preview in a build.

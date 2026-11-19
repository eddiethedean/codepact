# CodePact

CodePact finds Python resource bugs before runtime. Declare how an object
closes and which functions own or borrow it. CodePact reads source and can flag
a connection used after close, cleanup skipped on a supported exit, or a
discarded must-use result. Run it alongside your type checker and tests.

[![Quality](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml)
[![Docs](https://app.readthedocs.org/projects/codepact/badge/?version=stable)](https://codepact.readthedocs.io/en/stable/index.html)
[![PyPI helper](https://img.shields.io/pypi/v/codepact?label=PyPI%20helper)](https://pypi.org/project/codepact/)
[![PyPI CLI](https://img.shields.io/pypi/v/codepact-cli?label=PyPI%20CLI)](https://pypi.org/project/codepact-cli/)
[![MIT license](https://img.shields.io/badge/License-MIT-blue)](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

[Documentation](https://codepact.readthedocs.io/en/stable/index.html) ·
[Install options](https://codepact.readthedocs.io/en/stable/install.html) ·
[Rule catalog](https://codepact.readthedocs.io/en/stable/rules.html) ·
[Release notes](https://codepact.readthedocs.io/en/stable/release-notes.html) ·
[Roadmap](https://github.com/eddiethedean/codepact/blob/main/ROADMAP.md)

## Quick start

Use an empty directory with [uv](https://docs.astral.sh/uv/getting-started/installation/)
and Python 3.11 or later. This POSIX-shell example installs a pinned release,
writes the sample, and checks it as source text. For the install pin that
matches the current distribution, use the [stable install guide](https://codepact.readthedocs.io/en/stable/install.html).

<!-- generated: readme-demo -->
```sh
uv init --bare --name codepact-demo
uv add 'codepact[cli]==0.10.0'
mkdir src
cat > src/connection.py <<'PYTHON'
"""Small lifecycle example with a borrowed use after close."""

from codepact import resource
from codepact.rust import Borrowed


@resource(closed_by="close")
class Connection:
    def status(self) -> str:
        return "ready"

    def close(self) -> None:
        pass


def inspect_connection(conn: Borrowed[Connection]) -> str:
    return conn.status()


conn = Connection()
inspect_connection(conn)
conn.close()
inspect_connection(conn)  # TP1001 at this call; close is the related location.
PYTHON
uv run --locked codepact check --root src
```
<!-- /generated: readme-demo -->

Expect exit `1` with `TP1001` on the last call and a related location on the
preceding close. Remove the last call and confirm the repair.

<!-- smoke: readme-repair -->
```sh
sed '$d' src/connection.py > connection-repaired.py
mv connection-repaired.py src/connection.py
uv run --locked codepact check --root src --mode strict
echo "$?"
```

Expect `0`. [Quick start with PowerShell instructions](https://codepact.readthedocs.io/en/stable/quickstart.html) ·
[Install with pip or use the standalone checker](https://codepact.readthedocs.io/en/stable/install.html) ·
[Check whether your project fits](https://github.com/eddiethedean/codepact/blob/main/docs/compatibility.md)

## What you can check

| Declare | CodePact checks |
| --- | --- |
| `@resource(closed_by="close")` | Supported use after close and conflicting lifecycle metadata |
| `Owned[T]` | Cleanup duties across modeled exits and open resources at ownership handoff |
| `Borrowed[T]` | Closing a resource through a shared view |
| `@must_use` or `MustUse[T]` | Bare discarded calls to marked synchronous functions |
| `MustHandle[Result[T, E]]` or `MustHandle[Option[T]]` | Unacknowledged outcomes across supported assignments, aliases, and returns |

The helpers use `typing.Annotated`, so ordinary types remain available to mypy
and Pyright. The checker follows supported branches, aliases, calls, exceptions,
and cleanup paths. Check reports include primary and related source locations,
certainty, and coverage. [Contract syntax](https://codepact.readthedocs.io/en/stable/contracts.html) ·
[Supported syntax](https://codepact.readthedocs.io/en/stable/supported-syntax.html)

[See what changed in the published release](https://codepact.readthedocs.io/en/stable/release-notes.html)
for persistent outcomes, change review, and reviewed HTTP and SQL profiles.

## Try experimental ownership

Add the following to `pyproject.toml` to enable move and lexical borrow checks.

```toml
[tool.codepact]
config-version = 1
experimental-ownership = true
```

Use `move(value)` to transfer a tracked handle and its cleanup duty. The
checker reports later use of the old handle. The TaskGroup model requires a
separate default-task-factory assumption.
[Ownership examples and scope](https://codepact.readthedocs.io/en/stable/ownership.html)

The new runtime models require an explicit profile for CPython 3.13.16 on
Linux x86-64. The profile describes the program you analyze. You can run the
checker on any supported CLI platform.
[Configure a runtime profile](https://codepact.readthedocs.io/en/stable/reviewed-contracts.html#select-the-runtime-profile)

## Run in CI

Start with a narrow source root. Use strict mode when incomplete coverage
should fail the job, and retain the JSON report alongside the exit status.

```sh
uv run --locked codepact check --root src --mode strict --json > codepact-report.json
```

| Exit | Meaning |
| --- | --- |
| `0` | The completed report passed the selected policy |
| `1` | The completed report failed policy |
| `2` | The invocation or check failed |

Advisory mode can pass with incomplete coverage. The coverage field describes
which parts of the program the checker could analyze. Teams can configure rule
severity, review a baseline, and record line-level exceptions with a reason.
Checks with installed dependency roots or a runtime profile emit report schema
4 with provider selection and model applicability details.
[Adoption guide](https://codepact.readthedocs.io/en/stable/adopting.html) ·
[CI workflow](https://codepact.readthedocs.io/en/stable/automation.html)

## Compatibility

The helper package runs on Python 3.11 and later, with CI on CPython 3.11–3.14.
The analyzer supports Python **3.11–3.13 source targets**. Native CLI wheels are
available for Linux, macOS, and Windows on x86-64 and ARM64. Building from
source requires Rust 1.96 or later and a linker.
[Compare installation, source, and runtime versions](https://github.com/eddiethedean/codepact/blob/main/docs/compatibility.md) ·
[Platform matrix](https://codepact.readthedocs.io/en/stable/availability.html) ·
[Standalone CLI and Rust library](https://codepact.readthedocs.io/en/stable/install.html)

## Contribute

Run `uv sync --locked --extra cli` and `uv run --locked make check` after installing the
repository's pinned Cargo audit tools. The contributor guide lists the setup
and focused check targets.
[Contributing](https://github.com/eddiethedean/codepact/blob/main/CONTRIBUTING.md) ·
[Build the docs](https://codepact.readthedocs.io/en/stable/documentation.html) ·
[MIT license](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

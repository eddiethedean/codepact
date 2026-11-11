# ![CodePact](https://raw.githubusercontent.com/eddiethedean/codepact/main/docs/_static/codepact-banner.png)

**Find Python resource bugs before runtime.** Declare which method closes a
resource and which APIs own or borrow it. CodePact reads Python source and
reports supported use-after-close, missed cleanup, and discarded must-use
results. It checks your code without importing or running it.

[![Quality](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml)
[![Docs](https://app.readthedocs.org/projects/codepact/badge/?version=latest)](https://codepact.readthedocs.io/en/latest/index.html)
[![PyPI helper](https://img.shields.io/pypi/v/codepact?label=PyPI%20helper)](https://pypi.org/project/codepact/)
[![PyPI CLI](https://img.shields.io/pypi/v/codepact-cli?label=PyPI%20CLI)](https://pypi.org/project/codepact-cli/)
[![MIT license](https://img.shields.io/badge/License-MIT-blue)](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

[Documentation](https://codepact.readthedocs.io/en/latest/index.html) ·
[Install options](https://codepact.readthedocs.io/en/latest/install.html) ·
[Rule catalog](https://codepact.readthedocs.io/en/latest/rules.html) ·
[Roadmap](https://github.com/eddiethedean/codepact/blob/main/ROADMAP.md)

**New in 0.5.0.** Opt in to explicit moves, lexical shared borrows, and a
bounded `asyncio.TaskGroup` model. The default scope retains lifecycle and
must-use checks. [Ownership guide](https://codepact.readthedocs.io/en/latest/ownership.html) ·
[Release availability](https://codepact.readthedocs.io/en/latest/availability.html)

## Quick start

In an existing uv project, install the helper package and its matching CLI.

```sh
uv add codepact==0.5.0
mkdir -p demo
```

Save this as `demo/connection.py`.

```python
"""Small lifecycle example for `codepact check --root examples`."""

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
```

```sh
uv run --locked codepact check --root demo
```

```text
connection.py:23:1: error TP1001: Borrowed call requires an open resource
  certainty: definite
  trust: declared-unverified
  repair: Keep the use before close, or guard it with a lifecycle check.
  related: connection.py:22:1
1 diagnostics; 1 actionable; 0 suppressed; 0 rule-disabled; 1 returned
```

The check exits `1`. Remove the call after `conn.close()` and rerun the same
command to get exit `0`. The example uses a stub close method. CodePact checks
the declared state transition, while your implementation supplies the actual
cleanup. [Quick start and report walkthrough](https://codepact.readthedocs.io/en/latest/quickstart.html)

## What you can check

| Declare | CodePact checks |
| --- | --- |
| `@resource(closed_by="close")` | Supported use after close and conflicting lifecycle metadata |
| `Owned[T]` | Cleanup duties across modeled exits and open resources at ownership handoff |
| `Borrowed[T]` | Closing a resource through a shared view |
| `@must_use` or `MustUse[T]` | Bare discarded calls to marked synchronous functions |

The helpers use `typing.Annotated`, so ordinary types remain available to mypy
and Pyright. The checker follows supported branches, aliases, calls, exceptions,
and cleanup paths. Check reports include primary and related source locations,
certainty, and coverage. [Contract syntax](https://codepact.readthedocs.io/en/latest/contracts.html) ·
[Supported syntax](https://codepact.readthedocs.io/en/latest/supported-syntax.html)

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
[Ownership examples and scope](https://codepact.readthedocs.io/en/latest/ownership.html)

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
[Adoption guide](https://codepact.readthedocs.io/en/latest/adopting.html) ·
[CI examples](https://codepact.readthedocs.io/en/latest/ci-reporting.html)

## Compatibility

The helper package runs on Python 3.11 and later, with CI on CPython 3.11–3.14.
The analyzer supports Python **3.11–3.13 source targets**. Native CLI wheels are
available for Linux, macOS, and Windows on x86-64 and ARM64. Building from
source requires Rust 1.96 or later and a linker.
[Platform matrix](https://codepact.readthedocs.io/en/latest/availability.html) ·
[Standalone CLI and Rust library](https://codepact.readthedocs.io/en/latest/install.html)

## Contribute

Run `uv sync --locked` and `uv run --locked make check` after installing the
repository's pinned Cargo audit tools. The contributor guide lists the setup
and focused check targets.
[Contributing](https://github.com/eddiethedean/codepact/blob/main/CONTRIBUTING.md) ·
[Build the docs](https://codepact.readthedocs.io/en/latest/documentation.html) ·
[MIT license](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

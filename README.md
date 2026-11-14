# CodePact

![CodePact banner](https://raw.githubusercontent.com/eddiethedean/codepact/v0.5.1/docs/_static/codepact-banner.png)

CodePact finds Python resource bugs before runtime. Declare how an object
closes and which functions own or borrow it. CodePact reads source and can flag
a connection used after close, cleanup skipped on a supported exit, or a
discarded must-use result. Run it alongside your type checker and tests.

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

In an empty directory with [uv](https://docs.astral.sh/uv/getting-started/installation/)
and Python 3.11 or later, start a project.

```sh
uv init --bare --name codepact-demo
uv add codepact==0.5.1
uv run --locked codepact --version
mkdir src
```

Save the complete [connection example](https://github.com/eddiethedean/codepact/blob/v0.5.1/examples/lifecycle.py)
as `src/connection.py` and run the check.

```sh
uv run --locked codepact check --root src
```

Expect exit `1` with a related location pointing to the close.

```text
connection.py:23:1: error TP1001: Borrowed call requires an open resource
  related: connection.py:22:1
```

Delete the final line and rerun with `--mode strict` to confirm exit `0`.
[Full walkthrough](https://codepact.readthedocs.io/en/latest/quickstart.html) ·
[Install with pip or run the standalone command](https://codepact.readthedocs.io/en/latest/install.html)

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
[CI workflow](https://codepact.readthedocs.io/en/latest/automation.html)

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

# CodePact

**Find lifecycle mistakes in Python before running your code.**

CodePact is a static analyzer for resource contracts. Declare how a resource
closes and which functions own or borrow it; CodePact checks supported paths
for use after close, missed cleanup, and invalid ownership handoffs. It reads
source without importing or executing your application.

[![PyPI](https://img.shields.io/pypi/v/codepact.svg)](https://pypi.org/project/codepact/)
[![CLI on PyPI](https://img.shields.io/pypi/v/codepact-cli.svg?label=CLI)](https://pypi.org/project/codepact-cli/)
[![Python](https://img.shields.io/badge/Python-3.11%2B-3776AB?logo=python&logoColor=white)](#compatibility)
[![Quality](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/eddiethedean/codepact/actions/workflows/quality.yml)
[![Documentation](https://readthedocs.org/projects/typepact/badge/?version=latest)](https://typepact.readthedocs.io/en/latest/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[Quick start](#quick-start) · [Adopt 0.4](#adopt-the-04-preview) ·
[Documentation](https://typepact.readthedocs.io/en/latest/) ·
[Rule catalog](docs/rules.md) · [Roadmap](ROADMAP.md)

**CodePact 0.4.0 was published on November 10, 2026** as a synthetic preview
of project configuration, baselines, suppressions, and synchronous must-use
checking. Real-world usefulness remains unqualified. APIs and report schemas
remain provisional during 0.x.
[PyPI helper](https://pypi.org/project/codepact/0.4.0/) · [PyPI CLI](https://pypi.org/project/codepact-cli/0.4.0/) ·
[Crates.io](https://crates.io/crates/codepact/0.4.0) · [Verified release evidence](docs/releases/0.4.0-evidence.md)

## What it checks

| Contract | Finding | Available in |
| --- | --- | --- |
| A resource is closed by a declared method | Use after close (`TP1001`) | 0.3 and 0.4 |
| A function owns a resource | Outstanding cleanup duty on a modeled exit (`TP1002`) | 0.3 and 0.4 |
| A function returns `Owned[T]` | Invalid ownership handoff (`TP1003`) | 0.3 and 0.4 |
| A function borrows a resource | Close through a borrowed view (`TP2001`) | 0.3 and 0.4 |
| A synchronous result must be handled | Discarded must-use result (`TP3001`) | 0.4 |
| Resource declarations must agree | Conflicting lifecycle/access metadata (`TP4002`) | 0.3 and 0.4 |

The checker covers bounded local flow, aliases, supported calls, branches,
loops, exceptional exits, `finally`, context managers, and supported async
cleanup/cancellation paths. Reports expose certainty, related locations, and
coverage boundaries. A clean report describes the supported subset; it does
not establish general Python resource safety.

## Quick start

In an existing uv project, install the published helper and matching CLI:

```sh
uv add codepact==0.4.0
uv run --locked codepact --version
mkdir -p demo
```

Save this as `demo/connection.py`:

```python
from codepact import resource
from codepact.rust import Borrowed


@resource(closed_by="close")
class Connection:
    def status(self) -> str:
        return "ready"

    def close(self) -> None:
        pass


def read(conn: Borrowed[Connection]) -> str:
    return conn.status()


conn = Connection()
conn.close()
read(conn)  # TP1001: the connection has already been closed.
```

Run the checker on that directory:

```sh
uv run --locked codepact check --root demo
```

CodePact reports a definite `TP1001` at `read(conn)`, relates the earlier close,
and exits with status `1`. Move `read(conn)` before `conn.close()` and recheck
to clear the finding. These methods are small stubs: CodePact trusts the
lifecycle declaration and does not verify what `close()` releases at runtime.

To inspect the declarations as JSON:

```sh
uv run --locked codepact inspect-contracts demo/connection.py
```

This records the resource lifecycle and the shared access intent of `read.conn`.
The [lifecycle tutorial](docs/lifecycle.md) explains the analysis, and the
[cleanup tutorial](docs/planning/phases/0.3/cleanup-tutorial.md) covers exit duties.

### Other installation options

```sh
# Python helpers and matching CLI
python -m pip install 'codepact==0.4.0'

# Standalone CLI
uv tool install codepact-cli==0.4.0

# Rust library
cargo add codepact@0.4.0
```

The `codepact` distribution pins `codepact-cli` to the same version. Helpers
are pure Python and include `py.typed`; importing them does not start the
analyzer. Prebuilt CLI wheels include a native executable and require no Rust
compiler. Source builds require Rust 1.96+ and a linker.

## Declare intent with ordinary Python types

The helpers use `typing.Annotated`, preserving the underlying type for mypy
and Pyright:

| Declaration | Meaning |
| --- | --- |
| `@resource(closed_by="close")` | Declare the method that closes a resource |
| `Owned[T]` | Declare owned access or an ownership handoff |
| `Borrowed[T]` | Declare shared borrowed access |
| `BorrowedMut[T]` | Declare exclusive borrowed access |
| `MustUse[T]` or `@must_use` | Declare a result that requires immediate handling |

Access aliases come from `codepact.rust` or `codepact.contracts`. You can also
write metadata explicitly:

```python
from typing import Annotated

from codepact.contracts import Resource, SharedBorrow


ConnectionView = Annotated[
    Connection,
    Resource(closed_by="close"),
    SharedBorrow(),
]
```

Here, `Connection` is the class from the quick start. Metadata describes the
alias's view. Compatible declarations combine; conflicts report both source
locations. The helpers record intent without enforcing borrowing at runtime.
See [contracts and annotation forms](docs/contracts.md).

## Adopt the 0.4 preview

Use the 0.4.0 packages from the quick start to try configuration, policy,
baselines, and must-use results. The capabilities command lists the rules,
schemas, and limits supported by the installed CLI.

```sh
uv run --locked codepact capabilities --json
```

For a reproducible source build, use the `v0.4.0` tag.
[Build instructions](docs/install.md#build-from-source)

### Handle must-use results

```python
from codepact import discard, must_use


@must_use
def fetch() -> int:
    return 42


def run() -> None:
    fetch()             # TP3001: result discarded
    value = fetch()     # Immediate handling by assignment
    discard(fetch())    # Explicit intentional abandonment
```

`MustUse[int]` and `Annotated[int, MustUseResult()]` return annotations express
the same contract. The rule checks immediate handling of resolved synchronous
calls. It does not track later use of an assigned value, infer ownership, or
prove async result consumption. `discard()` preserves evaluation effects and
does not close a resource. `TP3001` defaults to warning severity; use
`--fail-on warning` to make it fail the check.

### Add a project policy

For a project with a `src/` directory, add this to `pyproject.toml`:

```toml
[tool.codepact]
config-version = 1
root = "src"
python-version = "3.12"
exclude = ["generated/**"]
mode = "strict"
fail-on = "warning"
```

Inspect the effective settings and run a check:

```sh
codepact check --show-settings --json
codepact check --json > codepact-report.json
codepact explain TP3001 --json
```

Strict mode also fails on unsuppressed possible violations and incomplete
coverage. JSON reports retain certainty, coverage, policy, suppression data,
and snapshot identity. See [CLI options](docs/cli.md) and
[report formats and schemas](docs/output.md).

### Adopt checks gradually

Review current findings, then explicitly create a baseline:

```sh
codepact baseline update --output codepact-baseline.json \
  --reason "reviewed findings before adoption"
codepact check --baseline codepact-baseline.json
```

Commit the reviewed baseline with the project. Replacing it requires
`--replace`; ordinary checks never rewrite it. For a local exception, name the
rule and explain why:

```python
fetch()  # codepact: ignore[TP3001] -- queried intentionally for side effects
```

Eligible suppressed findings remain in the JSON audit output. Baselines and
inline suppressions preserve mandatory errors and incomplete coverage.
Follow the [adoption guide](docs/adopting.md) for matching rules, stale entries,
policy configuration, and qualification status.

## Choose the right command

| Command | Purpose |
| --- | --- |
| `codepact check --root src` | Discover Python files and check supported flow |
| `codepact inspect-contracts --root src src/models.py src/client.py` | Inspect declarations in explicit files and resolve their supported imports |
| `codepact check --json` | Emit a machine-readable report; schema 2 in the 0.4 CLI |
| `codepact check --show-settings --json` | Inspect configuration and its origins |
| `codepact capabilities --json` | List rules, schemas, targets, and limits |

`check` scans its root; explicit file operands add inputs rather than narrowing
the scan. `inspect-contracts` reads only the files supplied. The default source
target is Python 3.11; select another with `--python-version`.

| Check status | Meaning |
| --- | --- |
| `0` | The completed report passes the selected policy |
| `1` | The completed report contains a finding or coverage condition that fails policy |
| `2` | Invocation, configuration, input, or execution failed |

Advisory mode can return `0` with incomplete coverage. Read the coverage and
certainty fields alongside the exit code. Inspection has its own preserved
exit policy, documented in the [CLI reference](docs/cli.md).

## Compatibility and limits

| Surface | Support |
| --- | --- |
| Python helper runtime | Python 3.11+; CI covers CPython 3.11–3.14 |
| Analyzed Python source | Python 3.11, 3.12, and 3.13 |
| Python typing | Inline types, `py.typed`, mypy and Pyright fixtures |
| Rust source builds | Rust 1.96+ |
| Published CLI wheels | Linux glibc/musl, macOS, and Windows; x86-64 and ARM64 |

Platform versions and published artifact evidence are in the
[availability guide](docs/availability.md). Running the helpers on Python 3.14
does not enable analysis of Python 3.14 grammar.

Analysis is bounded and reports unsupported constructs. String annotations,
`from __future__ import annotations`, dynamic dispatch, and resource aliases
through arbitrary fields or containers remain important limitations. The
checker does not prove that a declared close method releases an operating
system resource. See [supported syntax](docs/supported-syntax.md) and
[limitations](docs/limitations.md) before relying on a result.

## How it works

Python helpers provide declarations. The Rust engine uses Ruff's parser and
AST for syntax and source ranges, then performs bounded semantic and flow
analysis. Salsa caches reusable queries across edits in an analysis session.

The Rust crate exposes `inspect`, `check`, `check_report`, and
`AnalysisSession` for repeated updates and cancellation. Existing Rust APIs
keep their released result contracts; the 0.4 CLI's adoption policy wraps the
canonical findings. See the [Rust API guide](docs/rust-api.md) and
[architecture](docs/architecture.md).

## Contributing

From a checkout, install the pinned auditors and run the full gate:

```sh
uv sync --locked
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
```

The gate covers Rust and Python quality, tests, MSRV, dependency audits,
package builds and isolated installation, schema fixtures, documentation, and
checks that analyzed source is never executed. GitHub Actions runs the Python
matrix on 3.11–3.14. See [CONTRIBUTING.md](CONTRIBUTING.md) for focused targets.

- [Documentation](https://typepact.readthedocs.io/en/latest/) — guides and API references.
- [Rule catalog](docs/rules.md) — findings, certainty, and repair guidance.
- [Roadmap](ROADMAP.md) — release progression and pending work.
- [Planning index](docs/planning/README.md) — design decisions and qualification plans.

The 0.1 and 0.2 releases used the former TypePact name. The documentation site
retains its `typepact.readthedocs.io` address; the project and current packages
are named CodePact.

## License

[MIT](LICENSE).

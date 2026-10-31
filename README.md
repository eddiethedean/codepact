# TypePact

TypePact catches conflicting resource declarations in Python and previews lifecycle bugs such as use-after-close. Its analyzer reads source without importing or running application code.

[![PyPI](https://img.shields.io/pypi/v/typepact.svg)](https://pypi.org/project/typepact/)
[![CLI on PyPI](https://img.shields.io/pypi/v/typepact-cli.svg?label=CLI)](https://pypi.org/project/typepact-cli/)
[![Python](https://img.shields.io/badge/Python-3.11%2B-3776AB?logo=python&logoColor=white)](#compatibility)
[![Quality](https://github.com/eddiethedean/typepact/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/eddiethedean/typepact/actions/workflows/quality.yml)
[![Documentation](https://readthedocs.org/projects/typepact/badge/?version=latest)](https://typepact.readthedocs.io/en/latest/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/eddiethedean/typepact/blob/main/LICENSE)

Declare how resources close and how APIs intend to access them alongside ordinary Python types. The published 0.1.0 release inspects these declarations. The upcoming 0.2.0 release adds lifecycle checking; the current source branch includes its pre-release preview. The preview is not available from the published 0.1.0 packages. APIs, diagnostics, and the JSON schema remain provisional during 0.x.

| Task | Command | Availability |
| --- | --- | --- |
| Inspect lifecycle and access declarations | `typepact inspect-contracts FILE...` | Published 0.1.0 and source checkout |
| Find supported uses after close and closes through Borrowed parameters | `typepact check --root DIR` | Source checkout preview |

For the version currently published, start with [installation](#install) and [contract inspection](#inspect-your-first-contract). To evaluate the upcoming lifecycle checker, see [the source preview](#try-lifecycle-checking-from-source) and [availability guide](docs/availability.md).

## Install

Add TypePact to your Python project:

```sh
uv add typepact==0.1.0
```

This is the latest published release while 0.2.0 is being prepared. Once 0.2.0 is published, use `uv add typepact==0.2.0` to pin that release. With pip, install the same helper and matching CLI dependency using `python -m pip install 'typepact==0.1.0'`.

This installs two matching distributions:

| Distribution | Contents |
| --- | --- |
| `typepact` | Pure Python declaration helpers, typing metadata, and `py.typed` |
| `typepact-cli` | The native Rust `typepact` executable |

`typepact` pins `typepact-cli` to the same version. Importing the Python helpers does not start the analyzer or download an executable. Prebuilt CLI wheels require no Rust compiler; building the CLI from source requires Rust 1.96 or newer and a linker.

For the command alone:

```sh
uv tool install typepact-cli==0.1.0
typepact --version
```

## Inspect your first contract

Save this as `connection.py`:

```python
from typepact import resource
from typepact.rust import Borrowed, Owned


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def read(conn: Borrowed[Connection]) -> str:
    return "ready"


def consume(conn: Owned[Connection]) -> None:
    conn.close()
```

Inspect its declarations:

```sh
uv run typepact inspect-contracts connection.py
```

TypePact emits JSON containing these contract views:

| Declaration | Base type | Lifecycle | Access intent |
| --- | --- | --- | --- |
| `Connection` | `connection.Connection` | Closed by `close` | Unspecified |
| `read.conn` | `connection.Connection` | Closed by `close` | Shared |
| `consume.conn` | `connection.Connection` | Closed by `close` | Owned |

The `resource` decorator preserves the class identity and records lifecycle metadata. The inspector reads the file as text without importing it, evaluating annotations, or invoking decorators. In the published 0.1.0 release, method bodies are not checked: declaring `closed_by="close"` does not establish that `close` releases anything.

The published inspector also rejects conflicting declarations. The
[TP4002 example](docs/rules.md#example-conflicting-lifecycle-metadata) shows a
resource alias that declares a different close method, the reported locations,
and a valid repair. Lifecycle findings require the upcoming 0.2.0 checker below.

See the complete [paired example](https://github.com/eddiethedean/typepact/blob/main/examples/contracts.py) and its [JSON snapshot](https://github.com/eddiethedean/typepact/blob/main/tests/snapshots/example-contracts.json).

## Try lifecycle checking from source

Use a checkout and Rust 1.96+ to run the preview; the published 0.1.0 packages do not include `check`. For a reproducible preview, check out the implementation commit `cf2fb90989d3ed01b7ca9edb56cc5ff8bd7122c4`:

```sh
git clone https://github.com/eddiethedean/typepact.git
cd typepact
git checkout cf2fb90989d3ed01b7ca9edb56cc5ff8bd7122c4
cargo run --locked -- check --root examples
```

To try a newer preview, use `main` and record `git rev-parse HEAD` with the result. The source checkout still reports version `0.1.0` until the release version is prepared, so `--version` does not distinguish it from the published binary.

The [lifecycle example](https://github.com/eddiethedean/typepact/blob/main/examples/lifecycle.py) constructs a connection, passes it to a Borrowed helper, closes it, then calls the helper again. The final call produces a definite `TP1001`, includes the close location, and exits with status `1`. This example intentionally contains an error.

The preview follows supported normal synchronous flow, local aliases, direct calls, and `if`/`else` joins. It also reports closing a `Borrowed` parameter as `TP2001`. Possible findings and incomplete coverage are visible warnings that can accompany exit `0`; read the report even when the command succeeds.

Follow the [lifecycle tutorial](https://typepact.readthedocs.io/en/latest/lifecycle.html) for an isolated example, expected output, and a valid counterpart. The preview does not establish leak freedom, cleanup on every exit, general ownership safety, or that the declared close method releases a runtime resource. Release status and supported behavior are summarized in the [availability guide](docs/availability.md).

## Annotation forms

The access aliases use `typing.Annotated`, so ordinary Python type checkers retain the underlying type:

| Alias from `typepact.rust` | Metadata from `typepact.contracts` | Intent |
| --- | --- | --- |
| `Borrowed[T]` | `Annotated[T, SharedBorrow()]` | Shared access |
| `Owned[T]` | `Annotated[T, OwnedAccess()]` | Owned access |
| `BorrowedMut[T]` | `Annotated[T, ExclusiveBorrow()]` | Exclusive access |

For example, `Borrowed[Connection]` and `Annotated[Connection, SharedBorrow()]` produce the same shared-access facts. The aliases are also exported from `typepact.contracts`.

You can attach lifecycle and access metadata to an alias of an existing class:

```python
from typing import Annotated

from typepact.contracts import Resource, SharedBorrow


ConnectionView = Annotated[
    Connection,
    Resource(closed_by="close"),
    SharedBorrow(),
]
```

Here, `Connection` is the class from the quick start. Metadata on an alias describes that view; it does not change every use of the underlying class. TypePact combines compatible lifecycle and access declarations and reports conflicting values with their source locations. These annotations express intent; they do not enforce ownership or borrowing at runtime.

## Inspecting a project

Choose a source target explicitly when needed. The default is Python 3.11:

```sh
uv run typepact inspect-contracts --python-version 3.13 connection.py
```

For imports between files, pass every module that should participate in resolution. For a project with a `src` directory:

```sh
uv run typepact inspect-contracts --root src src/models.py src/client.py
```

For `inspect-contracts`, `--root` determines logical paths and package-qualified module names without discovering files. Without it, each input file's stem supplies its module name. Absolute `from module import Class` imports and direct imports of supported assignment aliases resolve across supplied files. The preview `check` command instead scans `.py` files below its root; explicit filenames add inputs and do not restrict that scan.

### Inspection output and exit codes

JSON is the default output; `--json` is accepted explicitly. Each result includes:

- `contracts`: normalized base symbols, lifecycle and access facts, source spans, and declaration provenance.
- `diagnostics`: errors and unsupported-boundary findings, including related locations where applicable.
- `coverage`: `complete-for-supported-subset` or `incomplete`, with reasons and work counts.
- `schema_version: 0` and `assurance: "contract-inspection-only"`.

Contract provenance is `declared-unverified`. Line and column numbers are one-based; columns count UTF-8 bytes.

| Exit code | Meaning |
| --- | --- |
| `0` | Inspection completed with no declaration errors or incomplete coverage |
| `1` | Declaration errors, syntax errors, or unsupported/unresolved constructs were reported |
| `2` | An invocation, file access, input limit, or fatal execution error prevented inspection |

A successful inspection establishes the recorded declarations within the supported subset. It does not prove resource safety.

## Scope of 0.1

The inspector supports module-level classes, literal TypePact resource decorators, simple assignment aliases, parameter and return annotations, and direct absolute imports among explicitly supplied modules. It resolves supported API imports by identity, respects local rebinding, and preserves the class and metadata captured by earlier aliases.

Relevant unsupported constructs produce incomplete coverage. Current boundaries include:

- String annotations and `from __future__ import annotations`.
- Relative imports, star imports, import re-export chains, and supplied-module cycles.
- Class inheritance and metaclass behavior, other class decorators, and decorated functions.
- Computed contract expressions, dynamic decorator arguments, and eager named expressions that could rebind module names.
- Module-level control flow and unsupported nested or generic declarations.

The inspector has explicit source and work limits, including a 1,000,000-byte total source budget per request. See the [inspector guide](https://github.com/eddiethedean/typepact/blob/main/docs/inspector.md) for the detailed limits and Rust session API.

The 0.2.0 preview adds bounded lifecycle flow checks. Cleanup obligations, precise alias analysis through fields and containers, and enforcement of ownership or exclusive access remain future work. The [release roadmap](https://github.com/eddiethedean/typepact/blob/main/ROADMAP.md) describes that progression.

## Compatibility

| Surface | Support |
| --- | --- |
| Python helper runtime | Python 3.11+; CI covers CPython 3.11–3.14 |
| Analyzed Python source targets | 3.11, 3.12, and 3.13 |
| Python typing | Inline types and `py.typed`; compatibility fixtures for mypy and Pyright |
| Rust source builds | Rust 1.96+ |

The native CLI wheel matrix covers eight platform targets:

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Supported | Supported |

The helper wheel is universal pure Python. CLI wheels contain a standalone executable and do not depend on a particular CPython ABI. Identifier validation follows the Unicode rules of the selected source target. See [compatibility](https://github.com/eddiethedean/typepact/blob/main/docs/planning/compatibility.md) and [distribution details](https://github.com/eddiethedean/typepact/blob/main/docs/planning/releases.md) for the complete policy.

## Architecture

The Python helpers supply declarations; the Rust engine performs inspection. Pinned Ruff parser and AST components provide structured syntax and source ranges. Salsa tracks parsing and module semantics so reusable sessions can retain unaffected work across edits.

The Rust library exposes `inspect` for a single request and `AnalysisSession` for repeated updates, removal, reset, cancellation, and revision-tagged results. The source preview adds `check` and session checking methods. CLI and library calls share the analyzer; the host application supplies the Rust API's source snapshot. See the [Rust API guide](https://typepact.readthedocs.io/en/latest/rust-api.html) and [analyzer design](https://github.com/eddiethedean/typepact/blob/main/docs/planning/analyzer-migration.md).

## Development

Use a source checkout with `uv`, Rust 1.96+, rustfmt, Clippy, and Make. Install the pinned dependency auditors, then run the full local gate:

```sh
uv sync --locked
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
```

`make check` covers Rust formatting, compilation, Clippy, tests, MSRV validation, dependency audits, strict Python typing and linting, identifier-table verification, package builds and clean installation checks, and the inspector's execution guard. GitHub Actions runs the Python checks on 3.11–3.14 alongside the Rust and dependency checks.

Run the repository example from the checkout:

```sh
uv run --locked typepact inspect-contracts --root . examples/contracts.py
```

See [CONTRIBUTING.md](https://github.com/eddiethedean/typepact/blob/main/CONTRIBUTING.md) for focused check targets and contribution guidance.

## Documentation

- [Full documentation](https://typepact.readthedocs.io/en/latest/): installation, quick start, adoption guidance, APIs, CLI, output, examples, security, and support.
- [Documentation source](https://github.com/eddiethedean/typepact/tree/main/docs): guides in the repository if the hosted site is unavailable.
- [Release and capability availability](docs/availability.md): what is published in 0.1.0 and what remains a 0.2.0 preview.
- [Inspector guide](https://github.com/eddiethedean/typepact/blob/main/docs/inspector.md): supported syntax, output, limits, and sessions.
- [Release roadmap](https://github.com/eddiethedean/typepact/blob/main/ROADMAP.md): the planned 0.1–0.9 sequence.
- [Product vision](https://github.com/eddiethedean/typepact/blob/main/docs/planning/vision.md): goals and intended use cases.
- [Limitations](https://github.com/eddiethedean/typepact/blob/main/docs/planning/limitations.md): analysis and assurance boundaries.
- [Planning index](https://github.com/eddiethedean/typepact/blob/main/docs/planning/README.md): architecture, decisions, and future feature specifications.

## License

TypePact is distributed under the [MIT license](https://github.com/eddiethedean/typepact/blob/main/LICENSE).

# Installation

Choose the published **0.3.0 release** for distributed packages, or build the
**0.4.0 source candidate** to evaluate adoption tooling. Candidate package
versions in this repository do not imply a published release. See
[availability](availability.md) for versions and platform evidence.

## Python helpers and matching CLI

In an existing uv project:

```sh
uv add codepact==0.3.0
uv run --locked codepact --version
```

Or install with pip:

```sh
python -m pip install 'codepact==0.3.0'
codepact --version
```

The `codepact` package pins `codepact-cli` to the same version. It contains pure
Python helpers, typing metadata, and `py.typed`. Importing those helpers does
not start the analyzer or download an executable:

```python
from codepact import resource
from codepact.rust import Borrowed, Owned
```

## Standalone command

Install only the native analyzer when Python helper imports are unnecessary:

```sh
uv tool install codepact-cli==0.3.0
codepact --version
```

Prebuilt CLI wheels contain a standalone Rust executable. They do not require
a Rust compiler or depend on a particular CPython ABI.

## Rust library

```sh
cargo add codepact@0.3.0
```

Use the [Rust API reference](rust-api.md) for one-shot checks and reusable sessions.
Source builds require Rust 1.96+, a linker, and the locked Cargo dependencies.

## Build the 0.4 candidate

With Python 3.11+, uv, and Rust 1.96+ installed:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
uv run --locked codepact capabilities --json
git rev-parse HEAD
```

This builds the local helper and CLI workspace. The version is `0.4.0`; record
the Git revision with reports to identify the candidate precisely. Continue
with [adoption](adopting.md) for project configuration and must-use results.

For a Rust-only build from that checkout:

```sh
cargo build --release --locked
./target/release/codepact --version
```

To reproduce the published release from source, check out `v0.3.0` before
building. Avoid mixing candidate helpers with the published CLI when evaluating
new contracts.

## Compatibility

| Surface | Support |
| --- | --- |
| Python helper runtime | Python 3.11+; CI covers CPython 3.11–3.14 |
| Analyzed source targets | Python 3.11, 3.12, and 3.13 |
| CLI wheels | Linux glibc/musl, macOS, and Windows on x86-64 and ARM64 |
| Source builds | Rust 1.96+ and a linker |

The [platform matrix](availability.md#platform-and-runtime-evidence) lists
minimum OS/runtime versions. The helper wheel is universal pure Python.
Running the helpers on Python 3.14 does not add analyzer support for Python
3.14 grammar; choose source grammar separately with `--python-version`.

## Verify and continue

```sh
uv run --locked codepact --version
uv run --locked python -c "from codepact import resource; from codepact.rust import Borrowed, Owned"
```

For a project using published dependencies, the command should print
`codepact 0.3.0`. Then follow the [quick start](quickstart.md). If an executable
or dependency cannot be found, use [troubleshooting](troubleshooting.md).

# Installation

Install **0.5.0** from PyPI or crates.io. Explicit moves, lexical borrowing,
and TaskGroup checks are available as an opt-in experiment.

| I need | Choose |
| --- | --- |
| Python annotations and the checker in my project | [Python helpers and matching CLI](#python-helpers-and-matching-cli) |
| A command for a source tree or CI job | [Standalone CLI](#standalone-command) |
| An embedded checker or reusable analysis session | [Rust library](#rust-library) |
| A reproducible checkout of the release | [Build from source](#build-from-source) |

[Published artifacts and platform support](availability.md)

## Python helpers and matching CLI

In an existing uv project:

```sh
uv add codepact==0.5.0
uv run --locked codepact --version
```

Or install with pip:

```sh
python -m pip install 'codepact==0.5.0'
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
uv tool install codepact-cli==0.5.0
codepact --version
```

Prebuilt CLI wheels contain a standalone Rust executable. They do not require
a Rust compiler or depend on a particular CPython ABI.

## Rust library

```sh
cargo add codepact@0.5.0
```

Use the [Rust API reference](rust-api.md) for one-shot checks and reusable sessions.
Source builds require Rust 1.96+, a linker, and the locked Cargo dependencies.

## Build from source

With Python 3.11+, uv, and Rust 1.96+ installed:

```sh
git clone --branch v0.5.0 https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
uv run --locked codepact capabilities --json
git rev-parse HEAD
```

This builds the helper and CLI workspace from the release tag. Record the
Git revision when sharing reports from a source build.
[Project configuration and must-use results](adopting.md)

For a Rust-only build from that checkout:

```sh
cargo build --release --locked
./target/release/codepact --version
```

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

For a project pinned to 0.5.0, the command should print
`codepact 0.5.0`. Then follow the [quick start](quickstart.md). If an executable
or dependency cannot be found, use [troubleshooting](troubleshooting.md).

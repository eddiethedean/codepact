# Installation

TypePact is distributed as two version-matched Python packages. The `typepact` package contains pure Python declaration helpers and depends on `typepact-cli`, which provides the Rust-powered `typepact` command.

## Install the package and command

Add TypePact to an application with [uv](https://docs.astral.sh/uv/):

```sh
uv add typepact==0.1.0
```

This installs both distributions at version 0.1.0. Your Python program can import the helper without starting the analyzer:

```python
from typepact import resource
from typepact.rust import Borrowed, Owned
```

The `typepact` command is available in the project environment:

```sh
uv run typepact --version
uv run typepact inspect-contracts app.py
```

## Install only the command

To use the analyzer without adding the Python helper to a project:

```sh
uv tool install typepact-cli==0.1.0
typepact --version
```

## Build requirements

Prebuilt CLI wheels include the executable and do not require a Rust compiler. The CLI source distribution requires Rust 1.96 or newer, a linker, and access to the locked Cargo dependencies. The helper package is pure Python in either wheel or source form.

Python 3.11 or newer is required to install either distribution. CLI wheels are available for Linux glibc and musl, macOS, and Windows on x86-64 and ARM64. The helper wheel is platform-independent. See [compatibility](supported-syntax.md#python-version-support) for the analyzer source-target versions.

## Platform compatibility

| Operating system | x86-64 minimum | ARM64 minimum |
| --- | --- | --- |
| Linux with glibc | glibc 2.17 | glibc 2.28 |
| Linux with musl | musl 1.2 | musl 1.2 |
| macOS | 10.13 | 11.0 |
| Windows | Native x86-64 wheel | Native ARM64 wheel |

CLI wheels contain a standalone executable and use platform tags without a CPython ABI dependency. Installing the helper's universal wheel still installs its native CLI dependency, so the combined installation requires a compatible CLI wheel or a working Rust source-build environment.

## Confirm the installation

```sh
uv run typepact --version
uv run python -c "from typepact import resource; from typepact.rust import Borrowed, Owned"
```

The command should print `typepact 0.1.0`; the Python import should exit successfully. The analyzer target is selected separately with `--python-version`. Running the helpers on Python 3.14 does not add analyzer support for the Python 3.14 grammar.

Continue with the [quick start](quickstart.md). If installation falls back to a source build or the command cannot be found, see [troubleshooting](troubleshooting.md).

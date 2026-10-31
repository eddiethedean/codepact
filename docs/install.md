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

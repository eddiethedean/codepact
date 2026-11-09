# Installation

CodePact 0.3.0 is published as `codepact` and `codepact-cli` on PyPI and as
`codepact` on crates.io. Install the Python helper and matching CLI with:

```sh
uv add codepact==0.3.0
uv run --locked codepact --version
```

The helper installs the matching CLI version automatically. To install only the
command, use:

```sh
uv tool install codepact-cli==0.3.0
codepact --version
```

To add the Rust library to a Cargo project, use `cargo add codepact@0.3.0`.
Python programs can import declarations without starting the analyzer:

```python
from codepact import resource
from codepact.rust import Borrowed, Owned
```

To run the checker in the uv project:

```sh
uv run --locked codepact inspect-contracts app.py
uv run --locked codepact check --root src
```

The helper wheel is platform-independent and requires Python 3.11 or newer.
Prebuilt CLI wheels include the executable and do not require a Rust compiler.
Source builds require Rust 1.96+, a linker, and access to the locked Cargo
dependencies. Published CLI wheels cover Linux glibc and musl, macOS,
and Windows on x86-64 and ARM64. See [compatibility](supported-syntax.md#python-version-support)
for the analyzer source targets.

## Build the checker from source

The CLI and Rust library share the same analyzer. From the repository root:

```sh
cargo run --locked -- check --root examples
```

The lifecycle example intentionally reports a use after close and exits `1`.
The [cleanup tutorial](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/cleanup-tutorial.md) demonstrates
the released exit-obligation diagnostics. Check out `v0.3.0` and record the full
Git commit when sharing source-build results.

## Confirm the installation

```sh
uv run --locked codepact --version
uv run --locked python -c "from codepact import resource; from codepact.rust import Borrowed, Owned"
```

The command should print `codepact 0.3.0`; the Python import should exit
successfully. The analyzer target is selected separately with
`--python-version`. Running the helpers on Python 3.14 does not add analyzer
support for Python 3.14 grammar.

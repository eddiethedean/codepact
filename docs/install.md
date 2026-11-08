# Installation

CodePact 0.3.0 is an unreleased candidate. The `codepact` and `codepact-cli`
Python distributions and the Rust `codepact` crate have not been published.
Build the candidate from a checkout:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
```

`uv sync` installs the pure Python helper and the native command from the local
workspace. Python programs can import declarations without starting the
analyzer:

```python
from codepact import resource
from codepact.rust import Borrowed, Owned
```

To run the candidate checker:

```sh
uv run --locked codepact inspect-contracts app.py
uv run --locked codepact check --root src
```

After the maintainer qualifies and publishes 0.3.0, install the Python helper
and matching CLI with:

```sh
uv add codepact==0.3.0
```

To install only the command after publication, use:

```sh
uv tool install codepact-cli==0.3.0
codepact --version
```

The helper wheel is platform-independent and requires Python 3.11 or newer.
Prebuilt CLI wheels include the executable and do not require a Rust compiler.
Source builds require Rust 1.96+, a linker, and access to the locked Cargo
dependencies. The candidate configures wheels for Linux glibc and musl, macOS,
and Windows on x86-64 and ARM64. See [compatibility](supported-syntax.md#python-version-support)
for the analyzer source targets.

## Build the checker from source

The CLI and Rust library share the same analyzer. From the repository root:

```sh
cargo run --locked -- check --root examples
```

The lifecycle example intentionally reports a use after close and exits `1`.
The [cleanup tutorial](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/cleanup-tutorial.md) demonstrates
the candidate's exit-obligation diagnostics. Record the full Git commit when
sharing candidate results.

## Confirm the installation

```sh
uv run --locked codepact --version
uv run --locked python -c "from codepact import resource; from codepact.rust import Borrowed, Owned"
```

The command should print `codepact 0.3.0`; the Python import should exit
successfully. The analyzer target is selected separately with
`--python-version`. Running the helpers on Python 3.14 does not add analyzer
support for Python 3.14 grammar.

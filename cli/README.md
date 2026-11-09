# CodePact CLI

`codepact-cli` distributes the Rust `codepact` command through Python packaging.
Installing `codepact` installs this matching CLI package automatically. The
CodePact 0.3.0 release is available on PyPI. The predecessor TypePact 0.2.0 release used `typepact-cli` and the
`typepact` command.

## Install and inspect

Install the command with `uv tool install codepact-cli==0.3.0`. To build from
the repository, use `uv sync --locked` followed by `uv run codepact`.

The 0.3.0 command provides declaration inspection and bounded lifecycle checking.
Use `--root src` with explicit source files to resolve supported local imports
for inspection. For `check`, `--root` scans Python files below the root. Exit
`0` can include possible findings or incomplete coverage; `1` reports definite
violations or inspection errors; `2` means invocation or input failure.

## Lifecycle checking

CodePact 0.3.0 includes bounded cleanup checking through `codepact check`. To
build it from the repository root, use Rust 1.96+:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
git checkout v0.3.0
cargo run --locked -- check --root examples
```

For a reproducible source build, use the release tag in the
[0.3.0 release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md).
The lifecycle example intentionally reports a use after close and exits `1`.
Checking scans Python files below its root, reports readable diagnostics by default,
and accepts `--json`. Possible findings and coverage warnings can accompany exit `0`.
The historical TypePact 0.2.0 CLI does not include the CodePact 0.3 cleanup and
cancellation behavior. The 0.3.0 release's qualification is recorded in the
[release evidence](../docs/releases/0.3.0-evidence.md).

## Requirements and reference

Platform wheels include the executable; source installs require Rust 1.96+ and a linker.
The analyzer reads Python source without importing or executing it.

The release matrix covers Linux glibc/musl, macOS and Windows on x86-64 and ARM64.
Python 3.11+ is required for installation. The CodePact `codepact` Rust crate
is published on crates.io; the older TypePact crate belongs to the historical
0.1.0 and 0.2.0 releases.

See the [installation guide](https://typepact.readthedocs.io/en/latest/install.html),
[command reference](https://typepact.readthedocs.io/en/latest/cli.html), and
[lifecycle tutorial](https://typepact.readthedocs.io/en/latest/lifecycle.html).
The source for all guides is in the [repository docs directory](https://github.com/eddiethedean/codepact/tree/main/docs).

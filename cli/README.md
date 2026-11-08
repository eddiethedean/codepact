# CodePact CLI

`codepact-cli` distributes the Rust `codepact` command through Python packaging.
Installing `codepact` installs this matching CLI package automatically. The
CodePact 0.3.0 candidate is being qualified; neither new distribution has been
published. The predecessor TypePact 0.2.0 release used `typepact-cli` and the
`typepact` command.

## Install and inspect

After the maintainer qualifies and publishes 0.3.0, install the command with
`uv tool install codepact-cli==0.3.0`. To run the candidate now, clone this
repository and use `uv sync --locked` followed by `uv run codepact`.

The published 0.1.0 command writes JSON describing resource and access declarations.
Use `--root src` with explicit source files to resolve supported local imports.
Exit `0` means complete inspection without errors, `1` means declaration errors or
incomplete coverage, and `2` means invocation or input failure.

## Lifecycle checking preview

The source preview for the upcoming 0.3.0 release includes expanded cleanup
checking through `codepact check`. Build it from the repository root with Rust
1.96+:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
git checkout main
cargo run --locked -- check --root examples
```

For a reproducible preview, use the candidate commit in the
[0.3.0 release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md).
The lifecycle example intentionally reports a use after close and exits `1`.
Checking scans Python files below its root, reports readable diagnostics by default,
and accepts `--json`. Possible findings and coverage warnings can accompany exit `0`.
The historical TypePact 0.2.0 CLI does not include the CodePact 0.3 cleanup and
cancellation behavior. Release gates for CodePact 0.3.0 remain pending.

## Requirements and reference

Platform wheels include the executable; source installs require Rust 1.96+ and a linker.
The analyzer reads Python source without importing or executing it.

The release matrix covers Linux glibc/musl, macOS and Windows on x86-64 and ARM64.
Python 3.11+ is required for installation. The Rust crate remains available on crates.io.

See the [installation guide](https://typepact.readthedocs.io/en/latest/install.html),
[command reference](https://typepact.readthedocs.io/en/latest/cli.html), and
[lifecycle tutorial](https://typepact.readthedocs.io/en/latest/lifecycle.html).
The source for all guides is in the [repository docs directory](https://github.com/eddiethedean/codepact/tree/main/docs).

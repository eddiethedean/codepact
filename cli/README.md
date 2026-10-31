# TypePact CLI

`typepact-cli` distributes the Rust `typepact` command through Python packaging.
Installing `typepact` installs this matching CLI package automatically.

## Install and inspect

```sh
uv tool install typepact-cli==0.1.0
typepact inspect-contracts your_file.py
```

The published 0.1.0 command writes JSON describing resource and access declarations.
Use `--root src` with explicit source files to resolve supported local imports.
Exit `0` means complete inspection without errors, `1` means declaration errors or
incomplete coverage, and `2` means invocation or input failure.

## Lifecycle checking preview

The repository's `main` branch also includes the unreleased `typepact check`
command. Build it from the repository root with Rust 1.96+:

```sh
cargo run --locked -- check --root examples
```

The lifecycle example intentionally reports a use after close and exits `1`.
Checking scans Python files below its root, reports readable diagnostics by default,
and accepts `--json`. Possible findings and coverage warnings can accompany exit `0`.
The published 0.1.0 CLI does not include this command.

## Requirements and reference

Platform wheels include the executable; source installs require Rust 1.96+ and a linker.
The analyzer reads Python source without importing or executing it.

The release matrix covers Linux glibc/musl, macOS and Windows on x86-64 and ARM64.
Python 3.11+ is required for installation. The Rust crate remains available on crates.io.

See the [installation guide](https://typepact.readthedocs.io/en/latest/install.html),
[command reference](https://typepact.readthedocs.io/en/latest/cli.html), and
[lifecycle tutorial](https://typepact.readthedocs.io/en/latest/lifecycle.html).

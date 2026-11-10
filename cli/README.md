# CodePact CLI

`codepact-cli` distributes the Rust `codepact` command through Python packaging.
Installing `codepact` installs this matching CLI package automatically. The
CodePact 0.4.0 synthetic preview is available on PyPI.
[Published CLI](https://pypi.org/project/codepact-cli/0.4.0/)

## Install and inspect

Install the command with `uv tool install codepact-cli==0.4.0`. To build from
the repository, use `uv sync --locked` followed by `uv run codepact`.

The 0.4.0 command adds project configuration, baselines, suppressions, and
synchronous must-use checks to bounded lifecycle checking. For `check`,
`--root` scans Python files below the root. Exit `0` means the report passed the
selected policy, `1` means it failed policy, and `2` means the check could not
complete. Advisory policy can pass with incomplete coverage. Inspection uses
explicit source files and reports its own status.
[Command reference](https://typepact.readthedocs.io/en/latest/cli.html)

## Lifecycle checking

Build the released command with Rust 1.96+:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
git checkout v0.4.0
cargo run --locked -- check --root examples
```

The lifecycle example intentionally reports a use after close and exits `1`.
Use `--json` to capture the schema 2 check report. Real-world usefulness of
the synthetic preview remains unqualified. [Verified release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.4.0-evidence.md)

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

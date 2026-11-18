# CodePact CLI

`codepact-cli` installs the native `codepact` command for static analysis of
Python resource contracts. Use it to check supported use after close, missed
cleanup, and discarded must-use results. The checker reads Python source
without importing or running the project.

## Install 0.8.0

```sh
uv tool install codepact-cli==0.8.0
codepact --version
```

If your project also needs annotations or typed outcome helpers, install
`codepact==0.8.0` in its Python environment. To install the helper and matching
CLI together, use `codepact[cli]==0.8.0`. The helper alone does not install the
native command.
[Installation and platform support](https://codepact.readthedocs.io/en/latest/install.html)

## Check a source tree

```sh
codepact check --root src
codepact check --root src --mode strict --json > codepact-report.json
```

Use a root that contains the source and its local contract providers. Exit
`0` means the report passes the selected policy, `1` means a completed report
fails policy, and `2` means the command could not complete. Advisory mode can
pass with incomplete coverage. Strict mode also fails on incomplete coverage
and unsuppressed possible violations.
[Quick start](https://codepact.readthedocs.io/en/latest/quickstart.html) ·
[CLI reference](https://codepact.readthedocs.io/en/latest/cli.html) ·
[CI reports](https://codepact.readthedocs.io/en/latest/ci-reporting.html)

## Compatibility

Installation requires Python 3.11 or later. The CLI analyzes Python 3.11,
3.12, and 3.13 source targets. Published wheels bundle the Rust executable
for supported Linux, macOS, and Windows platforms. Building from source
requires Rust 1.96 or later and a linker.
[Release artifacts](https://codepact.readthedocs.io/en/latest/availability.html)

CodePact 0.8.0 adds `codepact contracts export` and `codepact contracts verify`
for wheel publishers. Configure installed dependency roots or a reviewed
runtime profile to use the new models and get report schema 4.
[Reviewed contracts and runtime models](https://codepact.readthedocs.io/en/latest/reviewed-contracts.html) ·
[Source](https://github.com/eddiethedean/codepact)

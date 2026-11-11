# CodePact CLI

`codepact-cli` installs the native `codepact` command for static analysis of
Python resource contracts. Use it to check supported use after close, missed
cleanup, and discarded must-use results. The checker reads Python source
without importing or running the project.

## Install the published preview

```sh
uv tool install codepact-cli==0.4.0
codepact --version
```

If your project also needs annotations such as `resource`, `Owned`, and
`Borrowed`, install `codepact==0.4.0` in its Python environment. The helper
package installs the matching CLI automatically.
[Installation and platform support](https://github.com/eddiethedean/codepact/blob/main/docs/install.md)

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
[Quick start](https://github.com/eddiethedean/codepact/blob/main/docs/quickstart.md) ·
[CLI reference](https://github.com/eddiethedean/codepact/blob/main/docs/cli.md) ·
[CI reports](https://github.com/eddiethedean/codepact/blob/main/docs/ci-reporting.md)

## Compatibility

Installation requires Python 3.11 or later. The CLI analyzes Python 3.11,
3.12, and 3.13 source targets. Published wheels bundle the Rust executable
for supported Linux, macOS, and Windows platforms. Building from source
requires Rust 1.96 or later and a linker.
[Release artifacts](https://github.com/eddiethedean/codepact/blob/main/docs/availability.md)

The 0.5 development candidate adds opt-in explicit moves, lexical borrows,
and TaskGroup checks. Build it from source with matching helper and CLI
versions. [Experimental ownership](https://github.com/eddiethedean/codepact/blob/main/docs/ownership.md) ·
[Source](https://github.com/eddiethedean/codepact)

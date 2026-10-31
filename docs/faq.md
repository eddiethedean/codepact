# Frequently asked questions

## Which version can I install now?

Version 0.1.0 is the currently published release. It provides declaration
inspection through `typepact inspect-contracts`. Version 0.2.0 is being prepared;
the `check` command is currently available only from source. See
[availability](availability.md).

## Does TypePact import or run my application?

No. The analyzer parses the selected Python source as text. It does not import
the application, evaluate annotations, or invoke decorators. The Python
`resource` decorator itself attaches metadata when your application imports it;
TypePact's CLI does not import that application.

## Does a clean result prove my resources are safe?

No. Contract facts have `declared-unverified` provenance. The lifecycle preview
checks a bounded synchronous subset. Neither mode proves that a method releases
a runtime resource, that cleanup occurs on every exit, or that ownership is
generally safe.

## Why does the command exit successfully when I see a warning?

For `check`, possible findings and incomplete-coverage warnings can accompany
exit status `0`. Read `diagnostics` and `coverage.status`; use the policy example
in [report handling](ci-reporting.md) if CI requires complete coverage.

## What does `--root` do?

For `inspect-contracts`, it assigns logical module names to the explicit files;
it does not scan the root. For `check`, it scans `.py` files below the root.
Explicit check filenames add files to that scan and do not restrict it. See the
[CLI reference](cli.md) for both command-specific input rules.

## Why are my imports or annotations unresolved?

The analyzer supports a limited set of syntax and import forms. In 0.1.0,
relative imports, re-export chains, cycles, future/string annotations, and
dynamic expressions are outside the supported inspector subset. The 0.2 preview
has additional lifecycle-flow limits. Read [supported syntax](supported-syntax.md)
and [limitations](limitations.md), then inspect `coverage.reasons`.

## Do I need Rust to install the released command?

No, if your platform has a compatible prebuilt CLI wheel. Building the CLI from
source requires Rust 1.96 or newer and a linker. See [installation](install.md).

## Which Python version is analyzed?

The analyzer currently accepts source targets 3.11, 3.12, and 3.13. This is
separate from the interpreter running the command, and the default target is
3.11. Helper runtime support begins at Python 3.11.

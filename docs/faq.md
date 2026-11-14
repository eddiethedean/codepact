# Frequently asked questions

## Which version can I install now?

Install CodePact 0.5.1 from PyPI or crates.io. Its default scope includes
project policy, baselines, and synchronous must-use checks. Enable experimental
ownership to try moves, lexical borrows, and TaskGroup checks. [Install CodePact](install.md) ·
[Experimental ownership](ownership.md)

## Does CodePact import or run my application?

No. The analyzer parses the selected Python source as text. It does not import
the application, evaluate annotations, or invoke decorators. The Python
`resource` decorator itself attaches metadata when your application imports it;
CodePact's CLI does not import that application.

## Does a clean result prove my resources are safe?

No. Contract facts have `declared-unverified` provenance. CodePact checks a
bounded subset of lifecycle and cleanup flow. Neither mode
proves that a method releases a runtime resource, that cleanup occurs on every
possible exit, or that ownership is generally safe.

## Why does the command exit successfully when I see a warning?

Warnings below the configured threshold can accompany a passing check.
Strict mode additionally fails on unsuppressed possible findings and
incomplete coverage. Use `--fail-on warning` if active definite warnings
should fail too. [Exit-policy matrix](cli.md#policy-and-exits) Read `execution_status`,
`diagnostics`, `coverage.status`, and `exit_code` in the report. See
[report handling](ci-reporting.md).

## What does `--root` do?

For `inspect-contracts`, it assigns logical module names to the explicit files;
it does not scan the root. For `check`, it scans `.py` files below the root.
Explicit check filenames add files to that scan and do not restrict it. See the
[CLI reference](cli.md) for both command-specific input rules.

## Why are my imports or annotations unresolved?

Check that the provider files are included and that their module names match
the imports. Then read `coverage.reasons` for the specific boundary, such as a
cycle, string annotation, or unsupported dynamic expression. The inspector
and checker resolve different subsets of syntax.
[Multi-file example](project-guide.md) · [Supported syntax](supported-syntax.md)

## Will I need Rust to install the CodePact command?

The CLI distribution provides native CLI wheels for supported platforms, which do
not need Rust at installation. The published wheels and source archives passed
release checks described in the [availability guide](availability.md).
Building from source requires Rust 1.96 or newer and a linker. See
[installation](install.md).

## What does TP3001 mean?

The 0.5.0 CLI emits TP3001 when a resolved synchronous function or method
declared with `@must_use`, `MustUse[T]`, or `Annotated[..., MustUseResult()]`
has its result discarded as a bare expression. Assignment and passing the
result count as immediate handling, not proof of eventual use. Async/dynamic
consumption and unsupported nested expressions remain boundaries. See
[declaring contracts](contracts.md#mark-a-must-use-return).

## Which Python version is analyzed?

The analyzer currently accepts source targets 3.11, 3.12, and 3.13. This is
separate from the interpreter running the command, and the default target is
3.11. Helper runtime support begins at Python 3.11.

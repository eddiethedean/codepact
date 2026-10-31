# Troubleshooting

Start with the process exit status. Exit `1` normally includes a JSON report; exit `2` usually includes an explanation on standard error. For a JSON report, read both `diagnostics` and `coverage`.

## The command is unavailable after installation

For a project installation, run `uv run typepact --version` from the project directory. For a standalone installation, use `uv tool install typepact-cli==0.1.0` and check that the uv tool directory is on your shell's PATH.

Both distributions require Python 3.11+. A CLI source build requires Rust 1.96+ and a linker; a compatible platform wheel includes the executable. See [installation](install.md) for the platform matrix.

## Coverage is incomplete even though there are no errors

Unsupported constructs produce `TP9001` warnings and incomplete coverage. A warning can therefore make the command exit with status `1`. Check `coverage.reasons` as well as diagnostic severity. Typical boundaries include forward annotations, relative imports, decorated functions, and computed contract expressions.

Complete coverage describes the implemented declaration subset. It does not imply that function bodies were checked for resource safety.

## A contract is nested inside a container

TypePact reports incomplete coverage for annotations such as `list[Borrowed[Connection]]`, including nested uses of captured resource aliases. Container/field alias analysis belongs to later phases. Use direct parameter and return contracts within the supported 0.1 subset; keep container declarations accurate and treat the diagnostic as an analysis boundary.

Literal `None` parameters and returns are ordinary non-resource annotations and do not cause incomplete coverage. A wrapper such as `Owned[None]` still lacks a resolvable resource class. String annotations such as `"None"` and future annotations remain unsupported.

## An imported class cannot be resolved

Pass its defining module and the consuming module in the same invocation. Set `--root` so the derived module names agree with the absolute imports. TypePact does not search installed packages or follow imports on disk.

Use the [multi-file tutorial](project-guide.md) as a reference. Relative imports, import re-export chains, and supplied-module cycles are unsupported in 0.1 even when all files are supplied.

## Multiple files map to the same module

Without a root, two files such as `one/models.py` and `two/models.py` both map to `models`. Set `--root` to a common source directory so their logical names include their parent packages. Each file must map to a distinct valid Python module name.

## A file is outside the root

Every input must resolve inside `--root`. Paths are canonicalized, so a symbolic link can resolve outside the root even when its visible path is underneath it. Choose a root containing the intended inputs and pass file paths relative to the working directory.

## Resource metadata conflicts

`TP4002` identifies an invalid declaration or incompatible lifecycle/access metadata. Inspect the primary location and any related spans. Examples include two different `closed_by` methods or a shared-access alias combined with an explicit owned-access qualifier. Use a single compatible declaration for each view.

## The source exceeds a limit

The source budget is 1,000,000 UTF-8 bytes for the whole request as well as each file. The inspector also limits file count, statements, expressions, import edges, and emitted contracts. CLI file-loading limits can produce exit `2`; semantic work limits can produce a JSON report with incomplete coverage. See the [limits table](supported-syntax.md#input-and-work-limits).

## Report a problem

Open a [GitHub issue](https://github.com/eddiethedean/typepact/issues) with the TypePact version, operating system and architecture, selected source target, invocation, exit status, and a small source example. Include the diagnostics and coverage fields when available. Replace private application data before sharing the reproduction.

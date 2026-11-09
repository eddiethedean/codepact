# Command-line reference

The `codepact` command inspects declarations or checks resource lifecycles in Python source without importing or executing the project.

The published TypePact 0.2.0 release used the `typepact` command. CodePact 0.3.0
publishes the renamed `codepact` command; install it using the
[installation instructions](install.md). See the
[availability guide](availability.md) and [lifecycle tutorial](lifecycle.md).

## Commands

```text
codepact --version
codepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] [--json] FILE...
codepact check [--root DIR] [--python-version 3.11|3.12|3.13] [--json] [FILE...]
```

`inspect-contracts` requires explicit files and writes JSON. `check` discovers Python files under its root, defaults to readable diagnostics and accepts `--json` for the structured inspection result with lifecycle findings. Use `--root` to bound discovery; without it, the current directory is the root. Explicit files are included alongside discovered files. `--help` describes the invocation.

## Check resource lifecycles

```sh
codepact check --root src
codepact check --root src --python-version 3.12 src/app.py
```

The published TypePact 0.2 subset tracked direct resource construction, local
aliases, straight-line synchronous code, `if`/`else`, returns, resolved local
helpers, and acyclic named re-exports. It reported use after close as TP1001
and close through a `Borrowed` parameter as TP2001. The CodePact 0.3 candidate
extends this subset as described below.

Checking is bounded static analysis, not a guarantee of runtime safety, leak
freedom or cleanup on every Python path. Dynamic dispatch, arbitrary escaped
resource storage and unsupported expressions retain visible boundaries. Read
the [limitations](limitations.md) before treating a clean result as a proof.

## Select the Python grammar

The default source target is Python 3.11. Select Python 3.11, 3.12, or 3.13 with `--python-version`:

```sh
codepact inspect-contracts --python-version 3.13 app.py
```

This controls which Python grammar and identifier rules are used to parse the input. It does not change the Python interpreter running your application. Unsupported target versions produce incomplete coverage.

## Name modules with `--root`

For source files inside a package or `src` directory, set a root and pass every file that should be analyzed:

```sh
codepact inspect-contracts --root src src/models.py src/client.py
```

With a root, paths below it form the logical module name. For example, `src/my_app/models.py` under `src` becomes `my_app.models`. Without a root, the filename stem is used as the module name. Input files must map to distinct module names.

For `inspect-contracts`, `--root` only names the explicitly supplied modules. `check` discovers `.py` files under the root in sorted order and resolves supported direct absolute imports and acyclic named re-exports among them. Relative imports and star imports remain unsupported.

The checker scans the entire root even when filenames are supplied. It skips
symlinks and directories named `.git`, `.venv`, `venv`, `target`, `build`, `dist`,
and `__pycache__`. Explicit inputs must remain inside the root after path
resolution. Duplicate physical inputs are deduplicated; files mapping to the same
logical module produce an input error. Discovery does not search installed
packages. An empty root with no Python inputs is an input error.

Use a narrow root for a small supported source set. Passing one entry file does
not currently provide dependency-only discovery.

## Exit status

| Status | Meaning |
| --- | --- |
| `0` | Inspection has no errors and complete coverage; check has no definite lifecycle or declaration errors (possible findings/boundaries may remain) |
| `1` | Inspection has declaration errors or incomplete coverage; check has a definite lifecycle violation or declaration error |
| `2` | Invocation or file input failed, or a fatal input or execution limit prevented inspection |

For `check`, status `1` means a definite lifecycle violation or declaration error was found; possible findings and bounded coverage warnings alone return `0`. Status `0` does not establish safety outside the supported subset. See [result fields](output.md) for machine-readable coverage and diagnostic details.

## CodePact 0.3.0 behavior

CodePact 0.3.0 extends `check` to bounded cleanup obligations across
returns, raises, supported exception handlers, loops, `finally`, exact
`contextlib.closing`/`aclosing` imports, and resolved local coroutine calls
that are directly awaited once. Each awaited call is matched to its own
coroutine; nested calls that are not awaited remain pending and do not apply
cleanup. Repeated calls to one resolved factory site keep distinct bounded
allocation identities. Finite tuple loops are unrolled up to 16 items; each
flow state retains up to 128 pending coroutine records. It prints
completion, certainty, trust, coverage reason and repair guidance. With
`--json`, it emits the separately versioned `check_schema_version: 1` result
described in the [output reference](output.md); `inspect-contracts` keeps its
inspection schema. The release qualification is recorded in the [0.3
cleanup plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3-cleanup.md).
It does not provide a general lifetime, escape, or task-completion proof.

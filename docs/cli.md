# Command-line reference

The `typepact` command inspects declarations or checks resource lifecycles in Python source without importing or executing the project.

## Commands

```text
typepact --version
typepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] [--json] FILE...
typepact check [--root DIR] [--python-version 3.11|3.12|3.13] [--json] [FILE...]
```

`inspect-contracts` requires explicit files and writes JSON. `check` discovers Python files under its root, defaults to readable diagnostics and accepts `--json` for the structured inspection result with lifecycle findings. Use `--root` to bound discovery; without it, the current directory is the root. Explicit files are included alongside discovered files. `--help` describes the invocation.

## Check resource lifecycles

```sh
typepact check --root src
typepact check --root src --python-version 3.12 src/app.py
```

The current checking subset tracks direct resource construction, local aliases, straight-line synchronous code, `if`/`else`, returns, resolved local helpers and named imports. It reports use after a known close as TP1001 and close through a Borrowed parameter as TP2001. Unsupported statements and unknown effects are shown as incomplete coverage. A possible violation is a warning and does not by itself make the command fail.

This is bounded lifecycle checking, not a guarantee of runtime safety, leak freedom or cleanup across exceptions. Loops, exception handlers, context managers, dynamic dispatch and escaped resource storage are not modeled precisely; related resource facts are weakened and a coverage boundary is reported. Read the [limitations](limitations.md) before treating a clean result as a proof.

## Select the Python grammar

The default source target is Python 3.11. Select Python 3.11, 3.12, or 3.13 with `--python-version`:

```sh
typepact inspect-contracts --python-version 3.13 app.py
```

This controls which Python grammar and identifier rules are used to parse the input. It does not change the Python interpreter running your application. Unsupported target versions produce incomplete coverage.

## Name modules with `--root`

For source files inside a package or `src` directory, set a root and pass every file that should be analyzed:

```sh
typepact inspect-contracts --root src src/models.py src/client.py
```

With a root, paths below it form the logical module name. For example, `src/my_app/models.py` under `src` becomes `my_app.models`. Without a root, the filename stem is used as the module name. Input files must map to distinct module names.

For `inspect-contracts`, `--root` only names the explicitly supplied modules. `check` discovers `.py` files under the root in sorted order and resolves supported direct absolute imports among them. Relative imports and star imports remain unsupported.

## Exit status

| Status | Meaning |
| --- | --- |
| `0` | Inspection has no errors and complete coverage; check has no definite lifecycle or declaration errors (possible findings/boundaries may remain) |
| `1` | Inspection has declaration errors or incomplete coverage; check has a definite lifecycle violation or declaration error |
| `2` | Invocation or file input failed, or a fatal input or execution limit prevented inspection |

For `check`, status `1` means a definite lifecycle violation or declaration error was found; possible findings and bounded coverage warnings alone return `0`. Status `0` does not establish safety outside the supported subset. See [result fields](output.md) for machine-readable coverage and diagnostic details.

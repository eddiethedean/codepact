# Command-line reference

The `typepact` command inspects explicitly supplied Python source files and writes a JSON result to standard output.

## Commands

```text
typepact --version
typepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] [--json] FILE...
```

`--json` is accepted explicitly; JSON is already the default and only report format in 0.1. `--help` describes the invocation. At least one source file is required.

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

`--root` does not trigger directory traversal, file discovery, import following, or execution. Absolute imports between supplied files can resolve supported classes and direct assignment aliases. Relative imports, star imports, and re-export chains are unsupported.

## Exit status

| Status | Meaning |
| --- | --- |
| `0` | Inspection completed with no declaration errors and complete coverage for the supported subset |
| `1` | Syntax or declaration errors were found, or coverage is incomplete because source uses unsupported or unresolved constructs |
| `2` | Invocation or file input failed, or a fatal input or execution limit prevented inspection |

A status of 0 means the declared subset was inspected. It does not mean the code is resource-safe. See [result fields](output.md) for machine-readable coverage and diagnostic details.

# Run your first check

Use CodePact 0.5.0 to find a connection used after close, then fix the source
and confirm the repair. The analyzer reads the file as text.

## Set up an empty directory

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) and use
Python 3.11 or later. Open an empty directory and create the demo project.

<!-- smoke: setup -->
```sh
uv init --bare --name codepact-demo
uv add codepact==0.5.0
uv run --locked codepact --version
mkdir src
```

Expect `codepact 0.5.0`. In an existing uv project, skip `uv init` and create
`src` only if it is absent. For a disposable trial inside an existing project,
use the [standalone command](install.md#standalone-command) and an empty
directory. The [installation guide](install.md) also covers pip and PowerShell.

## Save the example

Save this file as `src/connection.py`. You can
{download}`download the complete file <../examples/lifecycle.py>` from this
documentation build.

```{literalinclude} ../examples/lifecycle.py
:language: python
:linenos:
:emphasize-lines: 22,23
```

`@resource` names the closing method. `Borrowed[Connection]` gives
`inspect_connection` shared access to an open connection and preserves
the underlying type for mypy and Pyright. The close method is a stub for this
exercise. [Contract declarations](contracts.md)

## Find and fix the bug

<!-- smoke: finding -->
```sh
uv run --locked codepact check --root src
```

<!-- smoke: finding-output -->
```text
connection.py:23:1: error TP1001: Borrowed call requires an open resource
  certainty: definite
  trust: declared-unverified
  repair: Keep the use before close, or guard it with a lifecycle check.
  related: connection.py:22:1
1 diagnostics; 1 actionable; 0 suppressed; 0 rule-disabled; 1 returned
```

The check exits `1`. The primary location is the call on line 23 and the
related location is the close on line 22. Delete line 23 and rerun in strict
mode.

<!-- smoke: repair -->
```sh
uv run --locked codepact check --root src --mode strict
```

The repaired check exits `0` and prints nothing. Use `echo $?` in a POSIX
shell or `$LASTEXITCODE` in PowerShell immediately after the command to see
the exit status.

## Keep going

- [Check multiple modules](project-guide.md) in the same `src/` directory.
- [Inspect declaration facts](inspector.md#try-declaration-inspection) with a
  separate declaration-only file and a passing result.
- [Evaluate an existing project](adopting.md#evaluate-one-package) before
  choosing a CI policy.

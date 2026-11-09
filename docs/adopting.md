# Evaluate CodePact in an existing project

Start with a small, reviewable source slice. CodePact does not import your
application or discover dependencies from installed packages. Inspection takes
explicit files; lifecycle checking scans Python files below a selected root.

## 1. Inspect declarations with the current candidate

CodePact 0.3.0 is not published. Clone the repository and install its locked
development environment, then point the checkout's CLI at your project:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact inspect-contracts --root /path/to/project/src \
  /path/to/project/src/data/connection.py /path/to/project/src/data/client.py
```

Use this first to check that resource decorators, access aliases, and direct
absolute imports resolve as expected. Read both `diagnostics` and
`coverage.reasons`; an unresolved import or unsupported declaration is a visible
boundary, not evidence that the declaration is safe.

## 2. Check whether lifecycle analysis fits

The 0.3 candidate checks use-after-close, closes through `Borrowed` parameters,
and owned cleanup obligations across a bounded set of synchronous and async
exits. Try it on a narrow directory containing only the relevant Python
modules:

```sh
uv run --locked codepact check --root /path/to/pilot
```

The checker scans every `.py` file under the selected root. A filename argument
adds an input; it does not restrict discovery to that file. It skips common
build/environment directories and symlinks, but it does not perform
entry-point-only dependency discovery. Use a root that avoids unrelated code,
generated sources, and unsupported frameworks.

## 3. Compare results with the source

For a useful pilot, select a few resources with a clear `closed_by` method and
direct synchronous uses. Review:

1. Whether the expected contract facts were emitted.
2. Whether each diagnostic points to the operation and related close location.
3. Whether `coverage.status` is `complete-for-supported-subset` for the pilot.
4. Whether the source actually closes the runtime resource as declared; CodePact
   does not execute or verify the method implementation.

A clean result covers only the supported subset and modeled exits. It does not
prove leak freedom, cleanup on every possible path, or general ownership safety.
Do not use an incomplete result as a clean bill of health.

## Fit checklist

The checker is most useful when the pilot uses direct resource construction,
local aliases, resolved helpers, `if`/`else`, supported `try`/`finally` and
loops, reviewed `contextlib` cleanup, or directly awaited local coroutines.
Common boundaries include future/string annotations, relative imports, dynamic
dispatch, arbitrary managers and awaitables, task scheduling, and resources
stored in fields or containers. See the full
[supported-syntax guide](supported-syntax.md) and [limitations](limitations.md)
before expanding the pilot.

If the pilot depends on these unsupported forms, keep the boundary visible and
wait for a documented capability that covers the required behavior. The
0.3.0 candidate has passed pre-tag qualification but is not published yet, and
CodePact does not provide a strict-mode switch, baseline file, or suppressions.

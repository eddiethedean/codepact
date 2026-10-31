# Evaluate TypePact in an existing project

Start with a small, reviewable source slice. TypePact does not import your
application or discover dependencies from installed packages. Its inspection
and lifecycle commands have different input rules, so choose the command based
on the release you can install.

## 1. Check the published release against declarations

The currently published 0.1.0 release inspects declarations in explicitly named
files. Install it and pass the files that define and use your resource type:

```sh
uv add typepact==0.1.0
uv run typepact inspect-contracts --root src src/data/connection.py src/data/client.py
```

Use this first to check that resource decorators, access aliases, and direct
absolute imports resolve as expected. Read both `diagnostics` and
`coverage.reasons`; an unresolved import or unsupported declaration is a visible
boundary, not evidence that the declaration is safe.

## 2. Check whether lifecycle preview fits

The upcoming 0.2.0 release is still being qualified. Its source preview can find
supported use-after-close operations and closes through `Borrowed` parameters.
Try it on a narrow directory that contains only the relevant Python modules:

```sh
git clone https://github.com/eddiethedean/typepact.git
cd typepact
git checkout 5da5d21b3638a91be590a66fe1b67d5ea2649843
cargo run --locked -- check --root /path/to/pilot
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
4. Whether the source actually closes the runtime resource as declared; TypePact
   does not execute or verify the method implementation.

A clean result covers only the supported subset and normal-flow model. It does
not prove leak freedom, cleanup through exceptions, or general ownership safety.
Do not use an incomplete result as a clean bill of health.

## Fit checklist

The current preview is most useful when the pilot uses direct resource
construction, local aliases, ordinary synchronous calls, local helpers, and
`if`/`else` branches. Common blockers include future/string annotations, relative
imports, dynamic dispatch, loops, exception handlers, context managers, async
code, and resources stored in fields or containers. See the full
[supported-syntax guide](supported-syntax.md) and [limitations](limitations.md)
before expanding the pilot.

If the pilot depends on these unsupported forms, keep the boundary visible and
wait for a documented capability that covers the required behavior. TypePact
does not provide a strict-mode switch, baseline file, or suppressions in this
release line.

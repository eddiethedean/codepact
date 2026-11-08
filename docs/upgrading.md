# Move from TypePact to CodePact 0.3

CodePact is the renamed project and 0.3.0 is the first candidate under the new
name. The 0.1.0 and 0.2.0 releases remain historical TypePact artifacts. The
CodePact candidate uses new package, import, command, and Rust crate names; it
does not provide compatibility aliases for the old names.

| Surface | TypePact 0.2.0 | CodePact 0.3.0 |
| --- | --- | --- |
| Python distribution/import | `typepact` | `codepact` |
| CLI distribution / command | `typepact-cli` / `typepact` | `codepact-cli` / `codepact` |
| Rust crate / import | `typepact` | `codepact` |
| GitHub repository | `eddiethedean/typepact` | `eddiethedean/codepact` |

## Future combined TypePact product

The [planned combined TypePact](https://github.com/eddiethedean/codepact/blob/main/docs/planning/typepact.md) will reuse the `typepact` command and distribution names for Pyrefly type checking plus CodePact behavioral checks. It is not implemented or released. Historical 0.1.0/0.2.0 artifacts retain their original meaning; a future combined release will use an explicit new version boundary and migration notes. Code declarations will continue to import `codepact`, with no restored `typepact` import aliases. Choose CodePact for behavioral checks or explicitly opt into the combined product when it is qualified.

## Update Python projects

After the maintainer qualifies and publishes 0.3.0, replace the package and
imports, then regenerate and commit the lockfile:

```sh
uv remove typepact
uv add codepact==0.3.0
uv sync
```

Change source imports, for example:

```python
from codepact import resource
from codepact.rust import Borrowed, Owned
```

The matching `codepact-cli` dependency supplies the `codepact` command. To
install only that command after publication, use
`uv tool install codepact-cli==0.3.0`.

The candidate has not been published, so these registry commands are not
available yet. To try it now, clone the repository and run `uv sync --locked`
from its root.

## Update Rust projects

After crate publication, replace the dependency/import and rebuild:

```sh
cargo remove typepact
cargo add codepact@0.3.0
cargo build --locked
```

Rust code imports the new crate with `use codepact::{AnalysisSession, SourceFile};`.
For the current candidate, build from this repository with
`cargo build --locked -p codepact`.

## Review the 0.3 behavior change

The candidate adds TP1002 for cleanup duties left outstanding on modeled
fallthrough, return, raise, and loop exits. This can make a check command return
status `1` for code that passed the TypePact 0.2 lifecycle check. Possible
findings and incomplete-coverage boundaries remain visible with status `0`.
Review the [candidate notes](release-notes.md#030--candidate-not-released),
[cleanup specification](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/specification.md), and
[availability guide](availability.md) before enabling the new check in a
required build gate.

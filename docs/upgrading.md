# Upgrade CodePact

## Upgrade from 0.4 to 0.5

Update the helper and its matching CLI with `uv add codepact==0.5.0`, then
commit the updated lockfile. Standalone CLI and Rust users can install their
matching 0.5.0 distributions directly. [Installation](install.md)

Existing projects use the same default lifecycle and must-use scope.
Enable `experimental-ownership = true` to check explicit moves and lexical
borrows. Enable `assume-default-task-factory = true` separately for the
bounded TaskGroup model under the documented CPython assumptions.

- **Report consumers.** Schema 2 adds ownership rules TP2002–TP2006 and the
  assurance value `experimental-ownership-checking-for-supported-exit-flow`.
  Accept that value and the new rule metadata before enabling the opt-in.
- **Baselines.** Ownership scope and the selected TaskGroup model revision
  are part of matching. Review findings and regenerate the baseline under
  the new scope when you opt in. An earlier baseline cannot suppress
  experimental findings.
- **Rust integrations.** Use `check_with_ownership` or the matching
  `AnalysisSession` methods to select the experimental scope. Existing
  default entry points retain their result types.

[Experimental ownership](ownership.md) · [Report schemas](schemas.md) ·
[Rust API](rust-api.md)

## Upgrade from 0.3 to 0.4

CodePact 0.4.0 is published as a synthetic preview. Update the project
dependency with `uv add codepact==0.4.0` and commit the updated lockfile.
For standalone CLI or Rust installs, pin the 0.4.0 distribution.
[Installation](install.md)

| Surface | Published 0.3 | Published 0.4 |
| --- | --- | --- |
| Project settings | CLI arguments | Validated `[tool.codepact]` settings and CLI overrides |
| Result-use checking | Unavailable | `@must_use`, `MustUse[T]`, and TP3001 |
| Check policy | Fixed released behavior | Advisory/strict policy, severity, and failure threshold |
| Acknowledged findings | No baseline or inline policy | Explicit baseline updates and reasoned line suppressions |
| CLI check JSON | Checker schema 1 | Adoption report with `check_schema_version: 2` |
| Declaration inspection | Inspection schema 0 | Inspection schema 0 and preserved exit policy |
| Existing Rust `check` / `check_report` | Released rule set | Preserved rule set and result contracts |

Update report consumers before switching the CLI: validate schema versions and
read execution status, coverage, suppression metadata, and truncation. Start
with advisory policy, review the first report, then enable stricter conditions
as appropriate. TP3001 is a warning by default; `--fail-on warning` makes it
fail a check. Strict mode also fails on unsuppressed possible violations and
incomplete coverage.

The 0.4 analyzer targets Python source 3.11–3.13. Existing resource helpers
keep their meaning; new must-use contracts do not transfer ownership or discharge
cleanup. Baselines should be created only after reviewing findings. See the
[adoption workflow](adopting.md) and [schema transition](output.md).

The 0.4.0 release uses fingerprint algorithm 5. Algorithm 3/4 baselines
fail with exit 2 and require an explicit reviewed `baseline update`.
Changed resolved resource/cleanup contracts reopen findings.

Generate a new file, review its entries, then point the project configuration
at that file. An unsupported old baseline cannot be replaced in place through
`--replace`, because existing baseline documents are validated before update.

```sh
codepact baseline update --output baseline-v5.json --reason "Reviewed findings after fingerprint upgrade"
codepact check --baseline baseline-v5.json --json
```

Rust callers constructing `Diagnostic` or `CheckDiagnostic` literals must
initialize the new `semantic_identity` field. The analyzer supplies this
internal evidence; public JSON omits it. Generate baselines from fresh analysis:
deserializing a public report cannot restore the resolved facts, and baseline
creation then fails before writing a file.

## Move from TypePact to CodePact 0.3

CodePact is the renamed project and 0.3.0 is the first release under the new
name. The 0.1.0 and 0.2.0 releases remain historical TypePact artifacts. The
CodePact release uses new package, import, command, and Rust crate names; it
does not provide compatibility aliases for the old names.

| Surface | TypePact 0.2.0 | CodePact 0.3.0 |
| --- | --- | --- |
| Python distribution/import | `typepact` | `codepact` |
| CLI distribution / command | `typepact-cli` / `typepact` | `codepact-cli` / `codepact` |
| Rust crate / import | `typepact` | `codepact` |
| GitHub repository | `eddiethedean/typepact` | `eddiethedean/codepact` |

## Future combined TypePact product

The [planned combined TypePact](https://github.com/eddiethedean/codepact/blob/main/docs/planning/typepact.md) will reuse the `typepact` command and distribution names for Pyrefly type checking plus CodePact behavioral checks. It is not implemented or released. Code declarations will continue to import `codepact`. Migration and historical backward compatibility are outside the combined product plan. Choose CodePact for behavioral checks or explicitly opt into the combined product when it is qualified.

## Update Python projects

Replace the package and imports, then regenerate and commit the lockfile:

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
install only that command, use
`uv tool install codepact-cli==0.3.0`.

## Update Rust projects

Replace the dependency/import and rebuild:

```sh
cargo remove typepact
cargo add codepact@0.3.0
cargo build --locked
```

Rust code imports the new crate with `use codepact::{AnalysisSession, SourceFile};`.
To build the workspace source, run `cargo build --locked -p codepact`.

## Review the 0.3 behavior change

CodePact 0.3.0 adds TP1002 for cleanup duties left outstanding on modeled
fallthrough, return, raise, and loop exits. This can make a check command return
status `1` for code that passed the TypePact 0.2 lifecycle check. Possible
findings and incomplete-coverage boundaries remain visible with status `0`.
Review the [0.3.0 release notes](release-notes.md#030--published-2026-11-08),
[cleanup specification](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/specification.md), and
[availability guide](availability.md) before enabling the new check in a
required build gate.

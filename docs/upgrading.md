# Upgrade CodePact

(upgrade-010)=
## Upgrade to 0.10.0

:::{only} codepact_release

Update the helper and matching CLI together and refresh the lockfile.

```sh
uv add 'codepact[cli]==0.10.0'
uv run --locked codepact --version
```

For other installation routes, see [installation](install.md).

:::

:::{only} not codepact_release

From the 0.10 checkout run
`cargo build --locked -p codepact`, then verify
`./target/debug/codepact --version` prints `codepact 0.10.0`. Use that binary
for [the new runtime examples](concurrency.md). If testing packaged helpers,
build matching checkout wheels with `uv build --wheel --no-sources --all-packages`.

:::

Regenerate baselines and capture both review revisions with the same checker.

- Enable the reviewed runtime profile for queues and synchronization. Declare
  `spawn-main-script` relative to the analysis root to select direct-script
  spawn entry and worker effects.
- Use `async-guards` to select an exact asyncio lock for a supported resource
  method. Effective settings include those declarations.
- Rust callers building `RuntimeProfile` and `FoundationInputs` as struct
  literals can set `spawn_main_script` to `None` and `async_guards` to an
  empty vector for requests that use earlier profiles.
- Review readers can use the optional `protocol_contracts` array in snapshot
  schema 1 to inspect matched operation keys and the applicable protocol facts.

[Configure the new profiles](reviewed-contracts.md#queues-ipc-and-guards)

(upgrade-09)=
## Upgrade to 0.9.0

Update the helper and CLI together and commit the refreshed lockfile.

```sh
uv add 'codepact[cli]==0.9.0'
uv run --locked codepact --version
```

Use `uv add codepact==0.9.0` for a helper-only project,
`uv tool install codepact-cli==0.9.0` for a standalone command, or
`cargo add codepact@=0.9.0` for a Rust host. [Install options](install.md)

- **Outcome contracts.** Add `MustHandle` to selected `Result` and `Option` returns
  when the caller must explicitly acknowledge each delivered outcome.
  [Persistent outcome examples](contracts.md#acknowledge-a-persistent-outcome)
- **Declaration policy.** Start in preview mode for the APIs you want to check.
  Configuring declaration policy selects settings schema 3. Ordinary source-only
  checks still emit CLI report schema 3.
  [Declaration policy](adopting.md#require-a-declaration-on-a-selected-api)
- **Baselines and review.** Rerun the check and regenerate your baseline under
  the intended settings. Compare both revisions using captures from the same
  0.9.0 binary. [Change review](cli.md#compare-captured-source-analyses)
- **Installed providers.** Use the reviewed wheels and explicit runtime profile
  to enable the new HTTP and SQL models. Compare published wheel changes with
  `codepact contracts diff`.
  [Provider profiles](reviewed-contracts.md#reviewed-http-and-sql-profiles) ·
  [Wheel comparison](cli.md#compare-published-wheel-contracts)

(upgrade-08)=
## Upgrade to 0.8.0

Update the helper and CLI together and commit the refreshed lockfile.

```sh
uv add 'codepact[cli]==0.8.0'
uv run --locked codepact --version
```

Use `uv add codepact==0.8.0` for a helper-only project,
`uv tool install codepact-cli==0.8.0` for a standalone command, or
`cargo add codepact@=0.8.0` for a Rust host. [Install options](install.md)

- **Source-only checks.** CLI reports still use schema 3 and settings use
  schema 2 under the default configuration. Existing Rust source-only
  entry points retain their result versions.
- **Installed dependencies.** Explicit dependency roots now select
  foundations checks and CLI report schema 4. Existing marked inline
  providers still work. Review the selected provider and coverage after
  upgrading. A recorded but missing manifest or a stale manifest leaves
  its published facts unavailable.
- **New opt-ins.** Add project models or an explicit runtime profile when
  you need their effects. Update consumers for CLI report schema 4, Rust
  result schema 2, and settings schema 3 as applicable. JSON execution
  failures retain the schema 3 envelope.
- **Baselines.** CodePact 0.8.0 can read a 0.7.0 baseline, but the engine
  version change makes it incompatible. Its entries stay unmatched even
  with identical source and settings. Findings previously suppressed by the
  baseline can fail CI again. Review a fresh 0.8.0 report, then regenerate
  the baseline under the intended settings before switching CI.
  [Create or update a baseline](adopting.md#record-the-initial-findings)

[Reviewed contracts and runtime models](reviewed-contracts.md) ·
[Report schemas](schemas.md) · [Release notes](release-notes.md)

## Upgrade from 0.5 or 0.6 to 0.7

CodePact 0.7 separates the pure-Python helper from the native command. A
project that previously installed `codepact` to get both must add the optional
CLI extra:

```sh
uv add 'codepact[cli]==0.7.0'
uv sync --locked
uv run --locked codepact --version
```

Install `codepact==0.7.0` by itself when the application needs only annotation
or outcome helpers. Install `codepact-cli==0.7.0` separately for a command-only
environment. Keep the helper and CLI versions matched.

- **Typed outcomes.** `codepact` and `codepact.rust` export `Ok`, `Err`,
  `Some`, `Nothing`, `Result`, `Option`, nullable adapters, and sync/async
  exception adapters. These tags do not carry ownership or cleanup semantics.
- **Finite matches.** `TP5001` is an opt-in warning. It is off by default;
  enable it with `experimental-finite-matches = true` or the matching CLI flag.
  Unsupported patterns and resource payloads retain coverage boundaries.
- **Installed providers.** Configure explicit dependency roots and
  `required-providers` to inspect marked pure-Python wheel declarations. Local
  source and earlier roots retain import precedence.
- **Reports.** CLI completed reports and their failure envelopes use report
  schema 3; `CheckResult` remains schema 1. `--show-settings --json` uses
  settings schema 2. Update strict JSON consumers before upgrading. Existing
  baselines remain readable, but regenerate them after reviewing the changed
  report scope and fingerprints.

[0.7 specification](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.7/specification.md) ·
[Installation](install.md) · [Schemas](schemas.md)

## Upgrade from 0.4 to 0.5

Update the helper and its matching CLI with `uv add codepact==0.5.1`, then
commit the updated lockfile. Standalone CLI and Rust users can install their
matching 0.5.1 distributions directly. [Installation](install.md)

Existing projects use the same default lifecycle and must-use scope.
Enable `experimental-ownership = true` to check explicit moves and lexical
borrows. Enable `assume-default-task-factory = true` separately for the
bounded TaskGroup model under the documented CPython assumptions.

- **Report consumers.** Schema 2 adds ownership rules TP2002–TP2006 and the
  assurance value `experimental-ownership-checking-for-supported-exit-flow`.
  Accept that value and the new rule metadata before enabling the opt-in.
- **Baselines.** Matching includes the exact engine version, target,
  ownership scope, and TaskGroup model revision. Review and regenerate a
  0.4 baseline with 0.5.0 even if you keep the default scope. Generate another
  baseline if you opt into ownership or change the TaskGroup assumption.
  Keep the previous file until the new entries and diff are reviewed.
- **Rust integrations.** Use `check_with_ownership` or the matching
  `AnalysisSession` methods to select the experimental scope. Existing
  default entry points retain their result types.

[Experimental ownership](ownership.md) · [Report schemas](schemas.md) ·
[Rust API](rust-api.md)

## Upgrade from 0.3 to 0.4

CodePact 0.4.0 is published as a synthetic preview. Update the project
dependency with `uv add codepact==0.4.0` and commit the updated lockfile.
For standalone CLI or Rust installs, pin the 0.4.0 distribution.
[PyPI helper](https://pypi.org/project/codepact/0.4.0/) ·
[PyPI CLI](https://pypi.org/project/codepact-cli/0.4.0/) ·
[Rust crate](https://crates.io/crates/codepact/0.4.0)

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

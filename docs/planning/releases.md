# Packaging and release plan

The predecessor TypePact 0.1.0 and 0.2.0 releases were published and verified
under their original package and crate names. CodePact 0.3.0 is the first
release using the new names; its published artifacts and passed gates are in
the [0.3.0 release evidence](../releases/0.3.0-evidence.md). The 0.4.0 synthetic preview was published and verified on 2026-11-10. [0.4.0 release evidence](../releases/0.4.0-evidence.md)

See also the
[historical 0.1.0 evidence](../releases/0.1.0-evidence.md) and
[historical 0.2.0 evidence](../releases/0.2.0-evidence.md).

CodePact 0.5.1 is the current published release. Use its [evidence and publication record](../releases/0.5.1-evidence.md) as the worked example.

## Release authority

The accountable maintainer can validate and approve a release alone, including their own implementation, semantic decisions, models and evidence. No independent reviewer, second-person approval or review waiver is required. Additional review is optional. Follow the [maintainer authority policy](release-gates.md#maintainer-authority); required tests, artifact checks and explicit maintainer release authorization still apply.

## Artifacts

Deliver a pure Python `codepact` package built with Hatchling and an exact dependency on `codepact-cli` at the same version (D015's distribution model, renamed under D019). Maturin bin bindings build `codepact-cli` with the Rust command. Publish eight CLI platform wheels, one universal helper wheel and two complete source archives. Retain the reusable `codepact` Rust crate and optional Cargo CLI installation. The Ruff/Salsa migration must retain reproducible registry-compatible dependencies and documented Rust prerequisites; qualify these before selecting dependency versions.

Confirm ownership and availability of the new `codepact` and `codepact-cli`
PyPI projects and the `codepact` crates.io name before publication. Repository
access does not imply registry access. The MIT license selected under D007
applies to the new package names as well.

## Versioning

Use semantic versioning with explicit pre-1.0 instability notices. Initially coordinate Python package, CLI and engine versions to reduce skew. Version machine-readable output and persisted cache schemas independently.

Diagnostic codes remain stable once published; wording can improve. New default errors and stricter analysis can break CI even without API changes, so publish migration notes and consider preview rules before enabling them by default.

Document deprecation warnings, replacement paths and removal releases. A 1.0 release requires a stated support matrix, stable core semantics, reviewed limitations and reproducible release procedures.

## Release checklist

1. Confirm milestone gates, issue triage and license/registry ownership.
2. Freeze a release candidate and generate changelog from reviewed changes.
3. Run full correctness, interoperability, compatibility and artifact checks.
4. Build wheels/source artifacts from the tagged revision in controlled CI.
5. Install each artifact into clean environments; check imports, CLI and metadata.
6. Verify bundled source/license notices, dependency inventory and artifact hashes.
7. Publish with restricted CI credentials or registry trusted publishing where configured.
8. Verify registry downloads and announce actual supported features and limitations.
9. Record artifact checksums, build provenance and rollback instructions.

Publishing is a separate authorized release action. Creating these planning documents does not publish packages or reserve registry names.

Before tagging, verify the helper, CLI, crate, and docs agree on the version.
Replay first-run and CI commands using the candidate wheels. Check that release
notes describe defaults, opt-ins, and any report or baseline migration.
After publication, download and verify registry artifacts, smoke-test a clean
install, and verify the hosted revision, source-example downloads, and install
paths using the [docs publishing checklist](../documentation.md#deploy-a-documentation-revision).
Record the public URLs and exact revisions in the evidence record.

## Planned TypePact combined product

The [TypePact plan](typepact.md) supersedes the `codepact-pyrefly-cli` proposal. TypePact combines pinned embedded Pyrefly with the CodePact Rust engine, using proposed `typepact` and `typepact-cli` distributions and the `typepact` command. Its main package supplies the compatible CodePact helpers through an exact dependency; Python source continues to import `codepact`. D015's helper/standalone CLI dependency remains in force, including its additional binary download.

Qualify PF01–PF05 and PF-V01–PF-V08 for the new combined product. Migration and historical backward compatibility are not release requirements. The first proposed combined version is `0.4.0a1`, tagged `typepact-v0.4.0a1`, subject to checking published version history. Use a separate `release-typepact.yml` workflow and artifact verifier; manual candidate builds do not publish. Verify current ownership and trusted publishers for both TypePact projects against the current repository and workflow. No earlier publisher screenshot establishes the new identity.

The first alpha establishes composition. Releases claiming selected type-informed stages TC01–TC03 must additionally qualify their applicable TC-V01–TC-V06 gates, bridge schema/pins, experimental-mode status and enriched diagnostic evidence. Do not treat agreement with standalone CodePact as sufficient evidence for analysis using additional type facts. No typed-outcome ownership claim precedes the required core ownership/outcome gates.

Start with qualified Linux glibc x86-64, macOS ARM64 and Windows x86-64 CLI wheels, a universal main-package wheel and both complete source archives. Publish the qualified CodePact dependency, then TypePact CLI, then its main package. The core artifact inventory and `release.yml` retain their D015 contract. The combined product remains planned; this does not implement its workflow or authorize publication.

## Recovery and support

Never silently replace a published artifact. For a broken release, document the issue, yank where supported, publish a corrected version and give pinning guidance. Preserve source tags for auditability.

Treat analyzer crashes, unsound acceptance in supported scope, and destructive suggested fixes as urgent defects. State the available vulnerability-reporting route accurately; do not request sensitive details in a public issue or imply that private reporting is configured when it is not. Define support and backport scope before 1.0.

## Release communication

Every release note states added rules, behavior changes, support versions, known limitations, performance evidence and migration steps. Separate experimental features from stable ones in both CLI help and reference documentation.

## Tag-triggered GitHub workflow

The implemented workflow `.github/workflows/release.yml` validates stable tags of the exact form `v1.2.3`. Manual dispatch and `codex/release-*` branches build candidates without publishing. Eight matrix jobs build native `codepact-cli` wheels. The source job builds the helper wheel and both source archives and runs a Cargo dry run. Publication requires the reusable quality workflow, all builds, complete artifact inventory and matching version/metadata checks. All three publishers consume the same validated candidate revision and verified artifacts. `codepact-cli` publishes before `codepact` so the exact dependency is available. The TypePact 0.1.0 and 0.2.0 evidence records retain those completed releases; repeat this process for each CodePact candidate.

Before tagging a subsequent release:

- Set the root Python project, `cli/pyproject.toml`, and Rust crate version to the tag version without the leading `v`; pin `codepact-cli` to that exact version in the helper dependency and refresh `uv.lock`.
- Confirm the `PYPI_API_TOKEN` GitHub Actions secret and the `CARGO_REGISTRY_TOKEN` secret are configured. `PYPI_API_TOKEN` must be able to upload both `codepact-cli` and `codepact`; never put credential values in evidence or logs.
- Use the pinned build tools and committed `uv.lock`/`Cargo.lock`. Require the eight CLI platform wheels, one pure helper wheel and both source archives; validate version metadata and dependency pins.
- Complete exact-candidate release evidence, maintainer approval, local checks, and CI review. The [0.5.1 release evidence](../releases/0.5.1-evidence.md) records the latest completed release.

Push the version tag to trigger publication. Both PyPI publish jobs use `PYPI_API_TOKEN` with username `__token__`; the release action's attestations are disabled because PyPI attestations require Trusted Publishing. Keep the `pypi` environment on both jobs, with no protection rule requiring another person's approval. The Cargo token is exposed only to the crates.io publishing step. All publishers depend on validation; codepact publication also depends on successful codepact-cli publication. Publication across projects/registries is not atomic. If one succeeds and the other fails, rerun only the failed job after resolving the cause; published versions cannot be overwritten.

This workflow does not publish anything until a matching tag is pushed. Manual dispatch is build-only. The current Quality workflow is wired into release validation; a successful run on an unrelated revision is not candidate evidence.

## Analyzer dependency updates

A Ruff or Salsa update requires a pinned dependency record, license/notice review, MSRV and target-version checks, AST/resolution regressions, incremental parity/reuse evidence and clean Cargo packaging. Never infer a published crate can build from a successful workspace build with Git/path dependencies. Serialize only CodePact-owned public data; database-local IDs and cache schemas carry no public stability guarantee. Changes to external declaration identity require explicit schema and migration review.

## Required codepact-cli native wheel matrix (D015)

| Platform | Rust target | Runner | Minimum compatibility |
| --- | --- | --- | --- |
| Linux glibc x86-64 | x86_64-unknown-linux-gnu | ubuntu-24.04 | manylinux 2.17 |
| Linux glibc ARM64 | aarch64-unknown-linux-gnu | ubuntu-24.04-arm | manylinux 2.28 |
| Linux musl x86-64 | x86_64-unknown-linux-musl | ubuntu-24.04 | musllinux 1.2 |
| Linux musl ARM64 | aarch64-unknown-linux-musl | ubuntu-24.04-arm | musllinux 1.2 |
| macOS x86-64 | x86_64-apple-darwin | macos-15 (cross build) | macOS 10.13 |
| macOS ARM64 | aarch64-apple-darwin | macos-15 | macOS 11.0 |
| Windows x86-64 | x86_64-pc-windows-msvc | windows-2022 | win_amd64 |
| Windows ARM64 | aarch64-pc-windows-msvc | windows-11-arm | win_arm64 |

The build matrix follows [robin-sparkless release CI](https://github.com/eddiethedean/robin-sparkless/blob/main/.github/workflows/release.yml); its test topology is not copied. Linux builds use Zig for explicit libc compatibility. Maturin 1.9.6 and Rust 1.96.0 build the release wheels. Python tags are py3-none with platform tags, constrained by Requires-Python >=3.11; no per-CPython ABI rebuild is needed for a standalone executable. Python 3.11–3.14 helper compatibility remains covered by quality CI. The analyzer's supported source grammar is a distinct capability.

The codepact source archive contains only its Python helpers, py.typed, metadata and license; installing it resolves the matching CLI dependency. The codepact-cli source archive contains Rust sources, workspace/crate manifests, Cargo.lock, CLI metadata and license files. Only CLI source installs compile Rust locally and require Rust 1.96+, a linker and network/cache access to locked Cargo dependencies. CLI platform wheels contain the executable and require no Rust compiler at installation. Runtime imports never start the analyzer or download executables. A user may explicitly choose Cargo installation for Rust library/CLI use.

All later phases inherit this distribution contract. Changes to the matrix, minimum platform versions or bundling model must update D015, this plan, the workflow and artifact verifier together. Cross-platform builds remain unqualified until their candidate jobs succeed; a successful local macOS build does not establish another target's compatibility.


## Two-project PyPI publication

The TypePact 0.1.0 release used `PYPI_API_TOKEN`; its 0.2.0 release exercised
OIDC for the old `typepact` and `typepact-cli` projects. The CodePact release
workflow uses the `PYPI_API_TOKEN` GitHub Actions secret for both new PyPI
projects. PyPI project-scoped tokens authorize only one project, so the shared
secret must use a token with access to both projects. Keep the helper
dependency exact to prevent skew with the native analyzer. Standalone users
can install `codepact-cli` without the Python helpers.

The uv workspace contains the root helper project and cli project; workspace source mapping resolves the CLI locally during development. Standard published metadata uses the ordinary registry dependency codepact-cli==X.Y.Z. Updating versions requires changing all three manifests and the helper dependency together, then refreshing uv.lock. A partial publication leaves the candidate incomplete; resume only missing projects, preserving published bytes.

## Concurrent capability release claims

Follow the phase-mapped [CS01–CS07 gates](concurrency.md) and include concurrency evidence in the candidate record when applicable. Release notes must name the qualified async subset and model versions, cancellation/cleanup behavior, known task/process/thread boundaries and opt-in experimental status. Never describe lifetime/protocol checks as race freedom, deadlock freedom or generic multiprocessing safety. Any expansion to serialization, inherited handles or shared-memory semantics needs an accepted model proposal and compatibility evidence before publication.

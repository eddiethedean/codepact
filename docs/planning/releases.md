# Packaging and release plan

TypePact 0.1.0 has been published and verified. The next planned release is
0.2.0; its lifecycle preview is not release-qualified yet. See the
[0.1.0 evidence record](../releases/0.1.0-evidence.md) for completed release
results and the [0.2.0 release checklist](release-0.2.0-checklist.md) for the
current candidate process.

## Artifacts

Deliver a pure Python typepact package built with Hatchling and an exact dependency on typepact-cli at the same version (D015). Maturin bin bindings build typepact-cli with the Rust command. Publish eight CLI platform wheels, one universal helper wheel and two complete source archives. Retain the reusable typepact Rust crate and optional Cargo CLI installation. The Ruff/Salsa migration must retain reproducible registry-compatible dependencies and documented Rust prerequisites; qualify these before selecting dependency versions.

Confirm ownership and availability of PyPI and crates.io names before publication. Repository access does not imply registry access. Select and add a license before the first distributable release.

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

## Optional combined Pyrefly prototype

The [typepact-pyrefly-cli plan](typepact-pyrefly-cli.md) defines a third, optional distribution combining embedded Pyrefly type checking with the TypePact Rust engine. It has a distinct `typepact-pyrefly` executable, independent prerelease versions and a planned `release-pyrefly.yml` workflow using environment `pypi`. The PyPI trusted publisher is project `typepact-pyrefly-cli`, owner `eddiethedean`, repository `typepact`, workflow filename `release-pyrefly.yml`, environment `pypi`. A maintainer-supplied screenshot confirms this configuration on 2026-10-30; workflow implementation and successful OIDC publication remain outstanding.

Qualify the PF01–PF05 work and PF-V01–PF-V08 gates before releasing it. The first proposed version is `0.1.0a1`, tagged `typepact-pyrefly-v0.1.0a1`; manual candidate builds do not publish. Start with verified Linux glibc x86-64, macOS ARM64 and Windows x86-64 wheels plus a buildable source archive; additional targets need evidence. Use a separate artifact verifier and publishing job. The existing two-project version pins, artifact counts and `release.yml` publication retain their D015 contract. The combined prototype remains planned, and this document does not implement its workflow or authorize publication.

## Recovery and support

Never silently replace a published artifact. For a broken release, document the issue, yank where supported, publish a corrected version and give pinning guidance. Preserve source tags for auditability.

Treat analyzer crashes, unsound acceptance in supported scope, and destructive suggested fixes as urgent defects. State the available vulnerability-reporting route accurately; do not request sensitive details in a public issue or imply that private reporting is configured when it is not. Define support and backport scope before 1.0.

## Release communication

Every release note states added rules, behavior changes, support versions, known limitations, performance evidence and migration steps. Separate experimental features from stable ones in both CLI help and reference documentation.

## Tag-triggered GitHub workflow

The implemented workflow `.github/workflows/release.yml` validates stable tags of the exact form `v1.2.3`. Manual dispatch and `codex/release-*` branches build candidates without publishing. Eight matrix jobs build native `typepact-cli` wheels. The source job builds the helper wheel and both source archives and runs a Cargo dry run. Publication requires the reusable quality workflow, all builds, complete artifact inventory and matching version/metadata checks. All three publishers consume the same validated candidate revision and verified artifacts. `typepact-cli` publishes before `typepact` so the exact dependency is available. The 0.1.0 evidence records the successful first release; repeat this process for each candidate.

Before tagging a subsequent release:

- Set the root Python project, `cli/pyproject.toml`, and Rust crate version to the tag version without the leading `v`; pin `typepact-cli` to that exact version in the helper dependency and refresh `uv.lock`.
- Confirm the PyPI trusted publishers for both projects and the `CARGO_REGISTRY_TOKEN` GitHub secret are configured. Never put credential values in evidence or logs.
- Use the pinned build tools and committed `uv.lock`/`Cargo.lock`. Require the eight CLI platform wheels, one pure helper wheel and both source archives; validate version metadata and dependency pins.
- Complete the exact-candidate release evidence, maintainer approval, local checks, and CI review. Use the [0.2.0 release checklist](release-0.2.0-checklist.md) for the next tag.

Push the version tag to trigger publication. Both PyPI publish jobs use Trusted Publishing: grant job-scoped `id-token: write`, omit username/password inputs, and enable attestations. Keep the `pypi` environment on both jobs. The Cargo token is exposed only to the crates.io publishing step. All publishers depend on validation; typepact publication also depends on successful typepact-cli publication. Publication across projects/registries is not atomic. If one succeeds and the other fails, rerun only the failed job after resolving the cause; published versions cannot be overwritten.

This workflow does not publish anything until a matching tag is pushed. Manual dispatch is build-only. The current Quality workflow is wired into release validation; a successful run on an unrelated revision is not candidate evidence.

## Analyzer dependency updates

A Ruff or Salsa update requires a pinned dependency record, license/notice review, MSRV and target-version checks, AST/resolution regressions, incremental parity/reuse evidence and clean Cargo packaging. Never infer a published crate can build from a successful workspace build with Git/path dependencies. Serialize only TypePact-owned public data; database-local IDs and cache schemas carry no public stability guarantee. Changes to external declaration identity require explicit schema and migration review.

## Required typepact-cli native wheel matrix (D015)

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

The typepact source archive contains only its Python helpers, py.typed, metadata and license; installing it resolves the matching CLI dependency. The typepact-cli source archive contains Rust sources, workspace/crate manifests, Cargo.lock, CLI metadata and license files. Only CLI source installs compile Rust locally and require Rust 1.96+, a linker and network/cache access to locked Cargo dependencies. CLI platform wheels contain the executable and require no Rust compiler at installation. Runtime imports never start the analyzer or download executables. A user may explicitly choose Cargo installation for Rust library/CLI use.

All later phases inherit this distribution contract. Changes to the matrix, minimum platform versions or bundling model must update D015, this plan, the workflow and artifact verifier together. Cross-platform builds remain unqualified until their candidate jobs succeed; a successful local macOS build does not establish another target's compatibility.


## Two-project PyPI publication

The first 0.1.0 release used the repository secret PYPI_API_TOKEN because two pending trusted publishers for the same GitHub project could not be configured. Both projects now exist on PyPI and have separate trusted publishers for repository eddiethedean/typepact, workflow release.yml and environment pypi. The release workflow has returned to OIDC: each publishing job receives only the job-scoped id-token: write permission, omits username/password inputs and enables attestations. The publish-cli job selects only typepact_cli artifacts; publish-typepact selects only typepact artifacts and waits for successful CLI publication. Remove the unused bootstrap API token from GitHub and revoke it in PyPI after the next successful trusted-publishing release. Keep the pure helper dependency exact to prevent skew with the native analyzer. Standalone users can install typepact-cli without the Python helpers.

The uv workspace contains the root helper project and cli project; workspace source mapping resolves the CLI locally during development. Standard published metadata uses the ordinary registry dependency typepact-cli==X.Y.Z. Updating versions requires changing all three manifests and the helper dependency together, then refreshing uv.lock. A partial publication leaves the candidate incomplete; resume only missing projects, preserving published bytes.

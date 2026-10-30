# Packaging and release plan

## Artifacts

Deliver a pure Python typepact helper wheel/source archive and a separate reusable typepact Rust crate/CLI installed through Cargo. Bundled CLI wheels remain an optional later packaging decision. The Ruff/Salsa migration must retain reproducible registry-compatible dependencies and documented Rust prerequisites; qualify these before selecting dependency versions.

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

## Recovery and support

Never silently replace a published artifact. For a broken release, document the issue, yank where supported, publish a corrected version and give pinning guidance. Preserve source tags for auditability.

Treat analyzer crashes, unsound acceptance in supported scope, and destructive suggested fixes as urgent defects. Publish security reporting instructions only after a working private channel exists. Maintain at least the current stable release and define backport scope before 1.0.

## Release communication

Every release note states added rules, behavior changes, support versions, known limitations, performance evidence and migration steps. Separate experimental features from stable ones in both CLI help and reference documentation.

## Tag-triggered GitHub workflow

The workflow `.github/workflows/release.yml` runs on candidate version tags and validates the exact stable form `v1.2.3`. It builds the Python package, checks distribution metadata and matching versions, and runs `cargo publish --dry-run` before enabling either publisher.

Before tagging a release:

- Add a buildable root `pyproject.toml` with a configured build backend, a root `Cargo.toml` (package or workspace) containing the `typepact` crate, and a committed `Cargo.lock`.
- Set both package versions to the tag version without the leading `v`.
- Configure the GitHub `pypi` environment to match the PyPI trusted publisher for `eddiethedean/typepact`, workflow `release.yml`, environment `pypi`.
- Add the repository Actions secret `CARGO_REGISTRY_TOKEN` with permission to publish the `typepact` crate.
- Ensure the backend produces a PyPI-compatible wheel and source distribution. The initial workflow builds on Ubuntu; a native extension or bundled native CLI will need portable wheel builds for the supported platform matrix before release.

Push the version tag to trigger publication. PyPI uses OIDC with job-scoped `id-token: write`; no PyPI token is required. The Cargo token is exposed only to the crates.io publishing step. Both publishers depend on validation, but publication across registries is not atomic. If one succeeds and the other fails, rerun only the failed job after resolving the cause; published versions cannot be overwritten.

This workflow does not create package implementations or publish anything until a matching tag is pushed. Full future quality gates from the planning documents must be wired in as implementation lands.

## Analyzer dependency updates

A Ruff or Salsa update requires a pinned dependency record, license/notice review, MSRV and target-version checks, AST/resolution regressions, incremental parity/reuse evidence and clean Cargo packaging. Never infer a published crate can build from a successful workspace build with Git/path dependencies. Serialize only TypePact-owned public data; database-local IDs and cache schemas carry no public stability guarantee. Changes to external declaration identity require explicit schema and migration review.

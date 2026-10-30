# Packaging and release plan

## Artifacts

Target one Python distribution named typepact containing lightweight public APIs and a documented way to run the Rust analyzer. Prefer platform wheels with the CLI if the packaging spike validates this; provide source builds with explicit Rust prerequisites. Publish the reusable Rust engine separately only when its public API is useful and maintainable.

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

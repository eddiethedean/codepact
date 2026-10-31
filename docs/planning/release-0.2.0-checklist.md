# 0.2.0 release checklist

This checklist prepares the 0.2.0 candidate for the existing tag-triggered
release workflow. It does not grant release approval. The current source preview
is not release-qualified until its phase and common gates have evidence and the
maintainer records a decision.

## Prepare the candidate

- [ ] Complete L01–L06 and pre-publication gates G1–G7; attach results for the
  exact candidate commit to a 0.2.0 evidence record. Record G8 after publication
  and registry verification.
- [ ] Confirm the tag workflow accepts the evidence record. It checks every
  common gate G1–G7, each phase gate, the candidate commit, and an explicit
  maintainer approval before any build or publication job starts. Run
  `python scripts/check_release_evidence.py 0.2.0 --tag-commit "$(git rev-parse HEAD)"`
  locally first; the tag workflow repeats that check.
- [ ] Confirm the 0.1.0 regressions remain covered and the release scope,
  supported syntax, limitations, exit behavior, output schema, and examples
  match the candidate implementation.
- [ ] Set the same stable version `0.2.0` in the root Python project, CLI
  Python project, and Rust crate. Update the helper's exact CLI dependency and
  regenerate `uv.lock`.
- [ ] Update this docs site to describe 0.2.0 as published only after registry
  verification. Before tagging, keep the 0.2.0 preview label and the 0.1.0
  install instructions accurate.
- [ ] Run `uv run --locked make check`, wait for the corresponding Quality
  workflow on the candidate commit, and review the source and artifact diffs.
- [ ] Record the exact source candidate commit after qualification. Add the
  completed evidence record and maintainer approval in a docs-only follow-up
  commit, then tag that commit. The tag workflow confirms the source tree has
  not changed since the qualified candidate.
- [ ] Confirm the repository secrets and PyPI trusted publishers are configured
  without reading or exposing credential values. The release job uses PyPI OIDC
  and `CARGO_REGISTRY_TOKEN` for crates.io publication.
- [ ] Record the maintainer's release approval in the candidate evidence before
  pushing the tag.

## Build and publish

The release workflow validates that all package versions match and that a tag
matches `v0.2.0`. A push of that exact tag runs quality, builds the eight CLI
wheels, the helper wheel, both source archives, and a Cargo dry run, then checks
the complete artifact inventory before publishing. PyPI publication uses
trusted publishing; crates.io uses the configured registry token. The helper
publication waits for the CLI publication. A manual workflow dispatch builds
and validates artifacts but skips publication.

## Verify after publication

- [ ] Confirm the tag workflow's Quality and artifact-validation jobs passed.
- [ ] Confirm both PyPI distributions and the crates.io crate serve version
  0.2.0; compare published Python artifact hashes with the verified CI artifact.
- [ ] Install the published packages in a clean environment outside the source
  checkout. Check version/help, inspect a declaration, exercise lifecycle
  checking, and confirm inspected Python did not execute.
- [ ] Verify the versioned documentation build and links from the release page.
- [ ] Update the docs index, availability page, install examples, release notes,
  and Sphinx title to say 0.2.0 is published. Keep the source preview reference
  only if it remains useful for unreleased changes.
- [ ] Remove and revoke the unused bootstrap PyPI API token after the successful
  OIDC publication, as recorded in the 0.1.0 evidence.

## If publication stops part way

Do not force-move the tag or rerun the full publication blindly. PyPI does not
allow uploading the same files again, and some registries may already contain
the version. Inspect the status of each publish job and each registry, retain
the verified CI artifacts and logs, and document which distributions landed.
The repository does not currently have an automated partial-publication
recovery workflow. The maintainer must choose a registry-specific recovery
before resuming; do not publish a different artifact under the same version.

If all artifacts are published but a defect is found, follow the release plan's
patch-version policy and document the correction. Do not change the release
record or artifact hashes to make a failed candidate appear successful.

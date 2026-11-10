# Build and deploy the documentation

The public site is a Sphinx project rooted at `docs/index.md`, written in MyST
Markdown with a reStructuredText Python API reference. Local checks, GitHub
Actions, and Read the Docs use `docs/requirements.txt` for the docs dependencies.
Documentation builds import the lightweight helpers from `python/`; they do
not need to install or compile the Rust CLI.

## Build a clean site locally

Install uv, then run from the repository root:

```sh
make check-docs
```

The target validates JSON schema fixtures and eight live CLI documents, checks relative documentation links,
and rebuilds every public page with the pinned Sphinx environment. Missing
references and warnings fail the build. An unrelated system `sphinx-build`
cannot change the selected toolchain. Live CLI validation needs Rust/Cargo in
the development environment. The Read the Docs hook uses the explicit
`--fixtures-only` mode so hosting needs only Python; GitHub CI retains the full
live-output validation.

The default output is `target/docs/html`. To choose another directory:

```sh
make check-docs DOCS_BUILD_DIR=/tmp/codepact-docs
```

Preview the default output:

```sh
uv run --no-project python -m http.server 8000 --directory target/docs/html
```

Open `http://localhost:8000`. Check the navigation, search, a narrow viewport,
Python API signatures, and downloads from the [schema reference](schemas.md).

## Read the Docs build configuration

The repository’s `.readthedocs.yaml` selects:

| Setting | Value |
| --- | --- |
| Operating system | Ubuntu 24.04 |
| Python | 3.11 |
| Dependency installation | `uv pip` using `docs/requirements.txt` |
| Pre-build checks | JSON schema fixtures and relative documentation links |
| Sphinx configuration | `docs/conf.py` |
| Output | HTML, with warnings treated as errors |

The Sphinx configuration enables strict reference checking, takes the canonical
URL from `READTHEDOCS_CANONICAL_URL`, and points source links at the exact commit
selected by the service. Schema files are copied into the published HTML tree.
Planning and release-evidence source pages remain linked on GitHub rather than
being included in the public navigation.

The configuration follows the official
[Read the Docs v2 reference](https://docs.readthedocs.com/platform/stable/config-file/v2.html),
[build-hook documentation](https://docs.readthedocs.com/platform/stable/build-customization.html),
and [environment-variable reference](https://docs.readthedocs.com/platform/stable/reference/environment-variables.html).

## Connect the existing project

The documented hosting address is `https://typepact.readthedocs.io/`. Keep that
existing project and URL when updating the CodePact documentation.

In its Read the Docs dashboard:

1. Verify the repository is `https://github.com/eddiethedean/codepact` and the
   configuration path is `.readthedocs.yaml`.
2. Confirm the repository integration/webhook is active so pushes trigger builds.
3. Enable `main` for the `latest` documentation and verify that the landing
   page shows the published 0.4.0 synthetic preview.
4. Enable `v0.4.0` for the documentation snapshot from the tagged commit.
   Publish post-release documentation updates from `main`.
5. Optionally enable pull request previews for reviewing documentation changes.

These dashboard settings are service state, not settings applied by this
repository. The local checks do not verify repository access, project ownership,
webhooks, or enabled versions.

## Deploy a documentation revision

1. Run `make check-docs` and review the local HTML.
2. Commit and push the documentation and build configuration to the branch the
   service builds. Confirm GitHub’s `docs` job passes for that same commit.
3. Trigger or inspect the Read the Docs build for that revision. Confirm its
   log uses the pinned requirements and both pre-build checks pass.
4. Verify the build reports success before checking the public version URL.
5. Check the landing page’s release status, search, source links, API reference,
   stylesheet, and schema downloads on the hosted site.

A successful local build establishes that the site can be generated. Deployment
is complete only when the service build and hosted revision have been verified.
If a new build fails, inspect its first failing command before changing versions
or dependencies. Historical tags retain their own build configuration.

## Maintain the site

Add public pages to a toctree in `docs/index.md`. Keep each page focused on one
task and link to reference material for details. Use source-backed examples via
`literalinclude` where practical. Preserve released installation commands and
label candidate-only commands explicitly.

After editing helpers or API signatures, rebuild the Python API reference. After
changing a schema, update its fixtures and regenerate any affected examples.
Keep the README, [availability](availability.md), [release notes](release-notes.md),
and page titles consistent about what is published. The [development guide](development.md)
covers the complete project gate.

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

The target validates JSON schema fixtures and live CLI documents, checks
relative documentation links, and rebuilds every public page with the pinned
Sphinx environment. Missing
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
The shared layout lives in `docs/_templates/layout.html`. Its CSS and brand
images live in `docs/_static/`. Preview the landing page and a long reference
page at both desktop and phone widths after changing the layout.

## Read the Docs build configuration

The repository’s `.readthedocs.yaml` selects:

| Setting | Value |
| --- | --- |
| Operating system | Ubuntu 24.04 |
| Python | 3.11 |
| Dependency installation | `uv pip` using `docs/requirements.txt` |
| Pre-build checks | Generated pins and schema references, JSON schema fixtures, and relative documentation links |
| Sphinx configuration | `docs/conf.py` |
| Output | HTML, with warnings treated as errors |

The Sphinx configuration enables strict reference checking, takes the canonical
URL from `READTHEDOCS_CANONICAL_URL`, and points source links at the exact commit
selected by the service. The template identifies the docs version, channel,
and exact hosted revision. The build copies schemas and creates downloadable
pilot, provider, work, and concurrency projects.
The public site links planning and release evidence directly to their GitHub
source pages.

The configuration follows the official
[Read the Docs v2 reference](https://docs.readthedocs.com/platform/stable/config-file/v2.html),
[build-hook documentation](https://docs.readthedocs.com/platform/stable/build-customization.html),
and [environment-variable reference](https://docs.readthedocs.com/platform/stable/reference/environment-variables.html).

## Connect the existing project

The [hosted documentation](https://codepact.readthedocs.io/en/latest/index.html)
uses the `codepact` project on Read the Docs. The
[GitHub documentation index](https://github.com/eddiethedean/codepact/blob/main/docs/README.md)
links to the guide sources.

In the Read the Docs dashboard, verify the repository integration, webhook,
and `.readthedocs.yaml` configuration. Build `main` for
[latest](https://codepact.readthedocs.io/en/latest/index.html) and a release
tag for [stable](https://codepact.readthedocs.io/en/stable/index.html).
Enable a public version for each approved release tag. Verify its URL before
adding it to that release's evidence. Use the [permanent source link](availability.md)
when the release's hosted version is unavailable.

## Deploy a documentation revision

1. Run `make check-docs` and preview the local HTML. The target checks
   navigation links, generated references, search, samples, and schemas.
2. Replay published and checkout wheels separately, as the Quality workflow
   does. Run the provider and protocol cases for changes to those guides.
3. Push the docs revision to the branch or tag the service builds. Verify the
   GitHub docs job and Read the Docs build for that revision.
4. For a package release, follow [hosted release verification](release-verification.md#verify-hosted-docs-after-publication).
   Record the permanent URL, verified revision, and published install replay
   in the release evidence.

Hosted aliases and webhooks are service settings. A local build establishes
that the source generates HTML. Verify the deployed revision after each push.

## Maintain the site

Add public pages to a toctree in `docs/index.md`. Keep each page focused on one
task and link to reference material for details. Use source-backed examples via
`literalinclude` where practical. Preserve released installation commands and
label candidate-only commands explicitly.

The published pin and immutable source revision live in `docs/versions.toml`.
After updating release metadata, regenerate the pinned commands and schema
table. To refresh rule tables, supply the actual published and checkout
executables. The published table uses that binary's explanations and
capabilities. The checkout table adds newly available rule IDs.

```sh
python scripts/sync_docs_reference.py \
  --published-cli /path/to/published/codepact \
  --checkout-cli target/debug/codepact
```

Record a new rule's first version in `docs/versions.toml`. Keep historical
commands in release notes and the upgrade guide pinned to their original
releases. The lightweight docs build checks generated pins and schema files.
It also keeps the 0.10 guard and spawn settings in the version-labeled example
while the published checker rejects those keys. The full docs job compares
rule tables to both executables.

After editing helpers or API signatures, rebuild the Python API reference. After
changing a schema, update its fixtures and regenerate any affected examples.
Keep the README, [availability](availability.md), [release notes](release-notes.md),
and page titles consistent about what is published. The [development guide](development.md)
covers the complete project gate.

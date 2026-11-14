# Development guide

CodePact development spans a typed Python helper package and a Rust analyzer. Use `uv` for the Python workspace and Rust 1.96 or newer for the analyzer.

## Start with a docs change

For a first prose or navigation edit, use the
[lightweight contribution walkthrough](https://github.com/eddiethedean/codepact/blob/main/CONTRIBUTING.md#make-a-first-change).
It needs Python and uv. The full workspace setup below builds the Rust CLI
and supports analyzer and packaging work.

## Set up a checkout

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
```

The full repository checks also use `cargo-audit`, `cargo-deny`, rustfmt, Clippy, Make, and the pinned Python checkers. Install the two Cargo auditors, then run:

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
```

`make check` covers Rust formatting, compilation, Clippy, tests, MSRV validation, dependency audits, strict Python typing and linting, packaging, and the docs build. `make check-docs` validates schemas and links, then performs a clean strict Sphinx build using the pinned docs environment.

## Focused checks

For a focused change, use the relevant target before the full gate:

| Change | Local check |
| --- | --- |
| Analyzer or lifecycle behavior | `make check-rust` |
| Python helpers or typing | `uv run --locked make check-python` |
| Python distributions | `uv run --locked make check-python-package` |
| Rust archive or installation | `make check-rust-package check-inspector` |
| Public docs | `make check-docs` |

Run the source CLI with `cargo run --locked -- ...` to avoid accidentally using
an installed published executable. The [lifecycle tutorial](lifecycle.md) contains
both an intentional failing example and its valid counterpart.

## Build the docs

The docs toolchain is pinned in `docs/requirements.txt`. Build the site locally with:

```sh
uv run --no-project --with-requirements docs/requirements.txt \
  sphinx-build -E -a -b html -n -W --keep-going docs target/docs/html
```

The Read the Docs configuration uses the same requirements file and fails the build on warnings. The generated site is written outside the source tree.

Preview the generated HTML locally:

```sh
uv run --no-project python -m http.server 8000 --directory target/docs/html
```

Open `http://localhost:8000` in a browser and check navigation, narrow layouts, code blocks, and API signatures. Stop the server when finished.

## Maintain the documentation

Public documentation starts at `docs/index.md`; repository planning notes remain under `docs/planning` and are linked from the public roadmap. The public site groups first steps, examples, adoption, reference, and project evaluation. Write guides in MyST Markdown and generated API reference entries in reStructuredText. `make check-docs` checks local relative file links and runs the strict Sphinx build; Sphinx checks public page references and headings.

The first check and lifecycle tutorial include `examples/lifecycle.py`.
The quick-start and CI shell snippets are replayed by
`scripts/check_docs_examples.py` in a disposable project. Run it with
`uv run --no-project python scripts/check_docs_examples.py` to install the
pinned packages from the configured index. To test unpublished local wheels,
set `UV_NO_INDEX=1` and `UV_FIND_LINKS` to the wheel directory and pass
`--checkout` to the script. The GitHub docs job builds and tests checkout wheels so release candidates can be checked
before publication.

The multi-file and declaration-only examples are under `docs/_examples`.
Regenerate the inspection snapshot after a source change. Use the logical name
`connection.py` to preserve module identity. See the
[0.5.1 evidence record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.5.1-evidence.md)
for current release qualification.

Read the Docs uses `.readthedocs.yaml` at the repository root. The repository
guide index lives in `docs/README.md`. See [documentation deployment](documentation.md)
for build configuration, version selection, and hosted verification. GitHub’s docs job
runs the same pinned local gate on pushes and pull requests.

## Contribution notes

- Add analyzer fixtures for supported behavior and explicit incomplete-coverage boundaries.
- Use the [project-shaped synthetic controls](synthetic-projects.md) to check planted findings, source repairs and unknown boundaries without executing fixture code. The release record describes the qualified synthetic scope and the evidence needed for field use.
- Keep unsupported source visible; do not silently treat unknown declarations as resolved.
- Preserve Python 3.11 compatibility for the helpers and type strictness in `python/codepact`.
- Update documentation when changing the contract API, CLI, schema, supported Python targets, or limits.
- Do not execute inspected Python files as part of analysis.

See [CONTRIBUTING.md](https://github.com/eddiethedean/codepact/blob/main/CONTRIBUTING.md) for repository guidance. Use the [current release checklist](https://github.com/eddiethedean/codepact/blob/main/docs/planning/releases.md#release-checklist) for publishing work.

## Measure performance

Use the [current workloads and methodology](performance.md) to compare CLI
and session behavior. Record the host, compiler, source target, analysis scope,
and exact revision. Report cold runs separately from unchanged or edited
session snapshots. The benchmark reports and query-reuse tests answer
different performance questions.

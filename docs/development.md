# Development guide

TypePact development spans a typed Python helper package and a Rust analyzer. Use `uv` for the Python workspace and Rust 1.96 or newer for the analyzer.

## Set up a checkout

```sh
git clone git@github.com:eddiethedean/typepact.git
cd typepact
uv sync --locked
```

The full repository checks also use `cargo-audit`, `cargo-deny`, rustfmt, Clippy, Make, and the pinned Python checkers. Install the two Cargo auditors, then run:

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
```

`make check` covers Rust formatting, compilation, Clippy, tests, MSRV validation, dependency audits, strict Python typing and linting, packaging, and the docs build. `make check-docs` runs just the Sphinx build.

## Build the docs

The docs toolchain is pinned in `docs/requirements.txt`. Build the site locally with:

```sh
uv run --no-project --with-requirements docs/requirements.txt \
  sphinx-build -b html -n -W --keep-going docs target/docs/html
```

The Read the Docs configuration uses the same requirements file and fails the build on warnings. The generated site is written outside the source tree.

Preview the generated HTML locally:

```sh
uv run --no-project python -m http.server 8000 --directory target/docs/html
```

Open `http://localhost:8000` in a browser and check navigation, narrow layouts, code blocks, and API signatures. Stop the server when finished.

## Maintain the documentation

Public documentation starts at `docs/index.md`; repository planning notes remain under `docs/planning` and are linked from the public roadmap. Write guides in MyST Markdown and generated API reference entries in reStructuredText. Use Sphinx document links so the strict build detects missing pages.

The quick start and multi-file tutorial include source files from `docs/_examples`. The JSON reference includes the quick-start inspection result from that directory. When changing the example, regenerate its JSON with the current CLI and keep the logical input name `connection.py` so the displayed paths and symbols remain consistent.

Read the Docs uses `.readthedocs.yaml` at the repository root. Connect the GitHub repository to the `typepact` Read the Docs project and enable the desired branches or release tags there. The checked-in configuration selects the build tools, requirements, and Sphinx entry point. GitHub's docs job runs the same strict local build target on pushes and pull requests.

## Contribution notes

- Add analyzer fixtures for supported behavior and explicit incomplete-coverage boundaries.
- Keep unsupported source visible; do not silently treat unknown declarations as resolved.
- Preserve Python 3.11 compatibility for the helpers and type strictness in `python/typepact`.
- Update documentation when changing the contract API, CLI, schema, supported Python targets, or limits.
- Do not execute inspected Python files as part of analysis.

See [CONTRIBUTING.md](https://github.com/eddiethedean/typepact/blob/main/CONTRIBUTING.md) for repository guidance.

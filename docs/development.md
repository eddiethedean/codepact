# Development guide

CodePact development spans a typed Python helper package and a Rust analyzer. Use `uv` for the Python workspace and Rust 1.96 or newer for the analyzer.

## Set up a checkout

```sh
git clone git@github.com:eddiethedean/codepact.git
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

Public documentation starts at `docs/index.md`; repository planning notes remain under `docs/planning` and are linked from the public roadmap. The public site is grouped into getting-started guides, analyzer references, and API/project information. Write guides in MyST Markdown and generated API reference entries in reStructuredText. `make check-docs` checks local relative file links and runs the strict Sphinx build; Sphinx checks public page references and headings.

The quick start and multi-file tutorial include source files from `docs/_examples`. The JSON reference includes the quick-start inspection result from that directory. When changing the example, regenerate its JSON with the current CLI and keep the logical input name `connection.py` so the displayed paths and symbols remain consistent.

The lifecycle tutorial includes `examples/lifecycle.py` directly. Check its exact
primary and related lines after edits, and confirm that removing the final use
leaves no diagnostics or coverage boundaries. Keep published installation
instructions distinct from source-build instructions. Qualification records are
under `docs/releases/`, including the CodePact 0.3.0 evidence record.
Documentation of implemented patterns does not by itself mark phase validation
rows or release gates as passed.

Read the Docs uses `.readthedocs.yaml` at the repository root. The existing
hosting project retains the `typepact.readthedocs.io` address. See
[documentation deployment](documentation.md) for the verified configuration,
version selection, dashboard setup, and publication checks. GitHub’s docs job
runs the same pinned local gate on pushes and pull requests.

## Contribution notes

- Add analyzer fixtures for supported behavior and explicit incomplete-coverage boundaries.
- Use the [project-shaped synthetic controls](synthetic-projects.md) to check planted findings, source repairs and unknown boundaries without executing fixture code. They qualify the 0.4 synthetic preview under AD01-A2; real-world usefulness remains unqualified.
- Keep unsupported source visible; do not silently treat unknown declarations as resolved.
- Preserve Python 3.11 compatibility for the helpers and type strictness in `python/codepact`.
- Update documentation when changing the contract API, CLI, schema, supported Python targets, or limits.
- Do not execute inspected Python files as part of analysis.

See [CONTRIBUTING.md](https://github.com/eddiethedean/codepact/blob/main/CONTRIBUTING.md) for repository guidance. The historical TypePact 0.2.0 release procedure is in [the release checklist](https://github.com/eddiethedean/codepact/blob/main/docs/planning/release-0.2.0-checklist.md); the [CodePact 0.3.0 release record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md) documents its completed qualification and publication workflow.

## Measure the release baseline

On macOS or Linux, build an optimized CLI and run the versioned synthetic corpus:

```sh
cargo build --release --locked
uv run --locked python scripts/benchmark_inspector.py
```

The report records median/range for fresh CLI processes and helper imports, corpus size, host information, and maximum child RSS. Process startup and OS cache effects are included. Query-reuse correctness is established separately by the Salsa execution-counter tests. Candidate results and gate decisions are tracked in the [0.1.0 evidence record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.1.0-evidence.md).

# Contributing to CodePact

Start with one focused change. CodePact 0.5.0 is the published release.
See the [current docs](https://codepact.readthedocs.io/en/latest/index.html)
and [release evidence](docs/releases/0.5.0-evidence.md) for its scope.

## Make a first change

For a prose, example, or navigation change, install Python 3.11 or later and
uv. Clone the public repository over HTTPS.

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
git switch -c docs/first-change
```

For example, clarify a rule in `docs/rules.md` or improve the expected output
in a tutorial. Run the lightweight docs checks and preview the page.

```sh
uv run --no-project --with-requirements docs/requirements.txt python scripts/check_schema_fixtures.py --fixtures-only
uv run --no-project python scripts/check_docs_links.py
uv run --no-project --with-requirements docs/requirements.txt sphinx-build -E -a -b html -n -W --keep-going docs target/docs/html
```

Open `target/docs/html/index.html`. Check the affected page and links. If you
change a documented command or its output, run
`uv run --no-project python scripts/check_docs_examples.py` as well.
[Development guide and focused checks](docs/development.md)

## Work on the analyzer or helpers

Use Python 3.11 or later, Rust 1.96 or later, uv, Make, and a linker.
`uv sync --locked` installs the workspace and builds the CLI. Analyzer code
lives in `crates/codepact/src`, typed helpers in `python/codepact`, and analyzer
fixtures in `crates/codepact/tests`.

For an analyzer change, include a minimal supported or boundary fixture with
an expected result. If the analyzer cannot resolve a construct, keep that
boundary visible. Analyze fixture source as text. Follow the
[rule catalog](docs/rules.md) and [supported syntax](docs/supported-syntax.md).

Install the pinned auditors before running the full repository checks.

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
uv run --locked make check
```

## Submit a change

1. Discuss a behavior or API change with a minimal source example and the
   expected result. Record decisions for public APIs, diagnostic codes,
   configuration keys, and semantic changes.
2. Update the relevant examples, guides, limitations, and release notes.
3. Run the focused checks and include their results in the pull request.
   Explain behavior, compatibility impact, and any remaining uncertainty.
4. Check provenance and licenses before adding dependencies or copied code.

## Release work

Use the [release checklist](docs/planning/releases.md#release-checklist) and
[maintainer authority policy](docs/planning/release-gates.md#maintainer-authority).
The tag workflow validates the exact artifact set before publication.
[0.5.0 release evidence](docs/releases/0.5.0-evidence.md) ·
[Docs publishing](docs/documentation.md#deploy-a-documentation-revision)

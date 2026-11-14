# Synthetic project controls

CodePact keeps a project-shaped synthetic corpus to find checker defects and
prevent regressions. Each example contains hand-written resource or must-use
contracts, a buggy source version, a repaired version, and an independent
expected-result entry. The fixtures resemble small CSV tools, web handlers,
SDKs, workers, plugin hosts, and reporting jobs.

The original 0.4 suite covers 33 planted findings across eight project pairs and four
explicit boundary projects. It runs against Python source targets 3.11, 3.12,
and 3.13. It also checks repair behavior, repeated output, incremental library
sessions, input-order parity, read-only analysis, and advisory versus strict
handling of incomplete coverage.

## Run the corpus

From a source checkout:

```sh
cargo test -p codepact --test synthetic_projects --locked
```

To run one example directly:

```sh
cargo run --locked -- check \
  --config crates/codepact/tests/projects/csv_importer/buggy/pyproject.toml \
  --json
```

The corresponding repaired project is under `fixed/`. Project definitions,
expected findings, and repair notes are in the
[fixture corpus](https://github.com/eddiethedean/codepact/tree/v0.5.1/crates/codepact/tests/projects).

The public guide [Bugs CodePact catches before runtime](bugs-caught.md) shows
source-grounded examples for each diagnostic family, cross-module flow, and
the async boundaries where the analyzer reports incomplete coverage.

## Catalog-inspired examples

The 0.5 suite includes an additional suite of eight bug/repair pairs and
four boundary and comparison projects. The 21 expected findings span cleanup,
borrow and ownership violations, and dropped must-use results. Every repair
passes strict mode with the same project settings and provider source.
[Corpus and run instructions](https://github.com/eddiethedean/codepact/tree/v0.5.1/crates/codepact/tests/projects/code_smells)

```sh
cargo test -p codepact --locked --test synthetic_projects code_smell_corpus
```

The examples connect catalog smells to checkable contracts. For instance,
a provider's must-use annotation exposes a discarded status result, and a
resource close declaration exposes a cleanup duty missed on early return.
[Code Smells Catalog](https://www.codesmells.org/)

## Read the result carefully

These controls establish repeatable behavior for the listed source patterns.
They do not measure precision or usefulness on existing repositories, and they
do not validate that declared contracts match external libraries or operating
system resources. Some examples deliberately retain unknown effects; an
advisory exit of zero must be read together with its coverage status.

Maintainer-approved [AD01-A2](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.4/synthetic-preview-protocol.md)
uses this independent corpus for 0.4 synthetic preview qualification. Planted
findings do not become field-pilot findings; the original real-project results
remain historical and insufficient to qualify real-world usefulness.

# Synthetic project controls

CodePact keeps a project-shaped synthetic corpus to find checker defects and
prevent regressions. Each example contains hand-written resource or must-use
contracts, a buggy source version, a repaired version, and an independent
expected-result entry. The fixtures resemble small CSV tools, web handlers,
SDKs, workers, plugin hosts, and reporting jobs.

The test suite covers 33 planted findings across eight project pairs and four
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
[fixture corpus](https://github.com/eddiethedean/codepact/tree/main/crates/codepact/tests/projects).

## Read the result carefully

These controls establish repeatable behavior for the listed source patterns.
They do not measure precision or usefulness on existing repositories, and they
do not validate that declared contracts match external libraries or operating
system resources. Some examples deliberately retain unknown effects; an
advisory exit of zero must be read together with its coverage status.

Synthetic findings do not count toward the 0.4 real-project pilot or release
gate A05. See the [pilot protocol](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.4/pilots.md)
for that separate qualification.

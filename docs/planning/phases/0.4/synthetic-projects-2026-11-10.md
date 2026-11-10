# Synthetic project evaluation follow-up — 2026-11-10

Status: **The updated corpus and repository checks pass for the controlled
contracts below. This remains synthetic evidence and does not count toward
release gate A05.** The original November 9 run and artifacts are preserved as
a separate [dated evaluation](synthetic-projects-2026-11-09.md).

## What changed

The 8 buggy/repaired project pairs and 4 boundary projects still specify 33
planted findings across TP1001, TP1002, TP1003, TP2001, and TP3001. Fixture
sources now include the normal annotations and meaningful operations needed to
make each bug clear, while avoiding ordinary unused-variable and late-import
lint errors. The plugin's acyclic named re-exports use explicit import aliases
so the fixture stays within the analyzer's modeled import subset.

Every one of the 20 buggy, repaired, or boundary source variants passes the
repository's Ruff rules and strict Pyright. The full `uv run --locked make
check` run also passes, including the three Python analyzer targets, the
focused corpus suite, package smoke checks, dependency audits, schemas, and
strict Sphinx documentation build. The corpus test emits 72 CLI reports:
48 paired-project reports and 24 boundary reports.

The [public bug guide](../../../bugs-caught.md) includes source-backed examples
for each diagnostic family, cross-module cleanup flow, async cleanup, and
incomplete-coverage boundaries. Its static-tool claims are tied to these
repeatable repository checks; they do not claim that specialized tools cannot
find similar issues.

## Reproduce

```sh
cargo test -p codepact --test synthetic_projects --locked -- --nocapture
ruff check crates/codepact/tests/projects scripts/check_synthetic_static_tools.py
ruff format --check python/codepact scripts crates/codepact/tests/projects
python scripts/check_synthetic_static_tools.py
uv run --locked make check
```

The full check uses the repository's pinned tools. The evidence directory
retains the 72 reports, test log, static-tool logs, full-check log, source
hashes, and checker binary hash:

- [Evidence README](evidence/synthetic-projects-2026-11-10/README.md)
- [Deterministic raw report archive](evidence/synthetic-projects-2026-11-10/reports.tar.gz)

## Limits

These fixtures test hand-specified local contracts and controlled repairs.
They do not measure production precision, recall, adoption effort, or annotation
quality against external libraries. The findings cannot satisfy the real
project sample, actionability, diversity, and async requirements in A05. Read
the [pilot protocol](pilots.md) for those separate gates.

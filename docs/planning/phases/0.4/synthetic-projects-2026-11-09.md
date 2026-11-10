# Synthetic project evaluation — 2026-11-09

Status: **The synthetic controls pass for the listed contracts on Python source
targets 3.11–3.13. They are defect-discovery evidence, not real-project pilot
evidence and do not pass A05.**

The runnable corpus is in
[`crates/codepact/tests/projects`](../../../../crates/codepact/tests/projects/README.md).
The test harness is
[`synthetic_projects.rs`](../../../../crates/codepact/tests/synthetic_projects.rs),
and the independent expected outcomes are in its `manifest.json`.

## Why this corpus

The initial upstream scans produced too few eligible findings to exercise
repairs. Synthetic projects provide controlled contracts and paired source
repairs, so the analyzer can be checked against known outcomes without
pretending that unannotated upstream code is a useful acceptance sample.
The real-project pilot remains separate and unchanged.

## Design and observed results

The corpus has eight buggy/repaired project pairs and four unsupported-boundary
projects. Its manifest specifies 33 definite findings across TP1001, TP1002,
TP1003, TP2001, and TP3001. Each pair uses identical configuration and
provider files; only `src/main.py` changes. The suite compares the full stable
finding set by file, containing function, rule, and certainty, then requires
the repaired tree to have no stable findings.

The paired-project suite runs 48 CLI analyses: eight pairs, two variants, and
three Python targets. It reports 33 expected findings per target across the
eight buggy trees (99 target-specific detections of 33 distinct planted
cases), with no stable findings after repair. Four boundary projects run in
advisory and strict modes on all three targets, producing 24 more reports.
Each pair is also checked with fresh and reused library sessions and reversed
input order.

Six repaired projects have complete coverage for this supported subset.
`artifact_packager` remains incomplete because callable-alias lifecycle
effects are unknown; `sdk_client` remains incomplete because some class and
callable-alias effects are unresolved. These are retained as unresolved
boundaries, including on repaired source. The four separate boundary projects
assert that strict mode does not call incomplete analysis clean.

The source-read canary is a dynamic plugin fixture that would write a marker
file if executed. The suite hashes the complete temporary project before and
after each CLI run, so target execution or source-tree writes fail the test.
Policy and model fingerprints, selected files, and repeated JSON output are
also compared around each repair.

## Defect found and corrected

The imported-helper case exposed a real checker gap. On the previous 0.4
candidate, the plugin fixture's helper was reported as an unknown call effect,
so the analyzer missed the definite use after close. The current flow analysis
follows acyclic named re-exports to supplied helper bodies and reports TP1001
at the later use, with the helper's close as its related location. Cyclic
imports remain an unknown boundary; the existing import-cycle file-order
regression verifies that cycle uncertainty is preserved.

The first draft also had an oracle mistake: it assigned TP1003 to repeated
close even though the repeat-close policy is unspecified. The corpus now tests
repeated close as TP9001 unknown. Draft expectations are recorded in the
manifest revision history; only revision 3 is used by the passing suite.

## Reproduction and artifacts

Run the focused test and optionally retain every CLI JSON report:

```sh
cargo test -p codepact --test synthetic_projects --locked
CODEPACT_SYNTHETIC_RESULTS_DIR=/tmp/codepact-synthetic-reports \
  cargo test -p codepact --test synthetic_projects --locked
```

The dated evidence directory contains the test log, environment and source
hashes, the 72 raw reports in a deterministic archive, and the prior-candidate
report for the imported-helper reproduction:

- [Retained evidence README](evidence/synthetic-projects-2026-11-09/README.md)
- [Raw CLI reports](evidence/synthetic-projects-2026-11-09/reports.tar.gz)

## Limits

These fixtures use authored local contracts and deliberately small programs.
They do not measure production precision, recall, usability, annotation effort,
or behavior of real dependencies. The 33 cases are not a sample of naturally
occurring violations. They cannot satisfy the accepted minimum of 30 eligible
real findings, 90% actionability, three reviewed projects, or an async real
project required by A05. See the [pilot protocol](pilots.md) and
[release-readiness plan](release-readiness.md).

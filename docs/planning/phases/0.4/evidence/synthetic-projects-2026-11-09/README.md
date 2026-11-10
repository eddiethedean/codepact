# Synthetic project evidence — 2026-11-09

This folder retains the exact focused-test run and the CLI reports emitted by
the analyzer. The fixtures and expected outcomes are documented in the
[synthetic project evaluation](../../synthetic-projects-2026-11-09.md).

## Run summary

- Oracle revision: 3; provenance: synthetic; real-project pilot credit: none.
- Scope: eight buggy/repaired pairs, 33 distinct definite cases, four
  unsupported-boundary projects.
- Analysis targets: Python 3.11, 3.12, and 3.13.
- Reports: 48 paired-project reports (buggy and fixed) plus 24 boundary reports
  (advisory and strict), for 72 total.
- Result: all 99 expected target-specific detections (33 cases across three
  targets) matched the independent manifest; repaired variants have no stable
  findings. Unsupported coverage remains visible.
- Focused test: `cargo test -p codepact --test synthetic_projects --locked
  -- --nocapture` — 4 passed, 0 failed.

The output files retain each report's schema, selected files, source snapshot,
policy/model fingerprints, diagnostics and coverage. The test also compares
repeated output, checks the temporary source trees before and after analysis,
and exercises fresh/reused library sessions.

Both async boundary projects contain possible TP1002 findings, so their
advisory runs exit 1 under the fixture's `fail-on = "warning"` policy. The
dynamic-plugin and repeat-close advisory runs contain only TP9001 unknown
boundaries and exit 0; all four strict runs exit 1 because coverage is
incomplete. The distinction is retained in the reports.

## Retained files

- [`reports.tar.gz`](reports.tar.gz): deterministic archive of the 72 raw CLI
  reports.
- [`synthetic-projects.log`](synthetic-projects.log): focused test output.
- [`make-check.log.gz`](make-check.log.gz): byte-exact compressed output from
  the full repository check, including package builds and installs, RustSec/Cargo
  Deny, MSRV, schema and documentation checks.
- [`previous-candidate-plugin-report.json.gz`](previous-candidate-plugin-report.json.gz):
  prior-candidate reproduction of the imported-helper gap.
- [`metadata.json`](metadata.json): environment, binary, source-file, report
  archive and prior-candidate hashes.

## Integrity

- Report archive SHA-256:
  `3387b1bcb3d876e92e831ba271dedf8ed6408c8fc29dd1db03f943796f86aa07`
- Focused test log SHA-256:
  `463157050c3218f0fec511bc660f646f07d7beebc1e04958e4e3c13e26747723`
- Full `uv run --locked make check` log SHA-256:
  `39ceb63c531385929519d1b19d40f2dd21afc09fd48d9fd47ea1be679b0f4180`
- Analyzer and fixture source tree SHA-256:
  `f4e44c556200830447dfde0fea28c4d105e489a07db69e4797295400bcd00a98`
- Full check log archive SHA-256:
  `3b6721f37d214935761abd2703b8e5d5d58d3e7a2137dd75784372af7a304f05`
- Checker binary SHA-256:
  `46ced3497cd62111472950299d9427e3e151b2fa02a7ca7ed78f7c48b9fab11f`

The source hash covers `db.rs`, `flow.rs`, `semantic.rs`, the integration
harness, the oracle manifest, and every fixture source/configuration file. The
aggregate is SHA-256 of compact UTF-8 JSON encoding `source_hashes` sorted by
path; `metadata.json` records the algorithm. The
run started from repository base revision
`c8cf0e2a1c18b95a39d1bb82fe462db759333ba0`; the source tree hash identifies
the exact uncommitted implementation and fixture state analyzed on top of
that base. All source and per-file hashes are listed in `metadata.json`.

The prior-candidate report was generated using the exact f4859b0 macOS arm64
CLI wheel (SHA-256
`ecda1279918f1a069c57dd27989cc777ff34aac96854aa465e319dc4b1b13cae`) on the
same `plugin_runner/buggy` tree. It reported an unknown effect for
`close_token` and did not report the later use after close. The current source
tree resolves that acyclic named re-export and reports TP1001.

## Limit

These are controlled analyzer checks, not real-world measurements. The 33
planted cases do not count toward release gate A05, whose real-project sample
and actionability requirements remain pending.

# Synthetic project evidence — 2026-11-10

This folder retains the focused test run, all CLI reports, static-tool results,
and the successful full repository check. The evaluation is synthetic and
provides no credit toward the real-project pilot or release gate A05. See the
[November 10 evaluation follow-up](../../synthetic-projects-2026-11-10.md).

## Run summary

- Scope: eight buggy/repaired project pairs, 33 distinct planted cases, and
  four unsupported-boundary projects.
- Analysis targets: Python 3.11, 3.12, and 3.13.
- Reports: 48 paired-project reports plus 24 boundary reports, for 72 total.
- Focused test: 4 passed, 0 failed; all 99 target-specific planted detections
  matched the independent manifest, and repaired variants have no stable
  findings.
- Static controls: Ruff passed; strict Pyright passed for all 20 source
  variants.
- Full `uv run --locked make check`: passed, including Rust and Python checks,
  package smoke tests, Cargo Audit, Cargo Deny, schema fixtures, and strict
  Sphinx documentation.

## Retained files

- [`reports.tar.gz`](reports.tar.gz): deterministic archive of all 72 raw CLI
  reports.
- [`synthetic-projects.log`](synthetic-projects.log): focused test output.
- [`ruff-static.log`](ruff-static.log): Ruff lint and format checks.
- [`pyright-static.log`](pyright-static.log): strict Pyright results for all
  20 variants.
- [`make-check.log.gz`](make-check.log.gz): compressed full-check output.
- [`metadata.json`](metadata.json): exact environment, source hashes, tool
  outcomes, and artifact checksums.

## Integrity

- Report archive SHA-256: 0671cc032e3fb389e6d00a2443d836201dc60e5189368a22a9de0a63dcb8c8e4
- Focused test log SHA-256: 564ce8036c0af1f55fd85df576d344c728fced5070b511958c3929a72e6733d1
- Ruff log SHA-256: d29644cebc037e2a733db225b8964bef215b4d0e037a4d4d2ce1aeb46f05a78c
- Strict Pyright log SHA-256: ee969f244acccfb5774a2915e91bdc5eb6982d443dac14625705490ebbb069cd
- Full check log SHA-256: 88e23537f3bed57f55145084d3f407150f54aa1de31318b40d8a0f17c96929d9
- Full check compressed log SHA-256: f74ebcd91304ff845b9058b513aca655ab1a166f162d3118e549a4637d368570
- Source tree SHA-256: b95b391b80d5144b0517f18d73058260ff68e803c6f9913e15d85966ee28737d
- Checker binary SHA-256: 60927b9dc417bc5876da87f3fcdf0635b6581b233f55da7f1019c28362bfe901

The source hash covers the analyzer implementation files, integration harness,
manifest and every fixture source/configuration file, static-tool checker,
Makefile, and the new corpus documentation. Per-file hashes and the algorithm
are recorded in `metadata.json`. The working tree was evaluated on top of base
revision `5dffaa819317c7fb4c07cedca36648378e93d07e`.

## Limits

These results establish controlled behavior for the specified local contracts.
They do not measure production precision, adoption effort, or real dependency
semantics. The 33 planted findings are not eligible pilot findings and do not
change A05.

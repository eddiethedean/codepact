# Phase 0.6 local candidate qualification

Status: **The exact candidate passes the local checks recorded below. The release remains held: phase-specific acceptance evidence, cross-platform artifact evidence and the G7 release decision are incomplete.**

## Revision and environment

- Base: `2d6bb6bcb27f501dd6c13421a77949cc9e70a33f`.
- Candidate: `1da46b68d8e0d9ebc226441abb3eea3c1c28f13f`, local branch `codex/phase-0.6-candidate`.
- Package version: `0.6.0` across the Python helper, CLI and Rust crate.
- Host: macOS 26.5.2 arm64, Darwin 25.5.0; Rust/Cargo 1.96.0; CPython 3.11.15 development environment.

## Exact-candidate local checks

- `uv run --locked make check` passed on the exact candidate. The workspace run passed 593 Rust unit and integration tests, including 40 phase 0.6 tests and 29 release-qualification tests. The advanced CLI policy coverage checks advisory and strict exit behavior, rule selection, suppression, JSON/text/concise output and detail caps.
- The Python suite passed 54 tests; five executor tests were skipped by the development interpreter's exact-version guard. Separate pinned runs passed helper probes (3/3) and TaskGroup probes (6/6) on CPython 3.11.15, 3.12.13 and 3.13.16; all executor probes passed (5/5) on 3.13.16. Exact interpreter and upstream source fingerprints are in [pinned model evidence](models-2026-11-14.md).
- Ruff, Basedpyright, strict Mypy and the Pyright State fixture passed. The synthetic static project suite passed 47 cases.
- Schema validation covered 9 schemas, 13 valid/invalid fixtures and 18 live CLI JSON outputs. After writing this candidate evidence, `uv run --locked make check-docs` passed again: relative links in 124 documents and Sphinx 9.0.4 with nitpicky warnings treated as errors.
- `cargo audit` found no advisories in 127 locked crates. `cargo deny` passed advisory, ban, license and source checks. The locked Rust 1.96 MSRV check passed.
- The crate source archive passed package verification and its full Rust test suite. The helper and CLI wheel/sdist builds, isolated installs and installed `codepact 0.6.0` smoke checks passed on this host. These do not establish the full native wheel matrix on other platforms.
- Exact-candidate retained and advanced optimized benchmarks passed their fresh/reused result assertions. A separate build of released `v0.5.1` in its own target directory enables a direct same-host comparison. Timing and memory results, commands and raw samples are in [candidate performance evidence](../evidence/candidate-performance-2026-11-14.json); interpretation is in [performance results](../performance.md). The earlier mutable-worktree performance files are retained but excluded from comparison because they have no source hash and do not reproduce this exact-candidate run.
- Query invalidation, location refresh, model changes and cancellation recovery have Rust session tests. The phase 0.6 protocol-provider test observes three initial flow queries, then two after changing the provider, while fresh/reused results agree. Budget-exhaustion controls exist for flow, object, alias and request limits. The remaining phase-specific workload and model-summary requirements are listed in the [validation matrix](../validation.md).

## Scope review and remaining release gates

- A connected GitHub search on 2026-11-14 found no open issues or pull requests for `eddiethedean/codepact`. This does not establish that private reports or maintainer triage outside this repository are complete. The connected account has repository maintain permissions.
- Exact-version API lookups on 2026-11-14 returned 404 for both PyPI projects (`codepact`, `codepact-cli`) and for `codepact/0.6.0` on crates.io; the crates.io sparse index returned 200 but contained no 0.6.0 entry. Registry ownership, upload credentials and publication workflow permissions have not been validated.
- Cross-platform release CI, the eight-platform native wheel build matrix, hashes and install/smoke checks on the other supported platforms have not run for this candidate.
- The V06-01–V06-16, T01–T07 and CS04 gates remain pending. The [validation matrix](../validation.md) records exact-candidate coverage and the outstanding acceptance cases. In particular, mixed-interface parity, several binding/finally/cancellation failure paths, checked-wrapper comparisons, precision recovery at configured bounds, some TaskGroup/executor failure cases, and the complete preview bug/repair/boundary corpus are not yet qualified.
- The exact candidate and local evidence are available for maintainer review in the [0.6.0 candidate release record](../../../../releases/0.6.0-evidence.md), but no G7 approval has been recorded.
- No tag, merge, push, registry publication or release announcement has been made.

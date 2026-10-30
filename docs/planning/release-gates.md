# Common 0.x release gates

These gates apply to every phase and patch release. Phase-specific gates add requirements. **No gate is passed merely by creating this document.**

## Gate matrix

| ID | Gate | Required evidence / pass condition |
| --- | --- | --- |
| G1 | Scope and semantics | Accepted scope and API/IR decisions; every advertised feature has defined valid, invalid and unknown-boundary behavior. |
| G2 | Correctness and interoperability | Applicable regression suites pass; equivalent contracts normalize equally; mixed-interface producer/consumer combinations agree. No unresolved blocker in claimed scope. |
| G3 | Compatibility and runtime behavior | Pinned Python/checker/platform matrix passes for advertised combinations; runtime identity, introspection and optional enforcement behavior are documented and checked. |
| G4 | Operational quality | No target-code execution; bounded analysis terminates with visible incompleteness; deterministic output. Record baseline time/memory, investigate regressions and verify Salsa fresh/updated parity and tracked-query reuse from 0.1; verify persistent-cache parity when introduced. |
| G5 | Distribution | Matching tag/package versions; clean builds, Python metadata validation, Cargo dry run, clean install/import/CLI checks for each advertised artifact, licensing and provenance. |
| G6 | User readiness | Examples and reference reflect shipped behavior; limitations, new diagnostics, configuration changes and migration steps are reviewed. Pilot requirements apply from 0.4 onward. |
| G7 | Release decision | Exact candidate revision, gate report, issue triage and accountable maintainer approval recorded. Missing evidence or a required failed gate blocks tagging. |
| G8 | Publication completion | Authorized tag publishes both registries; downloaded artifacts match expected versions and hashes; smoke checks succeed and release notes link known limitations. |

G1–G7 are pre-tag gates. G8 is a post-publication completion gate: a failed or partial publication leaves the release incomplete and invokes recovery. No package should be published merely to test these documents.

## Evidence rules

Use one evidence record per release candidate. Include revision, tool versions, environment, commands, logs/artifact links, fixture identifiers, result and reviewer. Re-run affected gates when code or configuration changes. Reuse unaffected evidence only with a recorded explanation of why it remains applicable.

Status values: Pending, Pass, Fail, or Not applicable. A required phase gate cannot be marked Not applicable without an accepted scope amendment. Conditional common gates may be Not applicable when the capability does not yet exist, with a reason; basic build, correctness, provenance and publication checks remain required for every release.

Do not infer support for combinations that were not checked. Fixtures demonstrating unsupported behavior count as boundary coverage, not as support.

## Blocker classification

Release blockers include incorrect acceptance within a claimed contract, data loss from a suggested fix, crashes on the supported corpus, silent loss of analysis coverage, frontend semantic divergence, broken artifacts or missing license/registry access. Record severity and scope for each issue. Minor wording defects may be deferred with an owner and follow-up; blockers require repair or an explicit reduction of supported scope.

## CI rollout

| Phase | Minimum new automation to add |
| --- | --- |
| 0.1 | Build/import, Ruff AST/binding/captured-alias corpus, Salsa parity/reuse/invalidation, compatibility and archive validation |
| 0.2–0.3 | Golden diagnostics, flow/alias/exception/cleanup corpus |
| 0.4 | JSON/schema, modes, baselines and configuration checks |
| 0.5–0.6 | Transfer, borrowing, escape, typestate and effect regressions |
| 0.7 | Runtime variants, adapters and finite-match cases |
| 0.8 | Persistent-cache equivalence/recovery, editor revisions/cancellation, platform and integration matrix |
| 0.9 | Full candidate qualification and documented recovery rehearsal |

The current release.yml includes validation, artifact checks and quality-workflow dependencies. Extend the quality suites to cover the new migration gates and retain their required dependency relationship for publication. Manual reports are appropriate for pilot triage and scope review; record them explicitly.

## Partial publication and recovery

PyPI and crates.io publication is not atomic. If one job succeeds, preserve that version and rerun only the failed publication job after resolving the cause. Verify whether an upload completed before retrying a timeout. Do not move a published tag or overwrite an artifact. If the released contents are wrong, document the issue, yank where appropriate and create a corrective patch release.

These operational policies extend the [release plan](releases.md).

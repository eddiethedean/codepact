# Carried-forward ecosystem and stabilization gates

Status: **All future gates pending**. This registry preserves the original E01–E12 and S01–S09 definitions verbatim while assigning their delivery across 0.8–0.13. Source: the former ecosystem and stabilization phase plans at main commit 4113996a218cd17b2c5c8bb0823b41572f86c983. This is a scheduling amendment, not a reduction in acceptance requirements.

The [root roadmap](../../../ROADMAP.md) is authoritative. [G1–G8](../release-gates.md) apply to every release. New asyncio, multiprocessing and bridge semantics use AS/MP/BR gates in the [concurrency plan](../concurrency.md#new-stdlib-semantic-gates); no historical gate changes meaning.

## Completion ownership

| Gate | New completion owner | Retained commitment |
| --- | --- | --- |
| E01, E02, E06 | 0.12 | Persistent-cache parity, recovery, deterministic execution and performance/memory. In-memory parity recurs earlier. |
| E03, E07, E08 | 0.12 | Current-revision editor parity and complete LE01–LE05 / LE-V01–LE-V07. |
| E04, E05 | Each applicable release; full ecosystem audit in 0.12 | Every claimed integration has pinned evidence; every candidate retains the complete distribution contract. |
| E09 | 0.12 | AI04–AI05 / AI-V01–AI-V06, read-only MCP, bounded snapshots and recorded client workflows. |
| E10 | 0.12 composite completion; mandatory checkpoints in 0.8 and 0.9 | E10-A blocks 0.8 on artifact integrity and author export/verification; E10-B blocks 0.9 on artifact comparison; E10-C completes supported-editor parity in 0.12. Overall status remains pending until every clause passes. |
| E11 | 0.8 | Exact-version project dependency model, applicability, override provenance and fresh/reused parity. |
| E12 | 0.9 | Opt-in declaration policy, baselines and strict coverage; affected behavior is requalified in later phases. |
| S01–S09 | 0.13 | Every original stabilization requirement remains required, including two successive qualified candidate revisions. |

CS06 retains its integration/model-matrix definition and moves to 0.12. CS07 retains its stable-scope audit definition and moves to 0.13. Their canonical definitions remain in the concurrency plan.

## Mandatory E10 checkpoints

These checkpoints allocate the unchanged E10 definition below. Each is a release requirement for its named phase, with valid, invalid and incomplete-evidence controls. The composite gate remaining pending does not defer an earlier checkpoint. All checkpoints are currently pending.

| Checkpoint | Release owner | Required evidence |
| --- | --- | --- |
| E10-A | 0.8 | Qualify two independently installed providers and a caller with matching inline/manifest cleanup, ownership-transfer and exceptional-unknown behavior. The installed author CLI exports a manifest from staged inputs; the author builds the wheel, verifies that actual artifact, installs it outside the source checkout and checks a downstream consumer. Verify distribution/import-scope identity, relevant file digests, marker/manifest inclusion and reproducibility. Wheel verification rejects stale, malformed, wrong-distribution or mismatched-file metadata; consumers leave affected facts unavailable with explicit coverage reasons. CLI/Rust and fresh/reused analysis agree after provider upgrades, shadowing and artifact/model edits. |
| E10-B | 0.9 | The installed author CLI compares two actual local contract artifacts and produces deterministic human-readable and machine-readable results. Qualify compatible changes, new caller obligations, weakened guarantees and insufficient evidence, with CLI/Rust and fresh/reused parity. Missing applicability or evidence cannot produce a compatible verdict. This artifact-author workflow is required independently of source/report comparison under CR01–CR09. |
| E10-C | 0.12 | Requalify applicable E10-A/E10-B evidence on the selected candidate and complete CLI/Rust/supported-editor parity after provider upgrades or shadowing. Close E10 only when every original clause and all checkpoints have evidence; cache/editor completion cannot substitute for the earlier artifact and author-tool requirements. |

The proposed command names are `codepact contracts export`, `contracts verify` and `contracts diff`; the required behavior and release ownership do not depend on the final spelling. The [publication design](../published-contracts.md#author-workflow-release-requirements) specifies the author workflow and its boundaries. Builds and controlled conformance runs are explicit author/fixture steps; contract analysis and author commands do not import or execute target modules.

## Ecosystem gates

### E01

- [ ] **E01** — Cached and fresh analysis produce equal facts and diagnostics across source, dependency summary, configuration, target-version and engine-version changes.

### E02

- [ ] **E02** — Corrupt or incompatible caches are discarded safely; parallel execution preserves deterministic diagnostic ordering.

### E03

- [ ] **E03** — CLI and editor agree on codes, locations and certainty; document changes and cancellation do not publish stale diagnostics.

### E04

- [ ] **E04** — Every supported integration names pinned upstream versions and has a maintained fixture; metadata is not silently interpreted as runtime validation.

### E05

- [ ] **E05** — The pure `codepact` wheel installs the helpers independently and its `cli` extra resolves the exact matching CLI. Each native CLI wheel supplies a working command on its declared platform. Both source archives and the optional Cargo installation remain usable.

### E06

- [ ] **E06** — Publish cold/warm latency and peak-memory measurements. Investigate regressions over 10% median on repeated controlled runs before acceptance.

### E07

- [ ] **E07** — Rapid edits, deletions and cancellation publish only current-revision diagnostics; a reset or rejected persistent cache recomputes equal semantic results.

### E08

- [ ] **E08** — Complete LE01–LE05 and LE-V01–LE-V07 in the [linting plan](../linting-and-editors.md). Each claimed editor displays CodePact findings and related locations, clears fixed findings, preserves coverage/certainty and has a reproduced installation recipe with a versioned support matrix.

### E09

- [ ] **E09** — Complete AI04–AI05 and AI-V01–AI-V06 in the [AI-agent plan](../ai-agent-integration.md). Qualify structured tool/CLI/editor parity, bounded current-snapshot results, read-only workspace enforcement and recorded client repair workflows before claiming support.

### E10

- [ ] **E10** — Two independently installed providers and a caller exercise supported cleanup, ownership transfer, and exceptional unknowns with matching inline and manifest behavior. Wheel verification rejects stale or invalid metadata. Contract diffs distinguish compatible changes, new caller obligations, and insufficient evidence. CLI, Rust and supported editor checks agree after a provider upgrade or shadowing change.

### E11

- [ ] **E11** — Qualify a project-supplied contract for a resolved factory returning a fresh open owned resource from an exact installed distribution version. Reproduce a leak and its repair. Verify that a successful close discharges cleanup and a raised close leaves it unknown without an explicit exceptional effect. Exercise a version-bounded override and report the replaced origin. Check installed metadata and matching/mismatching `uv.lock` entries. A stale model after upgrade or an ambiguous provider leaves affected coverage incomplete. Malformed or conflicting project contract files exit `2`. Verify provenance and fresh/reused parity after model or provider changes.

### E12

- [ ] **E12** — Qualify opt-in declaration rules for known resource parameter access, supported ownership handoffs and selected synchronous APIs or modules, including `@must_use`. Exercise a definite missing declaration and its repair, independent preview and enforcement, and baseline matching. A baseline affects finding disposition while strict still fails on incomplete coverage. Changes to applicable contracts or policy refresh affected diagnostics and snapshots and invalidate incompatible baselines.

## Stabilization gates

### S01

- [ ] **S01** — All required gates from prior phases have evidence or a reviewed scope change that removes the affected support claim; no unsupported feature is labeled stable.

### S02

- [ ] **S02** — No unresolved critical soundness issue, analyzer crash on the supported corpus, or unexplained cross-interface mismatch remains.

### S03

- [ ] **S03** — At least two successive candidate revisions pass the full supported matrix with no new blocking regressions; record exact revisions and artifact hashes.

### S04

- [ ] **S04** — Migration documentation covers every breaking 0.x change; a representative pilot upgrades using those instructions.

### S05

- [ ] **S05** — Release artifacts, licensing, source provenance, installation, trusted publishing and crates token setup are verified; partial-publish recovery has a written rehearsal record.

### S06

- [ ] **S06** — Named maintainers accept the 1.0 scope, support window, deprecation policy, public limitations and security reporting process.

### S07

- [ ] **S07** — The candidate passes the full edit/invalidation corpus and dependency-upgrade rehearsal, with bounded memory and no stale/incorrect cached facts in the supported scope.

### S08

- [ ] **S08** — Rehearse compatible, breaking and unknown provider changes using actual wheels and downstream code. Verify schema and applicability handling, provenance, baseline invalidation, and CLI/editor parity. Record the stable package layouts, capabilities and upgrade guarantees supported by the evidence.

### S09

- [ ] **S09** — Rehearse declaration policy adoption on a representative project from preview and baseline to opt-in strict enforcement. Upgrade a dependency with a project-supplied model, observe stale-model coverage, and repair the model and affected declarations. Verify strict coverage and baseline behavior throughout. Record which rules and model policies qualify for stable opt-in support, which remain preview or deferred, and the evidence for any proposed default change.

## Evidence accounting

Record completed sub-evidence without closing a composite gate early. E10-A is mandatory for 0.8 and E10-B for 0.9 even while E10 remains pending; a missing checkpoint blocks its phase or requires an explicit reviewed scope amendment. E10 cannot pass before E10-C and supported-editor parity in 0.12. Reuse prior evidence only with its exact source revision and an applicability explanation. An implemented prototype, clean local run or unsupported-boundary fixture is not a completed release gate.

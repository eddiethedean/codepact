# 0.4 release readiness work plan

Status: **Development and qualification work remain before a release can be
approved.** This plan preserves the accepted AD01 requirements, including
bounded-discovery amendment AD01-A1, and turns the held
[release ledger](../../../releases/0.4.0-evidence.md) into an ordered work
queue. Task names and proposed artifact paths below are work to do, not evidence
of a passing gate.

## Starting point and findings

The latest checked implementation snapshot, `eb093c9`, contains the expanded
synthetic corpus, fixes exercised by those fixtures, and representative
feature-set examples in the user docs. It passed the full local check and
[Quality run 42763947397](https://github.com/eddiethedean/codepact/actions/runs/42763947397).
Its [build-only Release run 42814622714](https://github.com/eddiethedean/codepact/actions/runs/42814622714)
passed the complete distribution workflow. The [current-candidate pilot
replay](evidence/2026-11-10-candidate-eb093c/pilot/README.md) still has no
eligible definite findings in completed scans; three of six projects fail
before analysis. The [V04 matrix](validation.md) still lacks complete row-level
qualification, the operational record covers only part of the required
workloads, and the real-project pilot has no eligible acceptance sample.

The readiness audit found two input-limit issues now addressed in development:

- Discovery counted excluded Python files toward the 10,000-input limit.
  A real CLI reproduction with one selected file and 10,001 files beneath
  `exclude=['generated/**']` failed with exit 2. Selection now precedes the
  input count. Separate limits now stop traversal after 100,000 filesystem
  entries, stop sorting a directory after 20,000 entries, and stop recursion
  beyond depth 256. The limits are published through capabilities. Unit tests
  cover each bound, exactly reaching the selected-file limit followed by empty
  directories, and rejecting the next selected file before growing storage.
- Configuration loading checked file metadata, then used an unbounded read.
  A growing file could bypass the 64 KiB allocation bound. The reader now
  consumes at most 65,537 bytes, rejects overflow, and accepts exactly 64 KiB.
  Unit and CLI controls cover consumption, the inclusive boundary and recovery.

The configuration and discovery changes are included in `83ae561` and its
exact-candidate checks pass. They do not complete V04-03/24; full near-limit,
containment and recovery controls still need to be retained before G4.

All 17 retained JSON documents from candidate `c0ea48a` (including decompressed
pilot reports) validated against their then-current applicable schemas during
this audit. This strengthens V04-05 evidence for those historical documents
only. The capabilities schema now includes the new discovery limits, and the
repeatable schema check validates fixtures plus eight current CLI outputs:
capabilities, explanations, completed/failing checks, settings, baseline,
baseline-update, and inspection documents.

## Baseline identity correction in development

RR01 reproduced a false baseline match: a function acquiring `Connection()`
still had its TP1002 suppressed after the resource's `closed_by` changed from
`close` to `release`. The acquisition, exit and related spans were unchanged,
so the previous source-span fingerprint omitted the changed contract.

Fingerprint algorithm 4 includes the resolved resource symbol, lifecycle,
access and provenance; synchronous must-use diagnostics retain their resolved
callable identity. Private query transport retains those facts without adding
fields to public inspection/check JSON. Findings reached through multiple
paths combine their contract identities deterministically. Old algorithm 3
baselines fail validation, and baseline creation refuses findings lacking the
internal resolved evidence. Regression controls also rebind the imported
resource while keeping both resource definitions selected, preserve harmless
comment/location edits, and compare fresh and reused sessions.

This correction requires a new source/distribution freeze. It does not yet
qualify all changed effect-summary, collision, interruption or V04 controls.

The [23 retained configuration controls](evidence/config-qualification-2026-11-10/README.md)
qualify V04-02 technically. The [14 CLI workloads](evidence/adoption-cli-2026-11-10/README.md)
retain seven optimized samples with independent oracles for MustUse, config,
discovery, baseline matching and reporting. Session query-event measurements,
final-candidate inherited comparisons and memory/pilot qualification remain.

## Critical path

| Order | Work | Concrete completion condition | Gates |
| --- | --- | --- | --- |
| RR01 | Close input, baseline and output correctness risks | Review bounded readers/writers and traversal; exercise changed contract/effect identity, baseline collisions, interrupted updates, hard limits and failure recovery with independently expected results. | G2/G4, A01–A03/A07/A08 |
| RR02 | Complete deterministic V04 qualification | Every V04-01–26 assertion maps to actual valid, invalid and unsupported controls; results carry revision, command, environment and retained artifacts. Complete the inherited V03 and CLI LE/AI mappings. | G2/G3, A01–A04/A06–A08/CS02 |
| RR03 | Establish useful real-project adoption | Preregister a viable adoption evaluation, retain all six original results, prepare justified contracts where needed, and obtain the unchanged minimum of 30 distinct definite violations with at least 90% actionable across at least three reviewed projects, including async. | G6, A05/A06/CS02 |
| RR04 | Complete operational measurements | One repeatable harness covers all accepted workloads with seven samples, medians/ranges, source/result hashes and query-event parity/reuse evidence, plus peak/retained-memory methods and reviewed regressions. | G4, A07/CS02 |
| RR05 | Finish the release user journey | Complete external-tool decoding, CLI agent stale/truncated/failure handling, advisory/strict CI exits 0/1/2, repair with affected dependents, and release-version/availability documentation review. | G6, A06/A08 |
| RR06 | Freeze and build the final candidate | All source, tests, documentation and raw evidence are committed; full local checks and Quality pass; build-only Release produces and verifies the complete distribution matrix for that exact candidate. | G2–G6 |
| RR07 | Record the maintainer decision | The accountable maintainer reviews the concrete gate report, signs the applicable passes with a date, and explicitly approves the exact release candidate. The pre-publication checker passes before any tag. | G7 |

Start RR03 feasibility work while completing RR01/RR02. Pilot viability can
require substantial source-contract or analyzer work and determines whether
the accepted useful-preview claim is attainable. Collect RR04 measurements
after semantic fixes; complete RR06 after the remaining source work is stable.

## RR02: missing qualification controls

The existing focused tests are a foundation. Review each assertion in the
required matrix rather than treating a test name as a full-row pass.

| Rows | Work to finish |
| --- | --- |
| V04-01–04 | Complete config discovery/origin and equivalent-invocation controls, containment/dedup/module collisions, selected scope and near-limit/recovery cases. Retain unsupported module-path behavior explicitly. |
| V04-05–08 | Validate emitted documents; compare canonical results across all formats and repeated/input-order runs; decode adversarial Unicode/CRLF/path labels with current byte spans; finish every policy truth-table combination. |
| V04-09–11 | Test individual suppression codes and unused directives; changed declarations/effects/trust/completion/target/model identities; harmless whitespace/location edits; forced identity collisions and duplicate insertion/reordering; failed/atomic baseline update behavior. Review fingerprint inputs against the accepted canonical contract, including resolved contract identity. |
| V04-12–16 | Cross decorator/alias/verbose producer and consumer forms; duplicate metadata; forwarding and explicit-discard effects/raises; cleanup overlap; reversed descriptor decorators and unsupported async/generator/dynamic cases; helper typing/introspection matrix. |
| V04-17–19 | Check bounded explanations and all modeled related-role spans; stale evidence after source/config/model/baseline changes; truncation with hidden actionable findings; hard report overflow, serialization failure and broken output streams. |
| V04-20–21 | For every listed mutation, compare fresh one-shot, fresh session and reused session; verify current locations, exact dispositions/coverage and recovery. Record semantic-query execution/reuse independently of elapsed time. |
| V04-22–25 | Extend the retained onboarding to affected dependents and CI; reproduce the external-tool adapter and opt-in agent instructions; consume unsupported-version, stale, truncated and failed results; verify read-only/non-execution and every fatal/recoverable budget boundary. |

A reference-index script can prevent stale matrix links. Semantic assertions
and reviewed actual results remain the basis for each row's status.

## RR03: make the pilot evaluate adoption

The three completed upstream scans yielded TP9001 boundaries and no eligible
definite violations. A search of their primary source roots found no CodePact
resource/must-use declarations. The supported resource model is declaration
based; the only standard-library cleanup models are `contextlib.closing` and
`aclosing` around a recognized resource. Scanning an unannotated repository
does not establish the checker can find its framework/resource contracts.
This is a selection/adoption mismatch to resolve before more acceptance runs.

The [synthetic project corpus](synthetic-projects-2026-11-10.md) supplies a
repeatable way to discover checker defects and verify source repairs across
different project shapes. It exposed an imported-helper lifecycle gap and
keeps unsupported cases explicit; all fixture variants also pass Ruff and
strict Pyright. Use it before real-project scoring; its planted cases are not
eligible pilot findings and do not change A05.

The other three failures have documented causes: pip includes a path that
cannot be mapped to a Python module, and Click/FastAPI exceed the inherited
1,000,000-byte total-source budget. These are retained execution failures,
not false-positive labels. A larger budget or broader import/model support
would need its own design, scope decision and qualification.

Use the annotation-preparation route already permitted by the
[pilot protocol](pilots.md):

1. Retain the original pinned projects, settings and results as the first
   evaluation. Inventory additional candidates and suitable bounded package
   scopes using project/API facts before examining new acceptance findings.
2. Prepare a dated amendment with exact project revisions, full include/exclude
   scope, source-byte inventory, target/rules/policy, sample method, and each
   source-attributed contract patch. Justify scope by the development task and
   analyzer budget. Scope changes require the amendment before new scoring.
3. Derive annotations from actual API lifecycle/return contracts. Retain patch
   provenance and unsupported assumptions. Keep upstream behavior intact; do
   not insert violations or count controlled demonstrations in the pilot.
4. Run a disclosed feasibility/adoption experiment, including one async project,
   to determine whether the current subset reaches useful real code. Keep it
   distinct from the frozen acceptance score. Fix supported-scope defects and
   retain their before/after evidence.
5. Freeze the final acceptance set and candidate before scoring. Rerun all
   affected scopes consistently, count the full eligible population, then
   apply the accepted census/stratified procedure and retain every rationale.
   Expand the preregistered inventory if findings remain too few.

If this process cannot meet A05, the accepted 0.4 release remains held. A
different preview scope is a separate maintainer roadmap/AD01 amendment with
revised public claims and acceptance criteria; it cannot be implemented by
marking a pending gate Pass or lowering the observed sample denominator.

## RR04: one operational harness

Extend the existing lifecycle/cleanup harnesses with a structured adoption
benchmark. Record individual samples and corpus/config/baseline hashes. Cover:

- inherited sync/async cold in-process and CLI checks, warm no-op, unrelated,
  imported semantic and location-only edits;
- MustUse producers/consumers, ordinary/near-limit configuration and discovery,
  empty/dense baselines, unchanged/changed/ambiguous matches;
- mode, threshold, severity, suppression and baseline-only edits with separate
  semantic and reporting costs and query events;
- text/JSON/concise and limited reports, full/returned counts and output bytes;
- all accepted pilot scopes with their coverage and boundaries;
- peak and retained memory with host/CPU/toolchain and measurement method.

Compare inherited workloads with the released 0.3 implementation on the same
host and corpora. Report the new adoption workloads separately because 0.3
does not implement those operations. Investigate regressions before acceptance.

## RR06–RR07: candidate and tag procedure

Commit source fixes, tests, user docs and raw evidence before selecting the
final candidate. Review the availability/install/reference/release-note prose:
several current pages identify 0.3.0 as published and 0.4.0 as an unpublished
candidate; the final release snapshot must describe its intended version.

Then run:

```sh
uv run --locked make check
```

Run Quality and the build-only Release workflow on the frozen ref. Retain the
exact commit, all platform/interpreter results, bundle and individual package
hashes, clean installs and publisher-skipped status. Reuse older evidence only
with an explicit unaffected-source explanation; repeat affected checks after
source/configuration changes.

The release guard allows the eventual tag commit to differ from the frozen
candidate only in `docs/releases/0.4.0-evidence.md`. Generate/commit other raw
evidence before the final freeze, then complete that ledger with the final
workflow results and dated maintainer decisions. Before tagging, verify:

```sh
python scripts/check_release_evidence.py 0.4.0 --tag-commit HEAD
```

The accountable maintainer can review and approve all gates alone under D018.
No additional reviewer or blanket review waiver is required. G7 is the final
explicit release decision after the technical evidence is complete. An
authorized publication then requires G8 registry downloads, version/hash
verification and clean smoke installs; partial publication follows the common
recovery policy.

# 0.4 adoption decision record — AD01

Status: **The accountable maintainer accepted all eight proposed AD01 defaults
on 2026-11-09, and delegated decision authority was used on the same date to
add the bounded-discovery amendment AD01-A1.** This freezes the candidate's
intended scope and acceptance oracle. It does not claim the required V04
fixtures, pilot, performance measurements or compatibility checks have passed,
and it does not authorize a release.

The specification and this record govern AD01. No second reviewer is required
under [D018 and the maintainer authority policy](../../release-gates.md#maintainer-authority).
Qualification evidence below remains mandatory; listing a fixture is not a
claim that it has passed.

## Accepted decisions

| Decision | Accepted choice | Alternative considered | Qualification evidence and reconsideration trigger |
| --- | --- | --- | --- |
| Configuration | One nearest `pyproject.toml`; explicit `--config` selects exactly one file, `--no-config` skips discovery, and scalar CLI values override file values and built-in defaults. Keep path bases, discovery, excludes, symlink policy and `inspect-contracts` compatibility as specified. | Merge ancestor/nested configuration, environment variables or project hooks. Rejected because implicit sources obscure effective policy and provenance. | Complete V04-01–04 and V04-24, including nested roots, precedence, malformed settings, near-limit inputs, path traversal and symlink cases. Reconsider only if pilot repositories show the single-file model prevents normal adoption; any expansion needs explicit precedence and provenance. |
| Policy | Keep advisory mode and fail-on-error as defaults for compatibility. Strict mode additionally fails on unsuppressed possible violations and incomplete coverage. TP3001 defaults to warning; mandatory source/coverage diagnostics cannot be demoted or suppressed. | Make strict mode the default or allow severity/filter settings to erase uncertainty and mandatory errors. Rejected to preserve gradual adoption and honest coverage. | Complete every V04-08 truth-table case, including thresholds, suppressions, incomplete analysis, cancellation and exit codes. Reconsider if pilot use shows the accepted default prevents useful CI adoption without misrepresenting certainty. |
| Schemas and API compatibility | Use checker schema 2 for expanded `check --json`; preserve inspection schema 0 and existing Rust one-shot/session APIs; use baseline, capability and explanation schema 1. Document checker 1-to-2 migration and reject unsupported major versions clearly. No runtime checker-schema-1 adapter or Rust API amendment is included. New policy/report APIs wrap canonical results. | Add fields to schema 1, maintain a runtime compatibility adapter, or change existing Rust API behavior. Rejected to keep contracts explicit and avoid implying older consumers understand new fields or silently changing current callers. | Complete V04-05 and consumer migration fixtures. Reconsider if a concrete supported consumer cannot migrate or an API amendment is needed; preserve field/API meaning or make the change explicit and versioned. |
| Suppression | Allow only reasoned, same-line directives with exact eligible TP1xxx/TP2xxx/TP3xxx codes. Keep malformed/unknown directives as unsuppressible TP4002 errors; preserve suppressed findings and reasons in audit output. | Blanket/file-wide ignores, `noqa` aliases or next-line scopes. Rejected because broad scopes can hide unrelated findings and coverage. | Complete V04-09 for multiline primary/related locations, malformed/unused directives, policy changes and unchanged coverage. Reconsider only if evidence shows the exact-line mechanism cannot support a common workflow while retaining auditable scope. |
| Baselines | Use versioned semantic-anchor/evidence fingerprints with one-to-one matching. Ambiguous or stale matches remain visible; no line-number or source-order fallback. Update only through the explicit atomic `baseline update` operation. | Line-based or ordinal matching that keeps entries stable across edits. Rejected because it could hide newly inserted identical findings. | Complete V04-10–11 and V04-20–21 for insertion, collision, rename, reorder, semantic changes, stale entries, interruption and fresh/reused parity. Reconsider the fingerprint inputs or version when a reproducible false match or excessive harmless churn is found; never resolve ambiguity by hiding findings. |
| Must-use | Provide the lightweight `@must_use`, `MustUse[T]`/`Annotated` and `discard` spellings. Enforce only resolved synchronous return contracts and immediate handling; do not track assigned values later abandoned. Unsupported async, generator and dynamic consumption remains a visible boundary. Preserve independent cleanup obligations. | Track eventual use, infer by symbol spelling or claim async/task consumption. Rejected because these would exceed the qualified identity and flow model. | Complete V04-12–16, including helper/type-checker compatibility, identities, aliases, descriptor ordering, mixed interfaces, cleanup overlap and unsupported cases. Reconsider expanded dataflow or async semantics only with a separate sound design, corpus and phase scope decision. |
| Reporting limits | Retain inherited analyzer budgets and accept 64 KiB per config file; 16 MiB and 100,000 entries per baseline; 32 MiB per serialized checker report; 32 related sites, 2 KiB of optional source snippets and 8 KiB of explanation/repair text per finding. Hard-limit overflow returns 2; detail caps retain canonical findings, counts, coverage and exits while reporting omissions. | Unbounded input/output or silent omission. Rejected because both can cause resource exhaustion or false-clean reports. | Complete near-limit and recovery fixtures, bounded-allocation review, and ordinary-pilot measurements; record regressions against the 0.3 baseline for G4. Reconsider a numeric limit if representative data shows it is too low or unsafe; any change needs corresponding boundary tests and visible failure/omission behavior. No latency target is implied. |
| Pilot evaluation | Use three distinct pinned real projects covering a synchronous library, service/CLI application and async application. Review at least 30 distinct definite violation findings and require at least 90% actionable under the frozen scope and protocol. Use a census when practical or a preregistered reproducible stratified sample; do not count synthetic findings. | Smaller or synthetic-only samples, selecting projects after seeing results, or counting possible/boundary findings toward the threshold. Rejected because these weaken reproducibility and invite selection bias. | Follow the accepted [pilot protocol](pilots.md): freeze projects and settings before scoring, retain labels/rationales/disagreements and per-project/rule counts, and review unknown boundaries separately. If the selected projects lack 30 eligible findings, expand the preregistered set and disclose it; do not lower the threshold or inject findings. Reconsider the protocol only before evaluation, with an explicit scope amendment. |

## Amendment AD01-A1 — bounded project discovery

The maintainer explicitly delegated AD01 decision-making authority earlier in
this task. Acting under that instruction, Codex accepts these fixed discovery
limits for 0.4: at most 100,000 filesystem entries across a project walk, at
most 20,000 entries retained from any one directory for sorting, and at most
256 directory levels below the effective root. `capabilities --json` exposes
all three values. Exceeding any bound is a fatal input error (exit 2); it must
not produce a completed check.

The original 10,000 selected-Python-file cap remains separate. This amendment
addresses excluded and non-Python trees that otherwise consume unbounded walk
and sorting work. It preserves sorted discovery and built-in skips. It does
not change the accepted pilot threshold, analyzer source-byte budget, or
supported semantic subset.

| Alternative considered | Decision and qualification |
| --- | --- |
| Leave directory enumeration and recursion unbounded | Rejected because excludes do not bound traversal and `read_dir` results were retained in full before sorting. Test exact/overflow behavior for selected files, total entries, a single directory and nesting depth; include excluded-tree and real-project near-limit evidence in V04-03/24 and G4. |
| Reuse the selected-file limit for traversal entries | Rejected because ignored files, excluded paths and ordinary non-Python project files should not consume the selected-source budget. Retain independent counters and publish both meanings. |

Reconsider these numbers only if the preregistered pilot or representative
repositories encounter them. Any change must be amended before qualification,
appear in the capability contract and schemas, and include exact-boundary,
overflow, recovery and operational evidence. This amendment does not itself
pass V04-03/24 or G4.

## Acceptance record

| Maintainer | Date | Accepted decisions | Specification baseline | Basis | Release authorization |
| --- | --- | --- | --- | --- | --- |
| `eddiethedean` | 2026-11-09 | All eight proposed AD01 defaults, without amendment | Candidate specification at `06086544fa9075eddcafa67db1ae7412c7f9f803`; see linked specification and qualification evidence | Explicit maintainer direction; accepted alternatives, limitations and evidence requirements are recorded above | Separate decision; not granted |
| `Codex`, under explicit maintainer delegation | 2026-11-09 | AD01-A1 bounded project-discovery limits | Current 0.4 specification and capability schema | The maintainer had authorized Codex to make the AD01 decision on their behalf | Separate decision; not granted |

The original acceptance and AD01-A1 together define the accepted scope and
support G1 for that scope. The V04 matrix, pilot protocol execution, operational
qualification, distribution install checks and G2–G7 evidence remain separate
requirements. Any change to an accepted default requires a recorded amendment
and updates to affected fixtures and plans before claiming support.

AD01 assigns accountable maintainer `eddiethedean` as owner of the remaining
AD02–AD08 work, A01–A08/CS02 evidence, pilot execution and operational
qualification. The maintainer may validate, review and approve that evidence
alone under D018. This assignment is not evidence of completion.

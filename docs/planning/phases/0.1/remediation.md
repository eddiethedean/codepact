# Phase 0.1 implementation repair plan

Status: analyzer implementation complete; R07/R08 candidate evidence and release gates remain pending. This document translates the [accepted analyzer migration](../../analyzer-migration.md) into changes to the repository. It refines WP01–WP07 and AM01–AM06 without replacing F01–F08 or V01–V24.

Implementation status (2026-10-30): pinned Ruff parser/AST 0.0.12 and Salsa 0.28.5 are in Cargo.lock; Rust MSRV is 1.96. Ruff AST traversal emits structured declarations and expression trees with exact source ranges, consumed by a TypePact-owned ordered binding and contract-view pass. It supports captured local aliases and deterministic direct absolute imports of exported classes and aliases. Salsa tracks per-file parsing, per-module semantic analysis and semantic-only export summaries. Session results carry an external revision; Salsa cancellation returns an error without publishing partial results. Source, file, statement, expression, import-graph, output and retained-session budgets are implemented. The CLI uses logical module names, root-relative spans and bounded reads. Local query, cancellation, span-refresh and work-limit tests are present. The locked Rust 1.96 check, strict Clippy gate, extracted Cargo archive test/install/smoke, three Python helper tests, wheel/sdist build and clean install, Twine metadata check, and pinned mypy/Pyright fixtures passed locally on Python 3.11. Full V01–V24/F01–F08 evidence and cross-platform candidate qualification remain open. The `ruff_python_semantic` integration assessment is recorded in D014. Keep release gates pending until their stated evidence exists.

## Required outcome

Replace the current RustPython syntax-validation and line-scanning inspector with Ruff AST analysis, correct declaration/binding semantics, and a reusable Salsa-backed analysis session. Preserve the lightweight Python helpers, `inspect-contracts` command, one-shot Rust entry point and inspection-only assurance. Qualify the resulting source distributions before marking 0.1 complete.

The immediate correctness target is `View = Borrowed[C]; C = int`: View must retain the original class declaration and its metadata, while subsequent bare C references use the new binding. The same rule applies to aliases captured before imports, function definitions or repeated class declarations replace a name.

## Current implementation gap map

| Current location | Existing behavior | Repair |
| --- | --- | --- |
| Cargo manifests and lockfile | Ruff parser/AST 0.0.12, Salsa 0.28.5 and Rust 1.96 are pinned and used | Complete candidate-specific archive, license and MSRV evidence |
| `crates/typepact/src/lib.rs`, `model.rs`, `source.rs`, `frontend.rs`, `semantic.rs` | Public facade and result model; structured Ruff declarations and source-range mapping feed a TypePact-owned binding and lowering pass | Keep the public facade stable; further index/lowering decomposition is maintainability work, not a remaining analyzer-query blocker |
| Ruff AST in `frontend.rs` and typed lowering in `semantic.rs` | Supported module declarations and annotation expressions are traversed as AST nodes; no semantic source-line or delimiter scan remains | Expand typed lowering coverage and keep unsupported forms explicit |
| `semantic.rs` declaration and binding state | Class declarations have independent identities; ordered AST events capture alias symbol, access and lifecycle views | Complete cross-version rebinding and diagnostic evidence in V01–V24 |
| `semantic.rs` module ordering and `ExportedView` summaries | Direct supplied-module class and assignment-alias imports resolve dependency-first; import re-export chains remain incomplete | Retain and qualify invalidation behavior; re-export chains remain outside 0.1 |
| `db.rs` and `AnalysisSession` | Reusable Salsa session, per-file parse and per-module semantic/export queries, no-op writes, update/removal, reset, cancellation, external revisions and one-shot wrapper | Run the full query edit matrix and document candidate evidence |
| `source.rs` and Ruff AST ranges | Source spans use AST byte ranges, multiline line/column conversion and one-based UTF-8 byte columns | Retain reviewed schema snapshots and complete matrix mapping |
| `crates/typepact/src/main.rs` | Logical module names, root-relative labels, bounded reads and the shared one-shot API are implemented | Keep CLI/API limits aligned and complete clean-install qualification |
| Rust frontend/resolution/incremental fixtures | Cover captured aliases, target grammar, module alias exports, multiline spans, UTF-8 byte columns, dependency invalidation, cancellation recovery and work limits | Complete V01–V24 mapping and external matrix evidence |
| `.github/workflows/quality.yml` | Rust/Python checks, Cargo archive verification, clean wheel/sdist installation and extracted CLI smoke are configured. Local equivalents passed on Python 3.11 | Run the full workflow across its matrix on the release candidate and record its result |
| Documentation and qualification | Architecture, supported boundaries, limits, session revisions and cancellation are documented | Complete F01–F08/G1–G7 evidence and candidate report; publisher verification remains explicit |

The current local Rust regression suite has 17 unit tests, 1 CLI test and 35 fixture tests. Candidate qualification, cross-platform CI results, release gates and publisher verification are not claimed.

## Target internal layout

Keep the existing Rust crate and binary while introducing internal modules. Avoid a workspace split during the repair unless dependency qualification demonstrates a need.

| Proposed file | Responsibility |
| --- | --- |
| `src/lib.rs` | Public re-exports, compatible one-shot `inspect`, session facade |
| `src/model.rs` | Public serializable contracts, diagnostics, coverage; internal typed access/provenance values |
| `src/source.rs` | Logical file/module identity, physical-path adapter, UTF-8 line index and range conversion |
| `src/frontend.rs` | Pinned Ruff API adapter, target-version options, parse errors and AST traversal |
| `src/semantic.rs` (current) | Ordered binding handling, class identities, captured alias views, direct import resolution, export summaries and metadata lowering; finer source-file decomposition remains maintainability work |
| `src/resolve.rs` (proposed) | Separate canonical API identities, module exports and cycle/unknown outcomes from the current semantic pass |
| `src/lower.rs` (proposed) | Isolate the current shared annotation, alias, parameter, return and diagnostic lowering path |
| `src/db.rs` | Salsa file inputs, per-file tracked parsing, per-module inspection and export queries, reusable session, cancellation, revisions, reset and retained-history cap |
| `src/lib.rs` limits | 1 MB per file/request, 10,000 files, 20,000 top-level statements per file, 50,000 request declarations/import edges, 128 expression depth, 50,000 expression nodes per statement, 10,000 contracts and a 1,000-mutation retained-history reset |
| `src/main.rs` | File input and CLI presentation; no independent semantic rules |
| `tests/frontend.rs`, `resolution.rs`, `incremental.rs`, `cli.rs` | AST/range, binding/import, edit/reuse and invocation regressions |

Module names are implementation guidance; public API names and selected Salsa macro signatures must follow the qualified dependency version. Upstream types and database IDs stay internal.

## Ordered work items

### R01 — Qualify dependencies and freeze semantic decisions

Maps to AM01 / WP01. Changes: Cargo manifests/lockfile, `decisions.md`, `compatibility.md`, and dependency evidence.

- Spike pinned Ruff parser/AST/text-range APIs and Salsa inputs/queries in an isolated development example. Assess whether `ruff_python_semantic` primitives can be integrated without importing Ruff's linter driver; record the chosen boundary and a TypePact-owned fallback index.
- Select exact compatible versions/revisions and verify licenses/notices, target-version behavior, Rust edition/MSRV and source-package availability. Use a registry-compatible dependency set or a reviewed vendoring/publication design before committing the migration to the release path.
- Specify eager annotation/alias evaluation order, including reading a prior binding on the right-hand side before rebinding the left. Explicitly classify string/future annotations, lazy aliases, conditional definitions and nested scopes as supported or incomplete.
- Decide deterministic external declaration identity for repeated definitions before serializing it. Preserve `module.Class` for existing unambiguous cases; document any required disambiguation/schema change. No raw Salsa IDs or absolute paths in canonical identities.
- Freeze direct-import scope: explicit module inputs, absolute imports and any accepted relative-import subset; reject unsupported re-exports and cycles visibly. Resolve builtins and non-resource types only as far as needed to avoid attaching old resource facts to new bindings.

Acceptance: V07/V24 parser/version and package spikes, written decisions, qualified toolchain, and an executable dependency example. Unresolved registry delivery or incompatible licensing blocks R02 dependency adoption. No exact dependency version or newly supported Python version is assumed by this plan.

### R02 — Isolate the public model and add the Ruff frontend

Maps to AM02 / WP03–WP04. Depends on R01. Changes: `model.rs`, `source.rs`, `frontend.rs`, facade and manifests.

- Move existing public result types without changing their ordinary serialized form. Use internal enums so values such as `conflict` cannot escape as valid access qualifiers.
- Parse using Ruff with the configured target version. Build a source line index and translate complete AST ranges, including multiline and Unicode spans.
- Walk declarations, imports, decorators, assignments and parameter/return annotations as nodes. Preserve relevant statement order; skip executable bodies only according to the 0.1 scope, not indentation guesses.
- Distinguish irrelevant ordinary expressions from relevant unsupported declarations. Never scan string/comment contents as declarations or interpret a local lookalike by spelling.
- Keep a temporary development comparison path if useful, but do not route unsupported AST cases back through the legacy scanner. Remove the old frontend at R07 cutover.

Acceptance: V07/V08/V16/V18, all existing paired examples, and structural fixtures for multiline signatures/decorators, comments/strings, defaults, inline bodies, async function declarations and every supported annotation position. Unknown scope forms remain visible boundaries. Snapshot span corrections require review rather than blind golden regeneration.

### R03 — Implement declaration identities and captured contract views

Maps to AM03 / WP03–WP04. Depends on R02. Changes: `index.rs`, `resolve.rs`, `lower.rs` and binding tests.

- Store immutable declaration records independently of current name bindings. Give repeated definitions distinct internal identities.
- Build ordered binding events for supported assignments, imports, class/function definitions and deletions. Preserve class records while names are rebound.
- Resolve an eager alias against its current environment and capture the resulting declaration, lifecycle/access metadata and provenance. Alias-of-alias views compose dimensions without looking up the original name again later.
- Flatten resolved alias views; use iterative, bounded resolution for unresolved graphs. Do not confuse valid eager reassignment with an actual unresolved cycle.
- Use a single annotation/metadata lowering path for aliases, parameters and returns. Emit TP4002 with primary and related metadata locations for incompatible access/lifecycle declarations. Unsupported metadata never becomes a fabricated resolved fact.

Acceptance: V01–V06/V17–V19. The captured-View example retains the old class contract; a later new class C has a distinct identity and no inherited old metadata. Equivalent Pythonic/explicit declarations agree; unrelated names, imports and rebinding never gain TypePact meaning.

### R04 — Add supplied-module resolution and semantic exports

Maps to AM03–AM04 / WP04. Depends on R03. Changes: `resolve.rs`, export types, source/module inputs and multi-file fixtures.

- Build a deterministic module map from explicitly supplied inputs; diagnose duplicate logical modules and ambiguous resolution. Module order and file argument order must not decide contract identity.
- Resolve supported direct imports to exported declaration/contract views. Known TypePact APIs are identified by canonical origin after local binding rules.
- Represent missing modules as tracked lookup outcomes so adding/removing an input can invalidate its users. Keep all filesystem reads and module discovery in the host; no Python imports.
- Separate export semantic values from whole ASTs and location rendering. Preserve declaration provenance using an origin reference that can retrieve current spans without forcing all semantic callers to depend on foreign source positions.
- Detect import cycles through bounded graph processing; return incomplete coverage with a reason and location instead of an accidental Salsa query cycle or panic.

Acceptance: V06/V09–V11/V21–V22. Cross-file names resolve only through supported imports; adding a missing dependency repairs prior unknowns; unrelated modules remain isolated; reversing supplied-file order preserves output.

### R05 — Introduce the reusable Salsa analysis session

Maps to AM04 / WP05. Depends on R03–R04; database scaffolding may be prototyped during R01. Changes: `db.rs`, source/settings inputs and public session facade.

Implementation: per-file parse queries feed per-module inspection/export queries. Consumers receive semantic-only class and alias summaries. `AnalysisSession` exposes set/remove/reset, no-op writes, a session revision outside the JSON result, and an `inspect_cancellable` outcome adapted from Salsa. Tests cover body-only provider edits, changed exports, moved provider spans, missing-module creation, fresh-result parity and no-op reuse. Candidate-wide acceptance mapping remains open.

- Add per-file source inputs and explicit module-map, target-version and relevant configuration inputs. Use granular inputs so changing one source does not invalidate every file through a monolithic source-list field.
- Add tracked parse, declaration-index, export, contract and inspection queries. A caller reads exported semantic summaries rather than routinely traversing another module's AST.
- Expose session operations for setting/removing source, changing settings and inspecting selected inputs. Specify replacement/removal behavior and duplicate-input errors. Retain `inspect(&[SourceFile], python_version)` as the compatible one-shot wrapper.
- Tag session results with a revision outside the existing JSON envelope unless an API/schema decision requires otherwise. Return owned public results so internal database lifetimes do not leak to callers.
- Ensure unchanged input writes are no-ops. Keep source offsets/current provenance separate from semantic equality so moved lines refresh locations while unchanged export semantics can stop further invalidation.
- Expose test-only query execution events or counters; do not infer query reuse from elapsed time.

Acceptance: V20–V22, fresh versus reused equality for contracts/diagnostics/coverage, no-op reuse, isolated edits, semantic export changes, body-only edits, moved spans, missing-module creation/deletion and target/settings changes. A body edit may rerun parsing/indexing; it must not rerun downstream semantic checks when the exported semantic summary is unchanged.

### R06 — Enforce limits, revision safety and CLI identity

Maps to AM05 / WP05. Depends on R05. Changes: `lib.rs` limits, session adapter in `db.rs`, `source.rs`, `main.rs` and CLI tests.

Implementation: bounded file reads and inputs, per-file and request-wide AST work limits, expression depth/node budgets, import-graph and output limits, plus a reset of retained Salsa history after 1,000 source mutations. Cancellation returns `AnalysisCancelled` and recovery is tested; the non-cancellable wrapper returns an incomplete TP9002 result if its session is externally cancelled. CLI module labels and spans remain independent of absolute roots. Full V10/V19/V23 matrix evidence remains open.

- Apply source limits during bounded file reading and before parsing; enforce declaration/graph/work limits during construction rather than only truncating emitted output afterward. Specify defaults and whether a limit is per file, request or session.
- Bound alias/import traversal and retained session memory. Provide explicit reset/eviction behavior consistent with the selected Salsa APIs. Do not create nondeterministic diagnostics from incidental cache occupancy.
- Adapt the qualified Salsa cancellation mechanism into a documented session outcome. Discard cancelled/stale revision output and ensure the next request succeeds. Cancellation is not a completed inspection with complete coverage.
- Derive logical module names independently of absolute physical paths. Define a deterministic base for CLI explicit files and package `__init__.py` handling; add an explicit root option if needed, with documented path rules. Treat ambiguity as an invocation/configuration error.
- Route CLI/API through the same queries and coverage aggregation. Preserve documented exit codes, version/help and inspection-only assurance; validate supported targets using the same configuration policy.

Acceptance: V10/V11/V19/V23, long chains within the byte limit, early work-limit exhaustion, cancellation followed by an update, relative/absolute invocation parity, multiline spans and no target execution. Limit/cancellation recovery must not publish stale complete results.

### R07 — Cut over fixtures, CI and clean distributions

Maps to AM05–AM06 / WP06. Depends on R02–R06. Changes: manifests/lockfile, tests, quality workflow and source-build documentation.

- Verify the Ruff AST frontend is the only semantic frontend and keep it free of source-line, header-slicing and delimiter scans. Continue reducing parallel alias metadata maps as tracked module summaries are introduced.
- Preserve the example golden and API/CLI contract where semantics are unchanged. Review fixes for missing provenance, invalid metadata and incorrect spans as intentional changes.
- Update `unresolved_qualified_types_and_multiline_declarations_are_incomplete`: keep unresolved-base coverage, but add a valid multiline case that must now succeed.
- Replace the misleading eager `alias_cycles_are_reported_without_recursing_forever` expectation with a positive captured-identity case; add a separate genuinely unresolved/lazy-cycle boundary case. Assert diagnostics/coverage, not merely no contracts.
- Correct the comment-quote fixture to contain actual triple quotes, not escaped quote characters; retain the separate ordinary-string regression. Keep the restored cross-file assertion and add explicit class/alias import-rebinding cases.
- Add packaged Rust frontend/resolution/incremental/CLI fixtures covering V01–V24. Keep fixture assets within the crate archive, not only the workspace.
- Extend quality jobs for the qualified MSRV/stable toolchains and advertised platforms. Exercise fresh/reused parity and query-event assertions in CI.
- Replace the Cargo `--no-verify` plus checkout `--path` smoke test with archive verification and build/test/install of the extracted package from a temporary directory outside the repository. Check the installed command there using packaged inputs.
- Retain helper/checker tests, wheel/sdist clean installs, no-execution fixture and publication dependencies. Separate helper Python versions from analyzer target versions.

Acceptance: complete V01–V24 mapping with fixture names, commands and result links; clean helper and Cargo source installations; no legacy fallback; reproducible dependency/MSRV matrix. Existing local passes are reused only when their assertions and dependencies remain applicable.

### R08 — Qualify the repaired 0.1 candidate

Maps to AM06 / WP07. Depends on R07. Changes: inspector/user docs, contributor setup, evidence record and handoff.

- Update `docs/inspector.md`, README, CONTRIBUTING and compatibility tables to describe the implemented architecture, commands, supported syntax and session behavior. Remove stale claims that no implementation or check commands exist.
- Record dependency pins/notices, tooling, supported targets, fixture results, no-execution evidence, incremental query events and cold/no-op/local-edit/dependency-edit measurements for the exact candidate.
- Complete an F01–F08 and G1–G7 report using the evidence template. Keep registry ownership/publisher verification explicit; CI alone does not approve a release.
- Hand 0.2 the declaration index, captured contract views, query APIs, documented cycle policy and supported-module boundaries. CFGs, resource instances, flow summaries, lifecycle enforcement, editor services and disk caches remain later work.

Acceptance: no unresolved correctness blocker in claimed 0.1 scope; walkthroughs reproduce clean installs and inspection; candidate report has evidence for every required gate. Publication/G8 requires a separately authorized release action and is not part of this repair implementation.

## Dependencies and review slices

~~~text
R01 -> R02 -> R03 -> R04 -> R05 -> R06 -> R07 -> R08
                 \------------> R05 scaffolding
~~~

R05 scaffolding can proceed after R03 while R04 resolves modules, but query acceptance depends on R04. Avoid maintaining two authoritative semantic engines across review slices. Each slice must identify its changed assertions, transitional limitations and remaining gates; intermediate passing tests do not make the migration release-ready.

## Definition of complete

- [ ] R01–R08 acceptance evidence exists, mapped to AM01–AM06 and F01–F08.
- [ ] Ruff AST traversal is the sole supported semantic frontend; RustPython/line scanning is removed.
- [ ] Captured aliases retain declarations and metadata through every supported rebinding form.
- [ ] Direct supplied-module imports, unknown boundaries and cycles have deterministic documented outcomes.
- [ ] A reusable Salsa session and one-shot wrapper agree on fresh and updated facts, diagnostics, provenance, spans and coverage.
- [ ] Query event evidence establishes unchanged-work reuse and dependency isolation.
- [ ] Limits, cancellation and source identity behave consistently through API, CLI and clean package installations.
- [ ] Dependency/MSRV qualification, docs and candidate gates reflect actual implementation rather than planned capabilities.

The AST frontend, typed declaration lowering, captured local/exported alias views, exact source ranges, per-module Salsa summaries, cancellation recovery and bounded work/cache policies are implemented. Finish candidate qualification by recording the full V01–V24/F01–F08 matrix, platform results and extracted-package installation evidence; do not mark publication gates complete without their separate evidence.

## Python helper and CLI distributions

D015 amends R01/R07/R08 and F04/F08: maturin native typepact-cli wheels carry the Rust CLI; typepact remains pure Python and depends on the same CLI version. Require clean installations that verify automatic dependency resolution and standalone CLI behavior. Release builds require the eight-target inventory defined in the release plan; Cargo packaging remains independently required.

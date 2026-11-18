# CodePact 0.x release roadmap

Planning baseline: 2026-10-31; post-0.7 sequence updated 2026-11-17 from retained evidence and stdlib concurrency priorities. **The predecessor project, TypePact, published and qualified 0.1.0 and 0.2.0 under the `typepact` package names.** The repository and project are now CodePact. CodePact 0.3.0 uses the new `codepact` Python package, `codepact-cli` command distribution, `codepact` CLI and Rust crate. The release implements cleanup behavior in the [refined cleanup plan](docs/planning/phases/0.3-cleanup.md); the code passed local checks, Quality CI and the tag-triggered release workflow. The [V03 matrix](docs/planning/phases/0.3/validation.md) maps automated coverage. CodePact 0.4.0 was published as a synthetic preview on 2026-11-10. Its phase gates, G1–G8, and registry verification are complete. [0.4.0 release evidence](docs/releases/0.4.0-evidence.md)

This document is the authoritative release sequence. The original [feature milestones](docs/planning/roadmap.md) and [implementation phases](docs/planning/implementation.md) remain technical work groupings; they are mapped below rather than treated as version numbers.

## Release phases

| Release line | Outcome | Phase plan and gates | Earlier milestone |
| --- | --- | --- | --- |
| 0.1.x | Foundations and feasibility | [0.1 plan](docs/planning/phases/0.1-foundations.md) | M0 |
| 0.2.x | Mixed-interface lifecycle checking | [0.2 plan](docs/planning/phases/0.2-lifecycle.md) | M1 |
| 0.3.x | Cleanup obligations and bounded async cancellation/cleanup | [0.3 plan](docs/planning/phases/0.3-cleanup.md) | M2 part 1 |
| 0.4.x | Adoption tooling and synthetic preview | [0.4 plan](docs/planning/phases/0.4-adoption.md) | M2 part 2 |
| 0.5.x | Explicit ownership, lexical borrowing and structured task lifetimes | [0.5 plan](docs/planning/phases/0.5-ownership.md) | M3 |
| 0.6.x | Suspension-aware exclusive access, typestate and effects | [0.6 plan](docs/planning/phases/0.6-advanced-contracts.md) | M4 |
| 0.7.x | Typed outcomes, helper and CLI install split, and an installed-wheel must-use preview | [0.7 plan](docs/planning/phases/0.7-typed-outcomes.md) | M5 runtime portion |
| 0.8.x | Reviewed contracts and concurrency foundations | [0.8 plan](docs/planning/phases/0.8-reviewed-concurrency-foundations.md) | M5 integration portion |
| 0.9.x | Persistent obligations and before-and-after review | [0.9 plan](docs/planning/phases/0.9-obligations-and-change-review.md) | M5 integration portion |
| 0.10.x | Queues, IPC and synchronization | [0.10 plan](docs/planning/phases/0.10-queues-ipc-and-synchronization.md) | M5 integration portion |
| 0.11.x | Shutdown, async resources and executor bridges | [0.11 plan](docs/planning/phases/0.11-shutdown-and-bridges.md) | M5 integration portion |
| 0.12.x | Portfolio adoption, broader models and integrations | [0.12 plan](docs/planning/phases/0.12-ecosystem-and-integrations.md) | M5 integration portion |
| 0.13.x | Stabilization and 1.0 readiness | [0.13 plan](docs/planning/phases/0.13-stabilization.md) | M5 stability portion |

## Sequence and dependencies

The released 0.8 foundations are the basis for the upcoming obligations,
change review, and library model work. Release acceptance follows these dependencies:

| Phase | Dependency and completion |
| --- | --- |
| 0.8 | Builds on published 0.7: reviewed provider evidence and the first useful asyncio/multiprocessing lifecycle slice. |
| 0.9 | Uses 0.8 identities and semantics for persistent obligations, change review and independently qualified HTTP/SQL profiles. |
| 0.10 | Uses responsibility tracking for queue, IPC and synchronization protocols. |
| 0.11 | Composes shutdown, async resources, executor bridges and bounded shared-memory ownership. |
| 0.12 | Qualifies broader portfolio/native/framework models, persistent caches, editors and MCP. |
| 0.13 | Audits every preceding claim and makes the recorded 1.0 readiness decision. |

Release acceptance is sequential by default. Independent prototypes may run earlier. Ruff AST resolution, declaration identities and in-memory Salsa queries remain foundational; prototype progress does not complete a later release gate. Early work does not waive dependencies or required gates. Update this roadmap if the sequence or supported scope changes.

The [0.4 adoption plan](docs/planning/phases/0.4-adoption.md), [specification](docs/planning/phases/0.4/specification.md) and [V04 matrix](docs/planning/phases/0.4/validation.md) define the adoption release. Maintainer-approved [AD01-A2](docs/planning/phases/0.4/synthetic-preview-protocol.md) makes 0.4 a synthetic preview: independent bug/repair and unsupported-boundary controls replace the original field-pilot exit criterion. Real-world usefulness remains unqualified; original field observations are retained as history. The maintainer approved the release and the final artifact matrix passed. CS02 preserves 0.3's modeled async evidence without pulling task ownership forward from 0.5.

CodePact 0.5.0 was published and verified on November 10, 2026. The [0.5 ownership plan](docs/planning/phases/0.5-ownership.md), [specification](docs/planning/phases/0.5/specification.md), and [V05 matrix](docs/planning/phases/0.5/validation.md) define the opt-in transfer, lexical borrow, and TaskGroup subset. O01–O07, CS03, and G1–G8 are complete. [0.5.0 release evidence](docs/releases/0.5.0-evidence.md)

CodePact 0.6.0 was published and verified. T01–T07, CS04 and G1–G8 passed for the qualified advanced-contracts preview. Its [release evidence](docs/releases/0.6.0-evidence.md) establishes the entry baseline for 0.7.

CodePact 0.7.0 was published and verified on PyPI and crates.io. R01–R10, CS05 and G1–G8 passed for typed outcomes, finite-match preview, the helper/CLI install split and installed-wheel must-use. [0.7.0 release evidence](docs/releases/0.7.0-evidence.md) is the entry baseline for 0.8. Persistent MustHandle obligations remain future work; the new roadmap does not expand released 0.7 behavior.

The 0.4.0 release includes CLI reporting foundations. The [linting and IDE diagnostics plan](docs/planning/linting-and-editors.md) keeps a shared language server plus VS Code, PyCharm/IntelliJ, Neovim and Emacs integration targets in 0.12. Acceptance requires CLI/editor parity, correct source highlighting, current-revision hints and verified setup for each claimed editor; those integrations remain planned.

The 0.4.0 release includes structured analysis schemas, repair explanations and an opt-in check–edit–recheck workflow. The [AI-agent integration plan](docs/planning/ai-agent-integration.md) keeps local read-only MCP tools and qualified agent clients in 0.12. Acceptance requires preserved certainty/coverage, current-source evidence, CLI/tool parity and verified repairs without weakening project policy; those integrations remain planned.

The planned [TypePact combined product](docs/planning/typepact.md) reuses the TypePact name for ordinary type checking through pinned embedded Pyrefly plus CodePact behavioral analysis. CodePact remains the independent contract engine and helper library. TypePact has its own proposed `typepact` / `typepact-cli` distributions, `typepact` command, independently versioned alpha sequence and `release-typepact.yml` workflow. PF01–PF05 and PF-V01–PF-V08 qualify embedding, reporting, engine compatibility, packaging and upstream upgrades. It may progress alongside the core roadmap without blocking it; no combined release or publisher configuration is qualified yet.

After the composition alpha, TypePact's selected TC01–TC03 stages add inferred resource-model selection, branch-aware contracts and ownership obligations through typed outcomes. The versioned fact bridge and TC-V01–TC-V06 require qualified evidence and observable value beyond separate checking. TC03 depends on the applicable core ownership and typed-outcome gates; these stages do not expand the alpha or reorder CodePact's release sequence.

The [concurrency plan](docs/planning/concurrency.md) preserves completed CS01–CS05 for 0.3–0.7. New AS01–AS07, MP01–MP06 and BR01–BR02 qualify asyncio, multiprocessing and executor bridges across 0.8–0.11. Both stdlib tracks start in 0.8, with explicit spawn and worker-local resource factories as process controls. AS07 makes bounded async operation-sharing evidence a 0.9 prerequisite for the named HTTP and SQL profiles: shared-client requests and task-exclusive session operations have distinct contracts. CS06 retains integration/model/cache parity in 0.12; CS07 retains the stable-scope audit in 0.13.

Stop requests, underlying completion, exception observation, typed-result acknowledgment and cleanup remain separate. Each support claim names exact Python/platform/start-method/provider evidence. Serialized values are not ownership moves. Lifecycle/protocol models do not prove generic race freedom, deadlock freedom, thread safety or process isolation.

Original E01–E12 and S01–S09 definitions remain verbatim in the [carry-forward registry](docs/planning/phases/carry-forward-gates.md). E11 completes in 0.8, E12 in 0.9, E01–E10 in 0.12 and S01–S09 in 0.13. E04/E05 recur in every applicable phase. E10 has mandatory checkpoints: E10-A artifact integrity plus author export/verification in 0.8; E10-B published-artifact comparison in 0.9; E10-C supported-editor parity and composite completion in 0.12. Later composite completion never waives an earlier checkpoint.

The [contract and portfolio workstreams](docs/planning/contract-and-portfolio-workstreams.md) retain library LC, review CR, outcomes OH and portfolio PC commitments. Modernization profiles distinguish runtime support ranges, analyzed targets and dependency artifacts. Adopt incrementally across large packages with complete named scope and before/after evidence; first-party wrapper publication follows qualified behavior.

## Follow-on contract phases

### Published contracts and Python installs (0.7+)

- **0.7** makes `codepact` the helper-only install, `codepact[cli]` the helper plus matching CLI, and `codepact-cli` the standalone command. The opt-in `py.codepact` preview checks discarded synchronous results across installed wheels. Finite-match checking covers resolved outcome variants, with resource payload tracking deferred to TypePact TC03. [0.7 scope](docs/planning/phases/0.7-typed-outcomes.md#finite-matches)
- **0.8** qualifies provider identity, resource/ownership effects, inline/manifest foundations, installed author export/wheel verification and exact-version project models, alongside initial asyncio/process lifecycle checks. [Foundations](docs/planning/phases/0.8-reviewed-concurrency-foundations.md)
- **0.9** adds [opt-in persistent outcome/work duties](docs/planning/persistent-outcomes.md), [before-and-after review with explicit preservation verdicts](docs/planning/change-review.md), and author contract-artifact comparison. HTTPX and HTTPX2 qualify separately; SQLAlchemy and SQLModel qualify separately. Direct-await async profiles promote independently after their required AS07 sharing controls. [Obligations and review](docs/planning/phases/0.9-obligations-and-change-review.md)
- **0.10–0.11** extend queues, IPC, guards, shutdown, streams/subprocesses, bridges and bounded shared-memory ownership through explicit semantic gates.
- **0.12** completes broader native/framework/first-party adoption, persistent caches and editor/agent parity. [Integrations](docs/planning/phases/0.12-ecosystem-and-integrations.md)
- **0.13** rehearses upgrades and audits markers, manifests, policies and tooling for 1.0. Further capabilities enter support only after qualification. [Readiness](docs/planning/phases/0.13-stabilization.md)

[Published contracts design](docs/planning/published-contracts.md) · [Packaging decision](docs/planning/decisions.md)

### Strict mode and contract adoption (0.7+)

Help projects find their first useful declaration, then let them require
explicit contracts where callers depend on cleanup ownership or result
handling. Advisory and strict use the same analysis. Strict continues to fail
on incomplete coverage. Projects can preview and opt into additional declaration
requirements. Review hints add no failure by themselves.

**0.7 — Released discovery and guidance.** The published release places the
missing ownership diagnostic on the function signature and relates the return
and acquisition sites. Its qualified hints suggest Owned[T] for a uniform
modeled open-owned return, with distinct guidance for borrowed, closed and
mixed returns and finally cleanup. A resolved contextlib.closing use can
suggest a resource declaration on an eligible local class. Direct synchronous
must-use forwarding has a separately qualified advisory hint. These hints
preserve the existing strict exit policy. [Released work and R10 evidence](docs/planning/phases/0.7-typed-outcomes.md#work-packages)

**0.8 — Supply reviewed dependency contracts.** Let
projects configure versioned contract files for installed packages. The first
slice identifies an exact distribution version, a qualified resource and its
cleanup method, and a resolved factory that returns a fresh open owned resource.
Verify applicability against the selected environment's package metadata, with
`uv.lock` as the first lockfile to qualify for consistency checks. Use the same
provider resolution as published contracts. Explicit version-bounded overrides
record what they replace. A successful close discharges cleanup under the
supplied contract. A raised close keeps cleanup unknown unless the contract
specifies its exceptional effect. [Model selection and policy](docs/planning/published-contracts.md#models-policy-and-downstream-reports)

**0.9 — Enforce selected boundaries and review obligations.** Add rule codes for missing declarations on supported synchronous signatures.
Cover unspecified access on known resource parameters, supported ownership
handoffs, and project-defined requirements on named APIs or selected modules,
including `@must_use`. Let projects preview, baseline, and enforce those rules
independently. A missing required declaration is definite. Enforcement follows
the project's opt-in strict policy and leaves underlying coverage visible.
Stale or ambiguous dependency contracts cause incomplete coverage at affected
calls. Malformed or conflicting project contract files exit `2`. Reports carry
the contract's origin, symbol, resolved version, and project-supplied
provenance. Contract changes
invalidate affected analysis, snapshots, and incompatible baselines. [0.9 plan](docs/planning/phases/0.9-obligations-and-change-review.md)

**0.13 — Qualify and review the policy for 1.0.** Rehearse adoption from preview
through enforcement and test dependency upgrades. Review each rule for stable
opt-in support before considering any change to strict defaults. Follow-ons
include `aclosing` discovery and propagation from a qualified must-use result
type once typed outcomes are supported. Usage patterns such as callers usually
checking a status result can supply optional review hints. [0.13 plan](docs/planning/phases/0.13-stabilization.md)

**Acceptance evidence.** [R10](docs/planning/phases/0.7-typed-outcomes.md#required-exit-gates)
qualifies the ownership diagnostic and advisory hints.
[E11–E12](docs/planning/phases/carry-forward-gates.md#ecosystem-gates) qualify
project-supplied models and declaration policy.
[S09](docs/planning/phases/carry-forward-gates.md#s09) rehearses
adoption and dependency upgrades and records what qualifies for 1.0.

### Contracts on methods and values (0.7+)

Start with bare `@resource`, a method-level `@closes` marker, and recognition
of a class-defined `__close__` as the fallback. Follow with method-level
post-close access and repeat-close policies, then source-checked context-manager
cleanup. These are post-0.7 candidates; they do not expand the released 0.7 scope. [Resource declaration plan](docs/planning/phases/contract-extensions.md#resource-declarations-and-cleanup)

Use the existing `@must_use` marker with the typed outcomes released in 0.7. Persistent acknowledgment follows OH01–OH07 in 0.9 and does not retroactively expand that preview.
Ownership tracking through outcome payloads follows the TypePact TC03 gates.
State predicates build on qualified typestate and branch analysis. Parameter
constraints and state-dependent field annotations build on the follow-on value
and field checks below. [Typed outcomes plan](docs/planning/phases/0.7-typed-outcomes.md) · [State predicates and typed outcomes](docs/planning/phases/contract-extensions.md#state-predicates-and-typed-outcomes)

The [contract extensions plan](docs/planning/phases/contract-extensions.md)
proposes validated values, relationships between values, typestate field
guarantees, and declared dependency boundaries. Scalar validation precedes
relationship checking. Field guarantees build on 0.6 typestate. Dependency
boundaries can be prototyped independently. Consider release assignments at
or after the 0.13 readiness decision for those four phases.

## Version policy

The first accepted release in each line is 0.N.0. Use 0.N.patch for corrections and compatible improvements within that phase's scope. Public API or semantic breaks belong in a new minor line with migration guidance. New default diagnostics can affect CI and require explicit release notes even when APIs do not change.

Keep Python distribution, analyzer CLI and published codepact crate versions aligned. Versions express planned scope; they are not calendar commitments. Estimates follow implementation evidence and available maintainers.

The existing workflow publishes only exact stable-format tags such as v0.2.0. Candidate revisions are reviewed as commits/build artifacts; suffix tags such as v0.2.0rc1 do not pass its validation. Automated prerelease publishing would require a separate workflow change.

## Common definition of done

Every phase must pass [G1–G8](docs/planning/release-gates.md) plus its numbered phase gates. Record evidence using the [release evidence template](docs/planning/release-evidence-template.md). Gates start pending. CI success alone does not approve product scope or safety claims.

Each phase owner is assigned before work starts; the accountable maintainer may serve as phase owner, validator, reviewer and release approver. The maintainer can validate and approve their own work alone; independent review and second-person sign-off are optional, with no waiver required. See the [maintainer authority policy](docs/planning/release-gates.md#maintainer-authority). A required gate that fails or lacks evidence holds the release. A scope amendment must explain the change and update affected APIs, limitations, phase documents and user claims.

## First executable release

The [0.1 plan](docs/planning/phases/0.1-foundations.md) is complete: its pure Python helper, matching CLI package, Rust crate, eight native CLI wheel targets, source archives, and F01–F08/G1–G8 evidence are recorded in the [release evidence](docs/releases/0.1.0-evidence.md). The [0.1 prototype](docs/planning/phases/0.1/prototype.md) and [validation matrix](docs/planning/phases/0.1/validation.md) remain historical references. The [0.2 lifecycle release](docs/releases/0.2.0-evidence.md) is also complete. CodePact 0.3.0 implements cleanup behavior under the [phase plan](docs/planning/phases/0.3-cleanup.md); its release qualification, publication and registry verification are recorded in the [release evidence](docs/releases/0.3.0-evidence.md).

## 1.0 boundary

0.13 ends in a recorded readiness decision. Stable scope requires demonstrated cross-interface parity, trustworthy diagnostics, supported-platform artifacts, migration documentation and maintenance commitments. General ownership for arbitrary Python, memory safety, data-race freedom, advanced lifetime systems and Rust trait coherence remain outside guaranteed scope unless separately designed and proven within explicit boundaries.

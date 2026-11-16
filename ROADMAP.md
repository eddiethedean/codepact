# CodePact 0.x release roadmap

Planning baseline: 2026-10-31; release status updated from the retained evidence. **The predecessor project, TypePact, published and qualified 0.1.0 and 0.2.0 under the `typepact` package names.** The repository and project are now CodePact. CodePact 0.3.0 uses the new `codepact` Python package, `codepact-cli` command distribution, `codepact` CLI and Rust crate. The release implements cleanup behavior in the [refined cleanup plan](docs/planning/phases/0.3-cleanup.md); the code passed local checks, Quality CI and the tag-triggered release workflow. The [V03 matrix](docs/planning/phases/0.3/validation.md) maps automated coverage. CodePact 0.4.0 was published as a synthetic preview on 2026-11-10. Its phase gates, G1–G8, and registry verification are complete. [0.4.0 release evidence](docs/releases/0.4.0-evidence.md)

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
| 0.8.x | Ecosystem integration, cross-package resource contracts, and wheel manifest tooling | [0.8 plan](docs/planning/phases/0.8-ecosystem.md) | M5 integration portion |
| 0.9.x | Stabilization, published-contract compatibility, and 1.0 readiness | [0.9 plan](docs/planning/phases/0.9-stabilization.md) | M5 stability portion |

## Sequence and dependencies

~~~mermaid
flowchart LR
  A["0.1 Foundations"] --> B["0.2 Lifecycle"]
  B --> C["0.3 Cleanup"]
  C --> D["0.4 Adoption"]
  D --> E["0.5 Ownership"]
  E --> F["0.6 Advanced contracts"]
  F --> G["0.7 Typed outcomes"]
  G --> H["0.8 Ecosystem"]
  H --> I["0.9 Stabilization"]
  I --> J["1.0 readiness decision"]
~~~

Release acceptance is sequential by default. Independent prototypes may run earlier: Result/Option work can begin after 0.2, and Ruff AST resolution, declaration identities and in-memory Salsa queries are required in 0.1. Early work does not waive dependencies or required gates. Update this roadmap if the sequence or supported scope changes.

The [0.4 adoption plan](docs/planning/phases/0.4-adoption.md), [specification](docs/planning/phases/0.4/specification.md) and [V04 matrix](docs/planning/phases/0.4/validation.md) define the adoption release. Maintainer-approved [AD01-A2](docs/planning/phases/0.4/synthetic-preview-protocol.md) makes 0.4 a synthetic preview: independent bug/repair and unsupported-boundary controls replace the original field-pilot exit criterion. Real-world usefulness remains unqualified; original field observations are retained as history. The maintainer approved the release and the final artifact matrix passed. CS02 preserves 0.3's modeled async evidence without pulling task ownership forward from 0.5.

CodePact 0.5.0 was published and verified on November 10, 2026. The [0.5 ownership plan](docs/planning/phases/0.5-ownership.md), [specification](docs/planning/phases/0.5/specification.md), and [V05 matrix](docs/planning/phases/0.5/validation.md) define the opt-in transfer, lexical borrow, and TaskGroup subset. O01–O07, CS03, and G1–G8 are complete. [0.5.0 release evidence](docs/releases/0.5.0-evidence.md)

The [0.6 advanced contracts plan](docs/planning/phases/0.6-advanced-contracts.md) builds on the published 0.5 scope. A preview implementation is documented in the [specification](docs/planning/phases/0.6/specification.md); the [V06 matrix](docs/planning/phases/0.6/validation.md) remains pending for release qualification. T01–T07 and CS04 remain pending.

The 0.4.0 release includes CLI reporting foundations. The [linting and IDE diagnostics plan](docs/planning/linting-and-editors.md) keeps a shared language server plus VS Code, PyCharm/IntelliJ, Neovim and Emacs integration targets in 0.8. Acceptance requires CLI/editor parity, correct source highlighting, current-revision hints and verified setup for each claimed editor; those integrations remain planned.

The 0.4.0 release includes structured analysis schemas, repair explanations and an opt-in check–edit–recheck workflow. The [AI-agent integration plan](docs/planning/ai-agent-integration.md) keeps local read-only MCP tools and qualified agent clients in 0.8. Acceptance requires preserved certainty/coverage, current-source evidence, CLI/tool parity and verified repairs without weakening project policy; those integrations remain planned.

The planned [TypePact combined product](docs/planning/typepact.md) reuses the TypePact name for ordinary type checking through pinned embedded Pyrefly plus CodePact behavioral analysis. CodePact remains the independent contract engine and helper library. TypePact has its own proposed `typepact` / `typepact-cli` distributions, `typepact` command, independently versioned alpha sequence and `release-typepact.yml` workflow. PF01–PF05 and PF-V01–PF-V08 qualify embedding, reporting, engine compatibility, packaging and upstream upgrades. It may progress alongside the core roadmap without blocking it; no combined release or publisher configuration is qualified yet.

After the composition alpha, TypePact's selected TC01–TC03 stages add inferred resource-model selection, branch-aware contracts and ownership obligations through typed outcomes. The versioned fact bridge and TC-V01–TC-V06 require qualified evidence and observable value beyond separate checking. TC03 depends on the applicable core ownership and typed-outcome gates; these stages do not expand the alpha or reorder CodePact's release sequence.

The [concurrent resource safety plan](docs/planning/concurrency.md) adds bounded async cleanup and cancellation foundations in 0.3 (CS01), reporting and synthetic async controls in 0.4 under AD01-A2 (CS02), opt-in structured task ownership/borrow checking in 0.5 (CS03), suspension-aware exclusivity and protocol states in 0.6 (CS04), cancellation-preserving outcomes in 0.7 (CS05), qualified models and integration parity in 0.8 (CS06), and a stable-scope audit in 0.9 (CS07). Start task-lifetime modeling with `asyncio.TaskGroup` in 0.5; 0.3 qualifies intratask cleanup only. Multiprocessing and thread handoffs remain conservative escape boundaries until separately qualified; no race, deadlock, Send/Sync or general process-ownership guarantee is added. The published synchronous 0.2 scope is unchanged.

## Follow-on contract phases

### Published contracts and Python installs (0.7+)

- **0.7** makes `codepact` the helper-only install, `codepact[cli]` the helper plus matching CLI, and `codepact-cli` the standalone command. It also targets an opt-in `py.codepact` preview for discarded synchronous results across an installed wheel.
- **0.8** extends the preview to qualified resource and ownership effects, versioned wheel manifests, and author commands to export, verify, and compare contracts. Editor and cache integration share the CLI's provider selection and evidence.
- **0.9** rehearses contract upgrades and audits the marker, manifest, policies, and tooling for 1.0 readiness. Further capabilities and provider layouts enter support after their own qualification.

[Published contracts design](docs/planning/published-contracts.md) · [Packaging decision](docs/planning/decisions.md)

### Strict mode and contract adoption (0.7+)

Help projects find their first useful declaration, then let them require
explicit contracts where callers depend on cleanup ownership or result
handling. Advisory and strict use the same analysis. Strict continues to fail
on incomplete coverage. Projects can preview and opt into additional declaration
requirements. Review hints add no failure by themselves.

**0.7 — Find and explain gaps.** Put the existing missing ownership diagnostic
on the function signature and relate the return and acquisition sites. Suggest
`-> Owned[T]` when each modeled normal completion hands off an open owned
resource of that type. Give different guidance for borrowed or closed returns,
mixed outcomes, and cleanup in `finally`. For a resolved
`contextlib.closing(LocalType(...))` call, suggest `@resource(closed_by="close")`
once on an undeclared local class with a `close` method, with the call as
evidence. Preview a `@must_use` hint for synchronous wrappers that directly
forward the result of a resolved must-use callable. [0.7 work packages](docs/planning/phases/0.7-typed-outcomes.md#work-packages)

**0.8 — Supply dependency contracts and enforce selected boundaries.** Let
projects configure versioned contract files for installed packages. The first
slice identifies an exact distribution version, a qualified resource and its
cleanup method, and a resolved factory that returns a fresh open owned resource.
Verify applicability against the selected environment's package metadata, with
`uv.lock` as the first lockfile to qualify for consistency checks. Use the same
provider resolution as published contracts. Explicit version-bounded overrides
record what they replace. A successful close discharges cleanup under the
supplied contract. A raised close keeps cleanup unknown unless the contract
specifies its exceptional effect. [Model selection and policy](docs/planning/published-contracts.md#models-policy-and-downstream-reports)

Add rule codes for missing declarations on supported synchronous signatures.
Cover unspecified access on known resource parameters, supported ownership
handoffs, and project-defined requirements on named APIs or selected modules,
including `@must_use`. Let projects preview, baseline, and enforce those rules
independently. A missing required declaration is definite. Enforcement follows
the project's opt-in strict policy and leaves underlying coverage visible.
Stale or ambiguous dependency contracts cause incomplete coverage at affected
calls. Malformed or conflicting project contract files exit `2`. Reports carry
the contract's origin, symbol, resolved version, and project-supplied
provenance. Contract changes
invalidate affected analysis, snapshots, and incompatible baselines. [0.8 work packages](docs/planning/phases/0.8-ecosystem.md#work-packages)

**0.9 — Qualify and review the policy for 1.0.** Rehearse adoption from preview
through enforcement and test dependency upgrades. Review each rule for stable
opt-in support before considering any change to strict defaults. Follow-ons
include `aclosing` discovery and propagation from a qualified must-use result
type once typed outcomes are supported. Usage patterns such as callers usually
checking a status result can supply optional review hints. [0.9 work packages](docs/planning/phases/0.9-stabilization.md#work-packages)

**Acceptance evidence.** Qualify an annotation-free class that receives one
hint and a shadowed lookalike that receives none. Test a version-pinned dependency
leak and undocumented handoff, their separate passing repairs, and a stale model
after a dependency upgrade. Verify that preview, strict enforcement, and
baselines work together while incomplete coverage remains visible. Compare
fresh and reused results after contract changes. Verify that weak must-use hints
alone leave the exit status unchanged and that a wrapper forwarding a resolved
must-use result receives a hint.

### Contracts on methods and values (0.7+)

Start with bare `@resource`, a method-level `@closes` marker, and recognition
of a class-defined `__close__` as the fallback. Follow with method-level
post-close access and repeat-close policies, then source-checked context-manager
cleanup. These are candidates from 0.7 onward. [Resource declaration plan](docs/planning/phases/contract-extensions.md#resource-declarations-and-cleanup)

Use the existing `@must_use` marker with the typed outcomes planned for 0.7.
Ownership tracking through outcome payloads follows the TypePact TC03 gates.
State predicates build on qualified typestate and branch analysis. Parameter
constraints and state-dependent field annotations build on the follow-on value
and field checks below. [Typed outcomes plan](docs/planning/phases/0.7-typed-outcomes.md) · [State predicates and typed outcomes](docs/planning/phases/contract-extensions.md#state-predicates-and-typed-outcomes)

The [contract extensions plan](docs/planning/phases/contract-extensions.md)
proposes validated values, relationships between values, typestate field
guarantees, and declared dependency boundaries. Scalar validation precedes
relationship checking. Field guarantees build on 0.6 typestate. Dependency
boundaries can be prototyped independently. Consider release assignments at
or after the 0.9 readiness decision for those four phases.

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

0.9 ends in a recorded readiness decision. Stable scope requires demonstrated cross-interface parity, trustworthy diagnostics, supported-platform artifacts, migration documentation and maintenance commitments. General ownership for arbitrary Python, memory safety, data-race freedom, advanced lifetime systems and Rust trait coherence remain outside guaranteed scope unless separately designed and proven within explicit boundaries.

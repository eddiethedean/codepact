# CodePact planning index

Planning baseline: 2026-10-31; release mapping updated 2026-11-17. These documents record the direction from the “Design Rust Typing Python” discussion, concrete proposals and implementation/qualification status. Candidate code is not a released or qualified support claim; published behavior is linked to release evidence.

## Reading order

| Document | Purpose |
| --- | --- |
| [Vision](vision.md) | Audiences, goals, adoption, and success measures |
| [API design](apis.md) | Pythonic and explicit APIs, interoperability, examples |
| [Architecture](architecture.md) | Components, dependencies, execution and packaging |
| [SOLID refactor plan](solid-refactor.md) | Behavior-preserving changes across the analyzer, Python helpers, tooling, and release delivery |
| [Analyzer migration](analyzer-migration.md) | Ruff AST, declaration identities, Salsa query graph and AM01–AM06 acceptance |
| [0.1 implementation repair](phases/0.1/remediation.md) | Archived file-level repair plan; see the release evidence for completed work |
| [0.2 lifecycle plan](phases/0.2-lifecycle.md) | LC01–LC07 work, released scope, V02 matrix and performance record |
| [0.3 cleanup plan](phases/0.3-cleanup.md) | Published cleanup and bounded async behavior with verified artifacts |
| [0.4 adoption plan](phases/0.4-adoption.md) | Published synthetic preview with completed A01–A08, CS02, and G1–G8 |
| [0.5 ownership plan](phases/0.5-ownership.md) | Published opt-in transfer, lexical borrowing, and TaskGroup checks with verified release evidence |
| [0.6 advanced contracts plan](phases/0.6-advanced-contracts.md) | Published preview of exclusive access, finite typestate and protocol models |
| [0.7 typed outcomes plan](phases/0.7-typed-outcomes.md) | Published Result/Option, finite matches, install split and installed-wheel must-use preview; R01–R10, CS05 and G1–G8 passed. [Specification](phases/0.7/specification.md) · [V07 matrix](phases/0.7/validation.md) |
| [0.8 foundations](phases/0.8-reviewed-concurrency-foundations.md) | Reviewed providers and first asyncio/multiprocessing lifecycle models. [Specification](phases/0.8/specification.md) · [V08 matrix](phases/0.8/validation.md) · [Implementation evidence](phases/0.8/implementation-evidence.md) |
| [0.9 obligations and review](phases/0.9-obligations-and-change-review.md) | Persistent responsibilities, before/after comparison and separate HTTP/SQL profiles |
| [0.10 queues and IPC](phases/0.10-queues-ipc-and-synchronization.md) | Queue accounting, process communication and bounded synchronization |
| [0.11 shutdown and bridges](phases/0.11-shutdown-and-bridges.md) | Async resources, worker shutdown, executor bridges and bounded shared memory |
| [0.12 integrations](phases/0.12-ecosystem-and-integrations.md) | Portfolio/native/framework adoption, caching, editors and MCP |
| [0.13 stabilization](phases/0.13-stabilization.md) | Compatibility/adoption rehearsals and recorded 1.0 readiness |
| [Carried-forward gates](phases/carry-forward-gates.md) | Unchanged E01–E12 and S01–S09 definitions and completion ownership |
| [Contract and portfolio workstreams](contract-and-portfolio-workstreams.md) | Retained library LC, review CR, outcome OH and portfolio PC work |
| [Persistent outcomes v1](persistent-outcomes.md) | Proposed opt-in MustHandle production, identity, compatibility and acknowledgment acceptance for 0.9 |
| [Change review v1](change-review.md) | Proposed preservation prerequisites, verdicts, trusted snapshots and review exit behavior for 0.9 |
| [Shared analyzer](semantic-analyzer.md) | Canonical contracts, flow analysis, summaries and diagnostics |
| [Publishing CodePact contracts](published-contracts.md) | Package markers, installed dependency analysis, manifest design, and contract compatibility checks |
| [Concurrent resource safety](concurrency.md) | Released CS01–CS05, new AS/MP/BR semantics, CS06 integration and CS07 audit |
| [Feature specifications](features.md) | Contracts and acceptance criteria for each capability |
| [Follow-on contract phases](phases/contract-extensions.md) | Proposed 0.7+ resource declarations, state predicates, typed outcome contracts, and value, field, and dependency checks |
| [Limitations](limitations.md) | Ownership, aliasing, dynamic code and assurance boundaries |
| [Compatibility](compatibility.md) | Python typing, versions and ecosystem integration |
| [Linting and IDE diagnostics](linting-and-editors.md) | Lint command, shared language server, editor targets and validation gates |
| [AI-agent integration](ai-agent-integration.md) | Structured findings, repair context, check–edit–recheck workflows and MCP tools |
| [TypePact combined product](typepact.md) | Pinned embedded Pyrefly plus CodePact, PF composition alpha, TC01–TC03 type-informed stages, fact bridge and acceptance gates |
| [Roadmap](roadmap.md) | Priorities, dependencies and release scope |
| [Implementation](implementation.md) | Work packages and completion gates |
| [Testing and performance](testing.md) | Correctness, interoperability, benchmarks and CI |
| [Contributor plan](contributors.md) | Review, governance and onboarding |
| [Release plan](releases.md) | Distribution, versioning, release and recovery |
| [0.2.0 release checklist](release-0.2.0-checklist.md) | Candidate checks, tag publication, verification, and partial-publish recovery |
| [Decisions and risks](decisions.md) | Settled direction, proposals and open questions |

## Status vocabulary

**Direction** is established by the project discussion: CodePact branding and `codepact` Python package name; one semantic engine; two interoperable interfaces; gradual adoption; compatibility with ordinary Python typing.

**Proposal** is an implementable starting point awaiting prototype evidence. Public API spellings, package layout, support versions and milestone targets are proposals. **Research** covers capabilities whose feasibility is unresolved. A feature becomes **implemented** only when code, tests, documentation and release notes establish its supported scope.

**Preview implementation** means code and user documentation exist, while the
acceptance matrix, candidate evidence, or release qualification is incomplete.
Do not present preview behavior as a qualified support claim.

In planning documents, “must” describes an acceptance requirement and is not by itself a claim of present functionality. Release evidence records automated coverage, maintainer qualification, and publication status. Cross-interface semantic parity is a release gate.

## Repository baseline

A Ruff AST-based inspector, Python helpers, package manifests and CI workflows now exist. The predecessor TypePact's 0.1.0 and 0.2.0 releases are published and qualified under the `typepact` names. CodePact 0.3.0 was the first release using the renamed packages and crate. CodePact 0.4.0 published the adoption preview, and 0.5.0 introduced opt-in ownership. CodePact 0.6.0 adds the qualified advanced-contracts preview. CodePact 0.7.0 publishes typed outcomes, finite matches, the install split and installed-wheel must-use preview. [0.6.0 release evidence](../releases/0.6.0-evidence.md) · [0.7.0 release evidence](../releases/0.7.0-evidence.md)

## Versioned release planning

The [root ROADMAP](../../ROADMAP.md) defines the authoritative 0.1–0.13 sequence. The predecessor's 0.1.0 and 0.2.0 releases are complete. CodePact 0.3.0 through 0.7.0 passed their phase-specific and common release gates. Apply the [common release gates](release-gates.md) and retain a [release evidence record](release-evidence-template.md) for every candidate.

## Python helper and CLI distributions

D015 established the pure-Python helper and matching native CLI distribution model, inherited by CodePact under the `codepact` and `codepact-cli` names for 0.3. From 0.7, D022 moves the matching CLI dependency to `codepact[cli]`. The inventory includes eight CLI platform builds, one pure helper wheel, two source archives and the Cargo route. [Decisions](decisions.md) · [Release plan](releases.md)

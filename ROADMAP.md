# CodePact 0.x release roadmap

Planning baseline: 2026-10-31; release status updated from the retained evidence. **The predecessor project, TypePact, published and qualified 0.1.0 and 0.2.0 under the `typepact` package names.** The repository and project are now CodePact. Its 0.3.0 candidate uses the new `codepact` Python package, `codepact-cli` command distribution, `codepact` CLI and Rust crate. The candidate implements the cleanup behavior in the [refined cleanup plan](docs/planning/phases/0.3-cleanup.md), with automated evidence mapped in the [V03 matrix](docs/planning/phases/0.3/validation.md). Remaining matrix evidence, maintainer qualification and release authorization are tracked there and in the candidate record.

This document is the authoritative release sequence. The original [feature milestones](docs/planning/roadmap.md) and [implementation phases](docs/planning/implementation.md) remain technical work groupings; they are mapped below rather than treated as version numbers.

## Release phases

| Release line | Outcome | Phase plan and gates | Earlier milestone |
| --- | --- | --- | --- |
| 0.1.x | Foundations and feasibility | [0.1 plan](docs/planning/phases/0.1-foundations.md) | M0 |
| 0.2.x | Mixed-interface lifecycle checking | [0.2 plan](docs/planning/phases/0.2-lifecycle.md) | M1 |
| 0.3.x | Cleanup obligations and bounded async cancellation/cleanup | [0.3 plan](docs/planning/phases/0.3-cleanup.md) | M2 part 1 |
| 0.4.x | Adoption tooling and useful preview | [0.4 plan](docs/planning/phases/0.4-adoption.md) | M2 part 2 |
| 0.5.x | Explicit ownership, lexical borrowing and structured task lifetimes | [0.5 plan](docs/planning/phases/0.5-ownership.md) | M3 |
| 0.6.x | Suspension-aware exclusive access, typestate and effects | [0.6 plan](docs/planning/phases/0.6-advanced-contracts.md) | M4 |
| 0.7.x | Typed outcomes and finite exhaustiveness | [0.7 plan](docs/planning/phases/0.7-typed-outcomes.md) | M5 runtime portion |
| 0.8.x | Ecosystem integration, editor services and persistent caching | [0.8 plan](docs/planning/phases/0.8-ecosystem.md) | M5 integration portion |
| 0.9.x | Stabilization and 1.0 readiness | [0.9 plan](docs/planning/phases/0.9-stabilization.md) | M5 stability portion |

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

The [linting and IDE diagnostics plan](docs/planning/linting-and-editors.md) adds lint reporting foundations in 0.4 and a shared language server plus VS Code, PyCharm/IntelliJ, Neovim and Emacs integration targets in 0.8. Acceptance requires CLI/editor parity, correct source highlighting, current-revision hints and verified setup for each claimed editor. These integrations remain planned.

The [AI-agent integration plan](docs/planning/ai-agent-integration.md) adds structured analysis schemas, repair explanations and check–edit–recheck workflows in 0.4, followed by local read-only MCP tools and qualified agent clients in 0.8. Acceptance requires preserved certainty/coverage, current-source evidence, CLI/tool parity and verified repairs without weakening project policy. These integrations remain planned.

The optional [codepact-pyrefly-cli prototype](docs/planning/codepact-pyrefly-cli.md) combines embedded Pyrefly type checking with CodePact resource checks in a separate package and command. It has independent prerelease versions, PF01–PF05 work packages and PF-V01–PF-V08 gates, and may progress alongside lifecycle work without blocking the main release sequence. Its planned trusted publisher uses `release-pyrefly.yml` and environment `pypi`; embedded-library feasibility and clean packaging must pass before publication.

The [concurrent resource safety plan](docs/planning/concurrency.md) adds bounded async cleanup and cancellation foundations in 0.3 (CS01), reporting and an async pilot in 0.4 (CS02), opt-in structured task ownership/borrow checking in 0.5 (CS03), suspension-aware exclusivity and protocol states in 0.6 (CS04), cancellation-preserving outcomes in 0.7 (CS05), qualified models and integration parity in 0.8 (CS06), and a stable-scope audit in 0.9 (CS07). Start task-lifetime modeling with `asyncio.TaskGroup` in 0.5; 0.3 qualifies intratask cleanup only. Multiprocessing and thread handoffs remain conservative escape boundaries until separately qualified; no race, deadlock, Send/Sync or general process-ownership guarantee is added. The published synchronous 0.2 scope is unchanged.

## Version policy

The first accepted release in each line is 0.N.0. Use 0.N.patch for corrections and compatible improvements within that phase's scope. Public API or semantic breaks belong in a new minor line with migration guidance. New default diagnostics can affect CI and require explicit release notes even when APIs do not change.

Keep Python distribution, analyzer CLI and published codepact crate versions aligned. Versions express planned scope; they are not calendar commitments. Estimates follow implementation evidence and available maintainers.

The existing workflow publishes only exact stable-format tags such as v0.2.0. Candidate revisions are reviewed as commits/build artifacts; suffix tags such as v0.2.0rc1 do not pass its validation. Automated prerelease publishing would require a separate workflow change.

## Common definition of done

Every phase must pass [G1–G8](docs/planning/release-gates.md) plus its numbered phase gates. Record evidence using the [release evidence template](docs/planning/release-evidence-template.md). Gates start pending. CI success alone does not approve product scope or safety claims.

Each phase owner is assigned before work starts; the accountable maintainer may serve as phase owner, validator, reviewer and release approver. The maintainer can validate and approve their own work alone; independent review and second-person sign-off are optional, with no waiver required. See the [maintainer authority policy](docs/planning/release-gates.md#maintainer-authority). A required gate that fails or lacks evidence holds the release. A scope amendment must explain the change and update affected APIs, limitations, phase documents and user claims.

## First executable release

The [0.1 plan](docs/planning/phases/0.1-foundations.md) is complete: its pure Python helper, matching CLI package, Rust crate, eight native CLI wheel targets, source archives, and F01–F08/G1–G8 evidence are recorded in the [release evidence](docs/releases/0.1.0-evidence.md). The [0.1 prototype](docs/planning/phases/0.1/prototype.md) and [validation matrix](docs/planning/phases/0.1/validation.md) remain historical references. The [0.2 lifecycle release](docs/releases/0.2.0-evidence.md) is also complete. Follow the [0.3 phase plan](docs/planning/phases/0.3-cleanup.md) for the next release; its proposed cleanup semantics and gates are not shipped behavior.

## 1.0 boundary

0.9 ends in a recorded readiness decision. Stable scope requires demonstrated cross-interface parity, trustworthy diagnostics, supported-platform artifacts, migration documentation and maintenance commitments. General ownership for arbitrary Python, memory safety, data-race freedom, advanced lifetime systems and Rust trait coherence remain outside guaranteed scope unless separately designed and proven within explicit boundaries.

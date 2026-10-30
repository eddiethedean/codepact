# Typepact 0.x release roadmap

Planning baseline: 2026-10-30. **All phases are planned; no release gate has been passed.** The repository currently contains planning documents and a tag-triggered release workflow. Implementation, package manifests and release artifacts remain to be built.

This document is the authoritative release sequence. The original [feature milestones](docs/planning/roadmap.md) and [implementation phases](docs/planning/implementation.md) remain technical work groupings; they are mapped below rather than treated as version numbers.

## Release phases

| Release line | Outcome | Phase plan and gates | Earlier milestone |
| --- | --- | --- | --- |
| 0.1.x | Foundations and feasibility | [0.1 plan](docs/planning/phases/0.1-foundations.md) | M0 |
| 0.2.x | Mixed-interface lifecycle checking | [0.2 plan](docs/planning/phases/0.2-lifecycle.md) | M1 |
| 0.3.x | Cleanup and resource obligations | [0.3 plan](docs/planning/phases/0.3-cleanup.md) | M2 part 1 |
| 0.4.x | Adoption tooling and useful preview | [0.4 plan](docs/planning/phases/0.4-adoption.md) | M2 part 2 |
| 0.5.x | Explicit ownership and lexical borrowing | [0.5 plan](docs/planning/phases/0.5-ownership.md) | M3 |
| 0.6.x | Exclusive access, typestate and effects | [0.6 plan](docs/planning/phases/0.6-advanced-contracts.md) | M4 |
| 0.7.x | Typed outcomes and finite exhaustiveness | [0.7 plan](docs/planning/phases/0.7-typed-outcomes.md) | M5 runtime portion |
| 0.8.x | Ecosystem integration and incremental analysis | [0.8 plan](docs/planning/phases/0.8-ecosystem.md) | M5 integration portion |
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

Release acceptance is sequential by default. Independent prototypes may run earlier: Result/Option work can begin after 0.2, and packaging/incrementality spikes should start in 0.1. Early work does not waive dependencies or required gates. Update this roadmap if the sequence or supported scope changes.

## Version policy

The first accepted release in each line is 0.N.0. Use 0.N.patch for corrections and compatible improvements within that phase's scope. Public API or semantic breaks belong in a new minor line with migration guidance. New default diagnostics can affect CI and require explicit release notes even when APIs do not change.

Keep Python distribution, analyzer CLI and published typepact crate versions aligned. Versions express planned scope; they are not calendar commitments. Estimates follow implementation evidence and available maintainers.

The existing workflow publishes only exact stable-format tags such as v0.2.0. Candidate revisions are reviewed as commits/build artifacts; suffix tags such as v0.2.0rc1 do not pass its validation. Automated prerelease publishing would require a separate workflow change.

## Common definition of done

Every phase must pass [G1–G8](docs/planning/release-gates.md) plus its numbered phase gates. Record evidence using the [release evidence template](docs/planning/release-evidence-template.md). Gates start pending. CI success alone does not approve product scope or safety claims.

Each phase owner is assigned before work starts; maintainers record the release decision. No dates or named owners are assumed by this plan. A required gate that fails or lacks evidence holds the release. A scope amendment must explain the change and update affected APIs, limitations, phase documents and user claims.

## First executable release

The refined [0.1 plan](docs/planning/phases/0.1-foundations.md) targets a usable contract-inspection API and CLI, lightweight Python helpers, seven ordered work packages and F01–F08 acceptance gates. Its [prototype specification](docs/planning/phases/0.1/prototype.md) defines normalization and inspection outcomes; its [validation matrix](docs/planning/phases/0.1/validation.md) defines fixtures and candidate evidence. Initial delivery proposes a pure helper wheel plus a separately Cargo-installed inspector, subject to the packaging decision. Lifecycle enforcement remains in 0.2.

0.1 must produce a useful foundational preview with matching Python and Rust artifacts. Before tagging it, implement the package manifests and lockfile, select a license, prove the packaging approach and configure registry credentials. The current workflow provides build/publish plumbing; it does not yet implement the full gate suite.

## 1.0 boundary

0.9 ends in a recorded readiness decision. Stable scope requires demonstrated cross-interface parity, trustworthy diagnostics, supported-platform artifacts, migration documentation and maintenance commitments. General ownership for arbitrary Python, memory safety, data-race freedom, advanced lifetime systems and Rust trait coherence remain outside guaranteed scope unless separately designed and proven within explicit boundaries.

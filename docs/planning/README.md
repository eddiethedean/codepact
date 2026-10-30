# Typepact planning index

Planning baseline: 2026-10-30. These documents record the direction from the “Design Rust Typing Python” discussion and concrete proposals for implementation. They do not assert that features exist.

## Reading order

| Document | Purpose |
| --- | --- |
| [Vision](vision.md) | Audiences, goals, adoption, and success measures |
| [API design](apis.md) | Pythonic and explicit APIs, interoperability, examples |
| [Architecture](architecture.md) | Components, dependencies, execution and packaging |
| [Shared analyzer](semantic-analyzer.md) | Canonical contracts, flow analysis, summaries and diagnostics |
| [Feature specifications](features.md) | Contracts and acceptance criteria for each capability |
| [Limitations](limitations.md) | Ownership, aliasing, dynamic code and assurance boundaries |
| [Compatibility](compatibility.md) | Python typing, versions and ecosystem integration |
| [Roadmap](roadmap.md) | Priorities, dependencies and release scope |
| [Implementation](implementation.md) | Work packages and completion gates |
| [Testing and performance](testing.md) | Correctness, interoperability, benchmarks and CI |
| [Contributor plan](contributors.md) | Review, governance and onboarding |
| [Release plan](releases.md) | Distribution, versioning, release and recovery |
| [Decisions and risks](decisions.md) | Settled direction, proposals and open questions |

## Status vocabulary

**Direction** is established by the project discussion: Typepact branding; one semantic engine; two interoperable interfaces; gradual adoption; compatibility with ordinary Python typing.

**Proposal** is an implementable starting point awaiting prototype evidence. Public API spellings, package layout, support versions and milestone targets are proposals. **Research** covers capabilities whose feasibility is unresolved. A feature becomes **implemented** only when code, tests, documentation and release notes establish its supported scope.

Documents use “must” for a proposed acceptance requirement, not a claim of present functionality. Update this index and the feature matrix as implementation lands. Cross-interface semantic parity is a release gate.

## Repository baseline

The GitHub repository and the existing local Typepact folder were inspected before authoring and were empty. This planning set initializes documentation without replacing existing source or policies. No implementation or dependency manifests are included.

## Versioned release planning

The [root ROADMAP](../../ROADMAP.md) defines the authoritative 0.1–0.9 sequence. Each linked phase has its own work packages and numbered gates. Apply the [common release gates](release-gates.md) and retain a [release evidence record](release-evidence-template.md) for every candidate. All phase gates currently remain pending.

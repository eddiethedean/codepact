# TypePact planning index

Planning baseline: 2026-10-30. These documents record the direction from the “Design Rust Typing Python” discussion and concrete proposals for implementation. They do not assert that features exist.

## Reading order

| Document | Purpose |
| --- | --- |
| [Vision](vision.md) | Audiences, goals, adoption, and success measures |
| [API design](apis.md) | Pythonic and explicit APIs, interoperability, examples |
| [Architecture](architecture.md) | Components, dependencies, execution and packaging |
| [Analyzer migration](analyzer-migration.md) | Ruff AST, declaration identities, Salsa query graph and AM01–AM06 acceptance |
| [0.1 implementation repair](phases/0.1/remediation.md) | Current-code gap map and file-level R01–R08 execution plan |
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

**Direction** is established by the project discussion: TypePact branding and `typepact` Python package name; one semantic engine; two interoperable interfaces; gradual adoption; compatibility with ordinary Python typing.

**Proposal** is an implementable starting point awaiting prototype evidence. Public API spellings, package layout, support versions and milestone targets are proposals. **Research** covers capabilities whose feasibility is unresolved. A feature becomes **implemented** only when code, tests, documentation and release notes establish its supported scope.

Documents use “must” for a proposed acceptance requirement, not a claim of present functionality. Update this index and the feature matrix as implementation lands. Cross-interface semantic parity is a release gate.

## Repository baseline

A Ruff AST-based inspector, Python helpers, package manifests and CI workflows now exist. The implementation includes local declaration identities, captured aliases, explicitly supplied direct class and alias imports, typed AST lowering, target-version-aware parsing, per-module Salsa semantic queries, cancellation recovery, revision-tagged session results and bounded work/cache budgets. The complete V01–V24 and F01–F08 qualification matrix remains open; 0.1 candidate qualification remains pending.

## Versioned release planning

The [root ROADMAP](../../ROADMAP.md) defines the authoritative 0.1–0.9 sequence. Each linked phase has its own work packages and numbered gates. Apply the [common release gates](release-gates.md) and retain a [release evidence record](release-evidence-template.md) for every candidate. All phase gates currently remain pending.

## Python helper and CLI distributions

D015 keeps typepact pure Python and pins the matching typepact-cli package from 0.1, with eight CLI platform builds, one pure helper wheel and two source archives. Cargo distribution remains available. The release plan is authoritative for the target matrix; later phases inherit it.

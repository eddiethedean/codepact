# CodePact planning index

Planning baseline: 2026-10-31. These documents record the direction from the “Design Rust Typing Python” discussion and concrete proposals for implementation. They do not assert that planned features exist; current release facts are linked to the evidence records.

## Reading order

| Document | Purpose |
| --- | --- |
| [Vision](vision.md) | Audiences, goals, adoption, and success measures |
| [API design](apis.md) | Pythonic and explicit APIs, interoperability, examples |
| [Architecture](architecture.md) | Components, dependencies, execution and packaging |
| [Analyzer migration](analyzer-migration.md) | Ruff AST, declaration identities, Salsa query graph and AM01–AM06 acceptance |
| [0.1 implementation repair](phases/0.1/remediation.md) | Archived file-level repair plan; see the release evidence for completed work |
| [0.2 lifecycle plan](phases/0.2-lifecycle.md) | LC01–LC07 work, released scope, V02 matrix and performance record |
| [0.3 cleanup plan](phases/0.3-cleanup.md) | Proposed CL01–CL08 work, cleanup/async semantics and pending V03 acceptance matrix |
| [Shared analyzer](semantic-analyzer.md) | Canonical contracts, flow analysis, summaries and diagnostics |
| [Concurrent resource safety](concurrency.md) | Structured async lifetimes, cancellation, process boundaries and CS01–CS07 gates |
| [Feature specifications](features.md) | Contracts and acceptance criteria for each capability |
| [Limitations](limitations.md) | Ownership, aliasing, dynamic code and assurance boundaries |
| [Compatibility](compatibility.md) | Python typing, versions and ecosystem integration |
| [Linting and IDE diagnostics](linting-and-editors.md) | Lint command, shared language server, editor targets and validation gates |
| [AI-agent integration](ai-agent-integration.md) | Structured findings, repair context, check–edit–recheck workflows and MCP tools |
| [TypePact combined product](typepact.md) | Pinned embedded Pyrefly plus CodePact, package split, historical-name migration, PF01–PF05 work and acceptance gates |
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

Documents use “must” for a proposed acceptance requirement, not a claim of present functionality. Update this index and the feature matrix as implementation lands. Cross-interface semantic parity is a release gate.

## Repository baseline

A Ruff AST-based inspector, Python helpers, package manifests and CI workflows now exist. The predecessor TypePact's 0.1.0 and 0.2.0 releases are published and qualified under the `typepact` names; see their historical phase plans, validation matrices, and release evidence. CodePact 0.3.0 is the first candidate using the renamed packages and crate.

## Versioned release planning

The [root ROADMAP](../../ROADMAP.md) defines the authoritative 0.1–0.9 sequence. The predecessor's 0.1.0 and 0.2.0 releases are complete; CodePact 0.3.0 gates remain pending until candidate evidence and the maintainer's decision are recorded. Apply the [common release gates](release-gates.md) and retain a [release evidence record](release-evidence-template.md) for every candidate.

## Python helper and CLI distributions

D015 keeps codepact pure Python and pins the matching codepact-cli package from 0.1, with eight CLI platform builds, one pure helper wheel and two source archives. Cargo distribution remains available. The release plan is authoritative for the target matrix; later phases inherit it.

# Implementation phases

## Phase 0: establish executable contracts

Create workspace manifests and lightweight Python package only after the design choices are reviewed. Select a compatible open-source license before distributing implementation. Prototype Annotated aliases and parser support; record results in decisions. Define IR types, source spans, diagnostic schema and fixture format.

Completion: one equivalent pair lowers to the same canonical contract; ordinary checkers retain underlying types; package import does not initialize the analyzer.

## Phase 1: deliver the mixed-interface slice

Implement module discovery, parsing, name resolution and resource decorator recognition. Add explicit Borrowed lowering, normal control flow and a minimal call-summary mechanism. Track local aliases and object lifecycle. Add human-readable CLI output.

Completion: the documented Connection example produces exactly the intended post-close diagnostic, with close and use locations. Valid calls, unrelated symbols named resource, imported aliases, branch joins and unknown calls have defined outcomes.

## Phase 2: cover real cleanup and adoption

Add exception edges, finally, context-manager models, early returns and loops. Introduce cleanup obligations and initial must-use checks. Add pyproject configuration, machine output, suppression reasons, baseline support and analysis coverage reporting.

Completion: pilot projects can run in CI with stable exit codes; modeled cleanup works on normal and exceptional paths; unresolved behavior is visible; baseline updates cannot silently hide new rules.

## Phase 3: introduce ownership experimentally

Add explicit move semantics, handle validity, ownership obligation transfer and lexical borrow regions. Verify callee preconditions/effects across supported modules. Define recursive-summary widening and escape policies.

Completion: use-after-move, double transfer, close-through-borrow and known conflicting aliases are diagnosed. Containers, callbacks and foreign code cannot silently restore uniqueness. Experimental status is documented.

## Phase 4: expand contract expressiveness

Implement finite typestate, exclusive borrowing and bounded effect summaries. Add runtime variants and explicit conversion adapters if their API RFC is accepted. Develop finite exhaustiveness checks without assuming guarded patterns cover all values.

Completion: feature-specific gates in features.md pass; both frontends produce the same relevant facts and diagnostics.

## Phase 5: prepare stable distribution

Finish platform artifacts, version compatibility, cache invalidation, documentation, editor protocol and release automation. Run the release candidate against the pilot corpus and published benchmark suite.

Completion: release checklist is satisfied, remaining limitations are published, and API/diagnostic stability commitments have maintainers.

## Work item template

Each implementation issue should name the user problem, proposed contract, affected IR fields, supported scope, negative examples, diagnostic behavior, compatibility impact, fixtures, documentation and completion evidence.

Do not convert every future idea into a mandatory dependency. Keep each change small enough to review one semantic extension at a time.

## Release mapping

These engineering phases group work by subsystem. The [root ROADMAP](../../ROADMAP.md) maps them to versioned 0.x releases and defines the dependency order. Use the linked phase plans and [common gates](release-gates.md) for release acceptance.

# Product vision

## Mission

Bring useful Rust-inspired contracts to ordinary Python through two interoperable ways to express them. Developers should choose their level of explicitness per function, library or team, while receiving consistent analysis.

The Pythonic interface favors decorators, native types, context managers and inference. The explicit interface exposes ownership, borrowing, typed outcomes and state annotations. The analyzer assigns the same meaning to equivalent contracts.

## Audiences and problems

| Audience | Problem | First useful outcome |
| --- | --- | --- |
| Application teams | Resources remain in use after close or escape cleanup | Precise lifecycle diagnostics with source locations |
| Async application teams | Background tasks outlive resources or cancellation interrupts cleanup | Modeled structured task lifetimes and explicit unsupported handoffs |
| Library maintainers | Callers misunderstand consumption or result handling | Published contracts and actionable caller diagnostics |
| Rust developers using Python | Ownership intent is hard to express | Familiar opt-in annotations with explicit limitations |
| Existing typing users | General type checks miss lifecycle mistakes | An additional analyzer that works beside their checker |

## Principles

1. Use valid Python and retain normal object identity and runtime behavior for metadata-only contracts.
2. Normalize both interfaces into one semantic vocabulary.
3. Explain certainty, assumptions and unsupported boundaries.
4. Start with narrow, useful checks and expand only with evidence.
5. Require explicit conversions when runtime representations differ.
6. Keep runtime imports lightweight; analysis must not import or execute target code.
7. Make adoption incremental, reversible and possible within a single module.

## Product scope

Initial scope is lifecycle checking, cross-interface normalization, a CLI and stable diagnostic identifiers. Later scope includes must-use results, resource obligation tracking, scoped ownership, exclusive borrowing and typestate.

A general replacement for mypy or Pyright, a Python compiler, and a universal memory-safety proof are outside the initial product. Native Python exceptions remain available. Users do not need to rewrite every return value into Result.

**CodePact — Behavioral contracts for Python.** The planned companion product, [TypePact — Type checking and behavioral contracts for Python](typepact.md), combines pinned embedded Pyrefly with the same CodePact engine in one command and report. Python declarations retain the `codepact` imports. General type semantics remain upstream-owned, and CodePact remains independently usable alongside any type checker. TypePact is independently versioned; migration and historical backward compatibility are outside its product plan.

Beyond composition, TypePact's selected extensions use type evidence to recognize modeled resources with fewer annotations, specialize contracts within narrowed branches, and track obligations inside typed outcomes. A small qualified fact bridge supplies evidence to CodePact while preserving its ownership of resource state and duties. These staged capabilities require separate acceptance and the relevant core features; they are not first-alpha claims.

## Adoption path

Begin with advisory checks and curated resource models that can find useful issues without annotations. Add explicit resource declarations where library behavior is known. Enable strict boundary checks in selected modules, then expand after triaging diagnostics. Support baselines for existing findings and fail CI on newly introduced issues.

Examples and onboarding should lead with a real resource bug, its diagnostic, and a context-manager repair. Offer the explicit API beside the Pythonic example.

## Success measures

Before a public preview, recruit at least three representative pilot projects and record confirmed bugs, false positives, unsupported cases, first useful result time and triage effort. Proposed preview gate: at least 90% of reviewed definite-violation diagnostics are actionable on the pilot corpus; publish sample size and disagreements.

Track runtime import cost, cold analysis time, incremental latency, memory and interoperability parity. These are goals to measure, not current performance claims. Avoid adoption forecasts based on branding alone.

## Analyzer foundation

The implementation direction is a Ruff AST frontend, CodePact-owned declaration and contract semantics, and Salsa incremental queries following ty's architectural separation. The [migration plan](analyzer-migration.md) makes correctness and observable reuse foundation requirements. This direction improves the basis for contract analysis without promising general Python type-checker coverage or Rust safety guarantees.

## Concurrent resource safety direction

Use the [concurrency plan](concurrency.md) to extend resource checking to structured async tasks before general concurrency analysis. Prioritize owner/borrow lifetimes, explicit handoffs, cancellation-aware cleanup and finite protocol states. Include an async application in adoption pilots and measure useful findings separately from unsupported cases. Multiprocessing requires explicit serialization, handle and start-method models; thread/process safety, lock proofs, race freedom and deadlock freedom are outside this direction's guarantees.

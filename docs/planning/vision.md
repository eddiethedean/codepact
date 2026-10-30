# Product vision

## Mission

Bring useful Rust-inspired contracts to ordinary Python through two interoperable ways to express them. Developers should choose their level of explicitness per function, library or team, while receiving consistent analysis.

The Pythonic interface favors decorators, native types, context managers and inference. The explicit interface exposes ownership, borrowing, typed outcomes and state annotations. The analyzer assigns the same meaning to equivalent contracts.

## Audiences and problems

| Audience | Problem | First useful outcome |
| --- | --- | --- |
| Application teams | Resources remain in use after close or escape cleanup | Precise lifecycle diagnostics with source locations |
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

## Adoption path

Begin with advisory checks and curated resource models that can find useful issues without annotations. Add explicit resource declarations where library behavior is known. Enable strict boundary checks in selected modules, then expand after triaging diagnostics. Support baselines for existing findings and fail CI on newly introduced issues.

Examples and onboarding should lead with a real resource bug, its diagnostic, and a context-manager repair. Offer the explicit API beside the Pythonic example.

## Success measures

Before a public preview, recruit at least three representative pilot projects and record confirmed bugs, false positives, unsupported cases, first useful result time and triage effort. Proposed preview gate: at least 90% of reviewed definite-violation diagnostics are actionable on the pilot corpus; publish sample size and disagreements.

Track runtime import cost, cold analysis time, incremental latency, memory and interoperability parity. These are goals to measure, not current performance claims. Avoid adoption forecasts based on branding alone.

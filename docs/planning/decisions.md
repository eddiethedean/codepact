# Decisions, risks and source notes

## Established direction

| ID | Decision | Rationale |
| --- | --- | --- |
| D001 | TypePact product name, `typepact` Python package, `typepact.rust` explicit namespace | One project usable by Python and Rust-oriented developers |
| D002 | One canonical semantic engine | Equivalent declarations need consistent behavior |
| D003 | Pythonic and explicit APIs are first-class | Teams can mix styles across boundaries |
| D004 | Gradual, opt-in adoption | Existing Python applications must remain usable |
| D005 | Explicit runtime conversions | Nullable values, Option, exceptions and Result differ in behavior |
| D006 | Mixed-interface resource slice first | Demonstrates the project's defining interoperability early |

## Proposed decisions requiring evidence

| Question | Starting proposal | Evidence needed |
| --- | --- | --- |
| Metadata representation | Annotated aliases with compatible stubs | Interpreter and checker matrix |
| Analyzer implementation | Rust engine and standalone CLI | Parser feasibility and distribution spike |
| Ownership scope | Explicit moves and lexical borrows | Alias/escape counterexamples and diagnostics |
| Trait support | Protocol bridge before novel trait machinery | Concrete unsupported use cases |
| Python support | 3.11–3.14 initial target | Packaging and annotation behavior on each version |
| License and registries | Maintainer decision before release | License selection and registry ownership |

## Risk register

| Risk | Impact | Mitigation / trigger |
| --- | --- | --- |
| Unsound alias assumptions | False assurance | Conservative unknowns; block ownership expansion on counterexamples |
| Competing frontend semantics | Inconsistent diagnostics | Canonical snapshots and mandatory interop fixtures |
| Public typing aliases rejected by tools | Adoption friction | M0 prototype; fallback to explicit Annotated metadata |
| Excessive false positives | Users disable checks | Advisory default, narrow rules and measured pilot triage |
| State explosion | Slow or incomplete analysis | Bounded domains, budgets and visible incompleteness |
| Runtime wrappers change behavior | Integration failures | Explicit conversions and dedicated runtime tests |
| Packaging maintenance cost | Missing platform releases | Early wheel spike and a limited published matrix |
| Overbroad scope | Delayed useful release | Resource slice first; research kept outside preview gates |

## Technical references

These sources ground existing language behavior; TypePact's designs and support commitments remain proposals. Consult current specifications during implementation and pin tool versions in compatibility reports.

- [Python typing qualifiers and Annotated](https://typing.python.org/en/latest/spec/qualifiers.html): metadata interpretation and underlying typing behavior.
- [Distributing Python type information](https://typing.python.org/en/latest/spec/distributing.html): stubs, typing markers and compatibility constraints.
- [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html): background for the ownership model inspiring the explicit API.

The prior project discussion supplies product direction, not implementation evidence. Registry availability mentioned in that discussion must be checked again before release.

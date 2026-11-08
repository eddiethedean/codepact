# Decisions, risks and source notes

## Established direction

| ID | Decision | Rationale |
| --- | --- | --- |
| D001 | CodePact product name, `codepact` Python package, `codepact.rust` explicit namespace (the project originally shipped as TypePact with `typepact` names; see D019) | One project usable by Python and Rust-oriented developers |
| D002 | One canonical semantic engine | Equivalent declarations need consistent behavior |
| D003 | Pythonic and explicit APIs are first-class | Teams can mix styles across boundaries |
| D004 | Gradual, opt-in adoption | Existing Python applications must remain usable |
| D005 | Explicit runtime conversions | Nullable values, Option, exceptions and Result differ in behavior |
| D006 | Mixed-interface resource slice first | Demonstrates the project's defining interoperability early |
| D007 | MIT license retained; RustPython 0.4 and line scanning were the prototype frontend, superseded by D009 | Permissive source and dependency terms; parser preserves source-only inspection without embedding Python |
| D008 | Prototype analyzer target 3.11–3.13; helper runtime target 3.11–3.14; Ruff migration range requires fresh qualification | RustPython 0.4 has a fixed grammar; defer 3.14 analysis until parser compatibility is verified |
| D009 | Ruff parser/AST adapter and CodePact declaration/binding index replace line scanning | Correct source structure and captured identities; evaluate ruff_python_semantic reuse rather than assuming ty semantics |
| D010 | Salsa inputs and tracked queries are 0.1 foundations | Reusable sessions with explicit dependencies; incremental correctness tested from the start |
| D011 | Follow ty query architecture while owning contract semantics | Avoid requiring a general type checker or coupling public APIs to upstream internals |
| D012 | In-memory incrementality in 0.1; persistent caches/editor hardening in 0.8 | Establish correctness early and defer storage/editor complexity |
| D013 | Pin Ruff parser/AST 0.0.12 and Salsa 0.28.5; raise Rust MSRV to 1.96 | crates.io packages and published metadata report these versions; exact lockfile and archive build are required |
| D014 | Do not depend on `ruff_python_semantic` in 0.1; keep a CodePact-owned binding index for the supported declaration subset | Ruff labels the component API unstable and frequently breaking; its semantic model adds Ruff-specific scope and reference behavior that does not define CodePact's eager captured-contract semantics |

| D015 | Keep codepact pure Python and depend on codepact-cli==the same version; build codepact-cli with maturin bin wheels from 0.1 and retain the Cargo crate | One Python installation resolves annotations and analysis; codepact stays pure Python and pins the matching codepact-cli version. Publish eight CLI wheels, one universal helper wheel and two source archives, following the robin-sparkless release build matrix |

D009–D012 establish direction, not implementation completion. [AM01–AM06](analyzer-migration.md) define the evidence and rollout; D007's MIT choice remains in force.

**D016 — Optional Pyrefly composition prototype (superseded product naming).** The earlier [combined CLI proposal](codepact-pyrefly-cli.md) selected embedded Pyrefly plus the CodePact engine under `codepact-pyrefly-cli` / `codepact-pyrefly` names. D020 replaces those proposed names and delivery choices with the TypePact combined product while retaining PF01–PF05 and PF-V01–PF-V08. Previous publisher records do not qualify the new distributions or workflow; verify their current registry identity. D002/D011/D014/D015 continue to govern the core engine and existing packages.

**D017 — Structured async resource safety first.** Adopt the [concurrency plan](concurrency.md) on 2026-10-31: stage CS01–CS07 through 0.3–0.9, beginning with intratask cancellation/cleanup and reviewed asyncio.TaskGroup lifetime models. Reuse the canonical ownership/borrow/typestate/effect engine. Keep 0.2 synchronous. Process/thread handoffs remain conservative escape boundaries until reviewed models distinguish serialization, handles, start methods and worker failure. No Send/Sync, race-freedom, deadlock-freedom or general cross-process ownership guarantee is introduced; implementation and qualification remain pending.

**D018 — Maintainer-only release validation and approval.** At the maintainer's explicit direction, the accountable maintainer may implement, validate, review and approve every release alone, including semantic decisions, models and performance evidence. No independent reviewer, second-person sign-off, or independent-review waiver is required. Additional review is optional at the maintainer's discretion. This applies to all phase and patch releases; it supersedes any implication that owner, validator, reviewer and approver must be different people. Required tests, exact-candidate gate evidence and explicit maintainer release authorization remain in force. See the [maintainer authority policy](release-gates.md#maintainer-authority).

**D019 — Rename TypePact to CodePact for 0.3.0.** At the maintainer's direction, the product, GitHub repository, Python distribution/import, CLI distribution/executable, and Rust crate/import use the CodePact name. The new package identifiers are `codepact` and `codepact-cli`; the Rust package and CLI executable are `codepact`. Do not carry compatibility aliases for the published 0.1.0 or 0.2.0 TypePact names. Keep those release records factually unchanged, document the required source/dependency rename, and qualify the new 0.3.0 artifacts under the new identifiers before publication.

**D020 — TypePact combines CodePact with pinned embedded Pyrefly.** On 2026-11-07, the maintainer selected CodePact for behavioral contracts and requested planning for TypePact as the combined type-checking product, explicitly accepting the maintenance cost of Pyrefly's unstable Rust library interface. Use an exact upstream pin behind an adapter, one combined command/report, and the canonical CodePact engine and Python imports. The [TypePact plan](typepact.md) proposes `typepact` and `typepact-cli` packages with an independent version boundary above historical releases; exact versions, packaging, publisher identity and support matrices remain to be qualified. No legacy Python import aliases are restored, and D019's core rename remains in force. PF01 qualifies the embedding and complete-source route; a subprocess replacement requires an explicit design amendment. This is a planning decision, not implementation or release authorization.

## Proposed decisions requiring evidence

| Question | Starting proposal | Evidence needed |
| --- | --- | --- |
| Metadata representation | Annotated aliases with compatible stubs | Interpreter and checker matrix |
| Analyzer dependencies | Pinned Ruff parser/AST and Salsa, CodePact-owned adapter/index per D014 | AM01 API, license, MSRV, registry-package and target-version evidence |
| Ownership scope | Explicit moves and lexical borrows | Alias/escape counterexamples and diagnostics |
| Async task models | Bounded TaskGroup captures, completion and cancellation effects | CS01–CS04 model truth tables, owner/borrow and interrupted-cleanup counterexamples |
| Process handoffs | Conservative unknown/escape until separately qualified | Serialization, start-method, handle/shared-memory and worker-failure model proposal; no general safety claim |
| Trait support | Protocol bridge before novel trait machinery | Concrete unsupported use cases |
| Python support | 3.11–3.14 initial target | Packaging and annotation behavior on each version |
| Dependency notices and registries | Retain MIT; review upstream licenses and verify registry ownership | Dependency notice review and publisher prerequisites |

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
| Task outlives owner or cancellation interrupts cleanup | Use-after-close or false cleanup evidence | Track captures/completion separately; qualify cancellation postconditions under CS01–CS04 |
| Process transfer conflated with a move | Invalid handle or false isolation guarantee | Explicit unsupported boundary; require serialization/start-method/handle models before support |
| Overbroad scope | Delayed useful release | Resource slice first; research kept outside preview gates |

## Technical references

These sources ground existing language behavior; CodePact's designs and support commitments remain proposals. Consult current specifications during implementation and pin tool versions in compatibility reports.

- [Python typing qualifiers and Annotated](https://typing.python.org/en/latest/spec/qualifiers.html): metadata interpretation and underlying typing behavior.
- [Distributing Python type information](https://typing.python.org/en/latest/spec/distributing.html): stubs, typing markers and compatibility constraints.
- [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html): background for the ownership model inspiring the explicit API.

The prior project discussion supplies product direction, not implementation evidence. Registry availability mentioned in that discussion must be checked again before release.

## Additional analyzer risks

| Risk | Mitigation / release trigger |
| --- | --- |
| Internal upstream APIs and unpublished dependencies | Pin compatible revisions/versions; adapter boundary; extracted archive and registry delivery gate |
| Salsa dependency omissions | Explicit source/settings/discovery inputs; fresh-session parity and query event checks |
| Name rebinding corrupts earlier aliases | Declaration identities distinct from current bindings; captured immutable contract views |
| Memoized unknowns become stale | Track missing-module existence and model/configuration inputs; invalidate coverage as well as facts |
| Cyclic queries or unbounded graph work | Explicit cycle policy and budgets; no reliance on caching for semantic convergence |
| External IDs reflect allocation order | Stable logical public identities and deterministic ordering; no serialized Salsa IDs |

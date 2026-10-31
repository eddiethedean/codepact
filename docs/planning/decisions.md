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
| D007 | MIT license retained; RustPython 0.4 and line scanning were the prototype frontend, superseded by D009 | Permissive source and dependency terms; parser preserves source-only inspection without embedding Python |
| D008 | Prototype analyzer target 3.11–3.13; helper runtime target 3.11–3.14; Ruff migration range requires fresh qualification | RustPython 0.4 has a fixed grammar; defer 3.14 analysis until parser compatibility is verified |
| D009 | Ruff parser/AST adapter and TypePact declaration/binding index replace line scanning | Correct source structure and captured identities; evaluate ruff_python_semantic reuse rather than assuming ty semantics |
| D010 | Salsa inputs and tracked queries are 0.1 foundations | Reusable sessions with explicit dependencies; incremental correctness tested from the start |
| D011 | Follow ty query architecture while owning contract semantics | Avoid requiring a general type checker or coupling public APIs to upstream internals |
| D012 | In-memory incrementality in 0.1; persistent caches/editor hardening in 0.8 | Establish correctness early and defer storage/editor complexity |
| D013 | Pin Ruff parser/AST 0.0.12 and Salsa 0.28.5; raise Rust MSRV to 1.96 | crates.io packages and published metadata report these versions; exact lockfile and archive build are required |
| D014 | Do not depend on `ruff_python_semantic` in 0.1; keep a TypePact-owned binding index for the supported declaration subset | Ruff labels the component API unstable and frequently breaking; its semantic model adds Ruff-specific scope and reference behavior that does not define TypePact's eager captured-contract semantics |

| D015 | Keep typepact pure Python and depend on typepact-cli==the same version; build typepact-cli with maturin bin wheels from 0.1 and retain the Cargo crate | One Python installation resolves annotations and analysis; typepact stays pure Python and pins the matching typepact-cli version. Publish eight CLI wheels, one universal helper wheel and two source archives, following the robin-sparkless release build matrix |

D009–D012 establish direction, not implementation completion. [AM01–AM06](analyzer-migration.md) define the evidence and rollout; D007's MIT choice remains in force.

**D016 — Optional Pyrefly composition prototype.** Plan a separate `typepact-pyrefly-cli` PyPI distribution and `typepact-pyrefly` command combining ordinary type checking from embedded Pyrefly with the existing TypePact resource engine. The maintainer selected Pyrefly for this experiment and supplied a screenshot confirming its trusted publisher configuration on 2026-10-30. Exact dependency/API selection, source packaging and supported targets remain evidence-dependent under [PF01–PF05 and PF-V01–PF-V08](typepact-pyrefly-cli.md). Use independent prerelease versions and the configured `release-pyrefly.yml`/`pypi` publisher identity; workflow implementation and publication remain pending. This adds an optional host; D002/D011/D014/D015 continue to govern the core engine and existing packages.

**D017 — Structured async resource safety first.** Adopt the [concurrency plan](concurrency.md) on 2026-10-31: stage CS01–CS07 through 0.3–0.9, beginning with intratask cancellation/cleanup and reviewed asyncio.TaskGroup lifetime models. Reuse the canonical ownership/borrow/typestate/effect engine. Keep 0.2 synchronous. Process/thread handoffs remain conservative escape boundaries until reviewed models distinguish serialization, handles, start methods and worker failure. No Send/Sync, race-freedom, deadlock-freedom or general cross-process ownership guarantee is introduced; implementation and qualification remain pending.

## Proposed decisions requiring evidence

| Question | Starting proposal | Evidence needed |
| --- | --- | --- |
| Metadata representation | Annotated aliases with compatible stubs | Interpreter and checker matrix |
| Analyzer dependencies | Pinned Ruff parser/AST and Salsa, TypePact-owned adapter/index per D014 | AM01 API, license, MSRV, registry-package and target-version evidence |
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

These sources ground existing language behavior; TypePact's designs and support commitments remain proposals. Consult current specifications during implementation and pin tool versions in compatibility reports.

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

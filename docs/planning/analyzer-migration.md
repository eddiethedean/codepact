# Analyzer foundation migration

Status: accepted architecture direction; analyzer implementation complete, qualification pending. The frontend produces structured Ruff AST declarations and expression trees with source ranges and target-version parsing. The TypePact semantic pass resolves ordered bindings, captured local views, and direct supplied-module class/alias exports. Salsa tracks per-file parsing, per-module semantic inspection, and semantic-only exports. Cancellation recovery, revision-tagged session results and bounded work/cache budgets are implemented. The V01–V24, F01–F08 and candidate-specific release evidence remains open; this plan does not claim a qualified release.

## Scope and upstream boundary

Use `ruff_python_parser`, `ruff_python_ast` and source ranges through a TypePact-owned adapter. Evaluate `ruff_python_semantic` for reusable scope and binding primitives, but do not assume it builds a complete semantic index from an AST or supplies ty's query engine. Record an integration spike before committing to that dependency. TypePact owns its declaration index, contract interpretation and supported resolution semantics.

Follow ty's separation of parsing, semantic indexing and inference. Study `ty_python_core`, `ty_python_semantic` and `ruff_db` as architectural references, without requiring their full type system. Salsa supplies tracked queries and dependency invalidation; TypePact supplies resolution, cycle policies, semantic rules and budgets. This project is not implementing a general Python type checker in 0.1.

Pin compatible Ruff component versions or an exact reviewed revision and a compatible Salsa version in the dependency decision and Cargo.lock. Never track upstream main as a reproducible dependency. Before selecting versions, verify license/notice obligations, Rust MSRV, supported Python grammar, AST/range APIs, API stability and crates.io packaging. A development-only Git dependency is not evidence that the published crate can build: crates.io delivery needs registry dependencies or a reviewed vendoring/publication approach. Retain Cargo delivery alongside the typepact-cli wheels carrying the compiled CLI under D015. Qualify Rust source completeness and the shared engine/CLI version through both installation paths.

## Identity and binding semantics

Keep these identities separate:

- FileId and ModuleId: logical workspace identities, independent of machine-specific absolute paths.
- ScopeId and DeclarationId: lexical scopes and individual definitions, including repeated definitions with the same spelling.
- BindingId: a name binding at a program point, referring to a declaration, an imported symbol or an explicit unknown.
- ContractView: captured base declaration and lifecycle/access metadata with source provenance.
- ResourceInstanceId: a flow-analysis abstraction introduced in 0.2; never substitute a class declaration ID for an object instance.

An ordinary eager alias assignment captures the resolved declaration and metadata before replacing the left-hand binding. For example, `View = Borrowed[C]; C = int` leaves View referring to the original C. Later declarations named C have distinct identities. Imports, assignments, function/class definitions and supported deletion forms update the current binding without deleting declarations referenced by earlier aliases. Evaluate supported annotations at their defined semantic point; do not clear the function name before resolving an eager annotation that references its prior binding.

Alias chains flatten to captured contract views where possible. Distinguish actual unresolved cycles from repeated name reassignment that is valid under Python's eager evaluation. Bound iterative traversal and preserve unknowns. Future annotations, string annotations, PEP 695 lazy aliases, conditional bindings, nested scopes and dynamic expressions need explicit semantics or an incomplete-coverage boundary; parsing them alone does not establish support.

Internal Salsa IDs are not persistent or public identities. Preserve current schema-v0 output for unaffected inputs. If repeated declarations require an externally visible identity disambiguator, specify a deterministic logical module/declaration key and review the schema change before updating snapshots. Never serialize database allocation order or absolute temporary paths as identity.

## Query graph

| Query / input | Depends on | Result and boundary |
| --- | --- | --- |
| SourceFile input | Host-supplied logical path and text | Immutable text for a database revision |
| AnalysisSettings input | Target Python, roots, rules, model versions | Explicit configuration dependencies; 0.1 currently tracks target Python as a query argument |
| parsed_module(file) | Source text, parser options | Ruff AST, syntax errors and source map |
| declaration_index(file) | Parsed module | Scopes, ordered binding events, declaration identities |
| module_exports(module) | Index and supported imported exports | Canonical exported contract views or explicit unresolved results |
| resolve_contract(declaration) | Relevant binding events and exported views | Normalized contract, provenance and declaration errors |
| inspect_file(file) | Relevant declarations and resolved contracts | Facts, diagnostics and coverage |
| cfg / function_summary / check_function | Definition body, callee summaries, models, rules | Added from 0.2; bounded flow facts and diagnostics |

Track filesystem observations as inputs outside queries: file contents, module discovery results, file existence and search-path precedence. A missing module that is later created must invalidate its users. Queries must not perform hidden filesystem reads, imports, environment reads or clock-dependent work. Target-version and model changes are inputs, not implicit globals.

Keep exported semantic summaries separate from diagnostic locations and whole ASTs so an implementation-only edit need not invalidate importers. Source positions must still refresh correctly when lines move. Cross-module queries consume summaries instead of routinely reaching into another file's AST. Prove the boundary with query execution events, not timings alone.

## Cycles, revisions and lifetime

0.1 supports direct imports among explicitly supplied modules and reports unsupported re-exports or import cycles with bounded incomplete coverage. It does not introduce recursive flow inference. From 0.2, recursive call components use an explicitly designed bounded fixed point and widening; Salsa memoization alone is not convergence or soundness. Budget exhaustion returns a structured incomplete result, never a panic or a successful proof.

Provide a reusable analysis session that owns the Salsa database and accepts source/settings updates. Retain the existing one-shot inspect entry point as a convenience wrapper. A short-lived CLI database gives consistent semantics, while reuse benefits library callers and later editor sessions. In-memory reuse starts in 0.1; no disk cache or language server is required there. The 0.1 API exposes the revision beside the result and adapts Salsa cancellation to an error outcome.

Every result belongs to a revision. Cancellation and resource limits must leave the session usable and prevent stale results from being published. Document Salsa's selected-version cancellation contract. Limit input bytes, import traversal, query work, graph size and retained memory; enforce limits before expensive traversal. Database-local identities must not escape across session resets.

The [0.1 implementation repair plan](phases/0.1/remediation.md) maps these architectural requirements to current files, R01–R08 work items, fixture changes and acceptance evidence.

## Ordered migration work

| ID | Deliverable | Acceptance evidence |
| --- | --- | --- |
| AM01 | Pin dependency set, frontend spike, Rust/MSRV and packaging decision | Parse/range/version fixtures; archived crate builds; semantic-crate integration assessment |
| AM02 | Introduce adapter and replace string/line scans with AST traversal | Existing canonical examples plus strings/comments, multiline headers, defaults, decorators and return annotations |
| AM03 | Add scopes, declaration identities, binding events and captured alias views | Rebinding through assignment/import/function/class, old aliases, repeated definitions and invalid/unknown forms |
| AM04 | Add Salsa inputs, tracked queries and reusable session | Cold/warm parity, no-op reuse, isolated-file edit, changed import, missing-module creation and settings invalidation |
| AM05 | Integrate CLI, diagnostics, limits and distribution | Stable applicable schema snapshots, deterministic ordering, cancellation/budget recovery and clean installs |
| AM06 | Qualify 0.1 and retire legacy lowering | Required fixture matrix and candidate evidence; no remaining semantic fallback to line scanning |

AM01 precedes dependency changes; AM02 and AM03 define the facts consumed by AM04. AM05 and AM06 require all earlier acceptance evidence. Keep implementation changes reviewable, but do not call migration complete while the legacy scanner remains the semantic authority. Unsupported syntax gets a visible boundary rather than a fallback parser with different semantics.

## Completion and rollout

All AM work belongs to 0.1 WP01–WP07 and gates F01–F08; the expanded validation matrix is required. The implementation has replaced the previous prototype path, and local query, cancellation and budget regressions now cover the updated architecture. Candidate qualification is still open until the full validation matrix, compatibility matrix and distribution evidence are recorded. Preserve the Python helper API and inspection-only assurance unless an explicit semantic decision changes them.

Later phases extend this query graph with flow, effects, models and editor services. Persistent caches are a separate 0.8 deliverable with format/version compatibility and corruption recovery. No later phase may replace unknown facts with certainty merely because a query was cached.

## Architecture references

- [Ruff parser](https://github.com/astral-sh/ruff/tree/main/crates/ruff_python_parser) and [AST](https://github.com/astral-sh/ruff/tree/main/crates/ruff_python_ast): frontend components.
- [Ruff semantic API](https://github.com/astral-sh/ruff/blob/main/crates/ruff_python_semantic/src/lib.rs): scope and binding primitives; assess integration requirements.
- [ty core](https://github.com/astral-sh/ruff/tree/main/crates/ty_python_core) and [inference queries](https://github.com/astral-sh/ruff/blob/main/crates/ty_python_semantic/src/types/infer.rs): indexing and query-boundary references.
- [Salsa overview](https://salsa-rs.github.io/salsa/overview.html): inputs, tracked computation and incremental reuse.

These moving upstream links explain the design. AM01 must record immutable revisions and verified versions for implementation evidence.

The current 0.1 integration assessment is recorded in [D014](decisions.md): `ruff_python_semantic` is not adopted because its published component API is explicitly unstable and its Ruff-specific semantic behavior does not replace TypePact's eager binding and contract-view rules. The frontend uses Ruff parser/AST crates with a TypePact-owned narrow binding index.

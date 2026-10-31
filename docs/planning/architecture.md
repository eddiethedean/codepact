# Architecture

Status: the 0.1.0 inspector implementation and release qualification are complete; the evidence is in the [release record](../releases/0.1.0-evidence.md). The current source extends the engine with lifecycle checking for 0.2.0, whose release gates remain pending. The inspector lowers structured Ruff AST declarations through a TypePact-owned binding and contract-view resolver. Salsa tracks per-file frontend results, per-module semantic analysis and semantic-only export summaries. Session cancellation, revisions and work/cache limits are implemented.

## Dependency direction

~~~mermaid
flowchart TD
    H[Host file and settings inputs] --> DB[Salsa database]
    DB --> P[Ruff parser and AST adapter]
    P --> I[Scopes and declaration identities]
    I --> R[Bindings and imported contract views]
    R --> A[Pythonic contract lowering]
    R --> B[Explicit contract lowering]
    A --> C[Canonical contracts]
    B --> C
    C --> F[CFGs and function summaries from 0.2]
    F --> E[Shared flow engine]
    C --> D[Diagnostics and coverage]
    E --> D
    D --> CLI[CLI and library session]
    D --> LSP[Editor service from 0.8]
~~~

Salsa tracks the inputs read by each query in this pipeline. Frontends translate declarations; one shared engine defines lifecycle and ownership semantics. AST nodes, binding events, declaration identities and runtime resource abstractions remain separate concepts.

## Component ownership

| Component | Responsibility |
| --- | --- |
| python/typepact/ | Lightweight runtime declarations and typing metadata |
| crates/typepact/ frontend adapter | Pinned Ruff parser/AST APIs, ranges, target-version options and syntax diagnostics |
| Semantic index | TypePact declaration identities, scope/binding rules and captured aliases; evaluate Ruff semantic primitives through AM01 |
| Salsa database | Explicit file/settings inputs, tracked parsing/index/resolution/inspection queries and revision management |
| Contract and flow engine | Canonical metadata, provenance and diagnostics; CFGs/summaries from 0.2 |
| CLI host | File input, configuration, invocation errors and deterministic presentation; current binary remains in the same crate |
| Future editor host | Reuse the database and semantic APIs with revision-aware updates and cancellation |
| tests and benchmarks | Semantic counterexamples, incremental parity and execution-count evidence, packaging and performance |

Use internal module boundaries first; additional crates require a demonstrated build or API benefit. Keep Ruff and Salsa internals behind TypePact-owned interfaces. The Python helper package never initializes the analyzer. Delivery uses two Python distributions: typepact is a pure Python helper wheel built by Hatchling, with an exact dependency on typepact-cli at the same version. Maturin bin bindings package the compiled Rust inspector as the typepact command in typepact-cli. CLI wheels have py3-none platform tags because the executable does not depend on the Python ABI. Importing helpers does not launch the CLI. Separate source archives provide helper code and the CLI Rust workspace/manifests/Cargo.lock; Cargo remains an additional delivery route. D015 and the release matrix govern all phases.

## Incremental computation

In-memory Salsa queries are required in 0.1. Expose a reusable analysis session and retain the one-shot inspection wrapper. Module exports are small semantic summaries; importers should not depend on irrelevant foreign AST details. Source text, target Python, resolver search paths, configuration and model versions are tracked inputs. Disk reads and module discovery happen in the host and enter the database explicitly.

Correctness requires identical semantic facts, diagnostics and coverage for fresh and updated sessions. Test unchanged query reuse and dependency isolation separately from output parity. Preserve deterministic output despite internal IDs or execution order. Queries return structured unknown/error/budget outcomes and never execute Python.

Salsa does not supply Python resolution or solve semantic cycles automatically. Use the bounded cycle policy in the migration plan; introduce recursive summary fixed points only with flow analysis. Persistent storage starts in 0.8 and must reject incompatible/corrupt caches. Do not persist Salsa allocation IDs as external identifiers.

## Operational constraints

Bound source bytes, graph traversal, declarations, work and retained memory. Attribute results to revisions, support cancellation without publishing stale diagnostics, and keep sessions usable after incomplete work. Analyze source/stubs as data; never evaluate annotations, execute decorators, import target modules or run project hooks. Honor roots/exclusions and detect path cycles when discovery is added in 0.2.

## Dependency qualification

AM01 records exact Ruff/Salsa versions or reviewed revisions, licenses, notices, Rust MSRV and build/archive evidence. Ruff semantic and ty internal crates are not assumed to be stable public integration APIs. Verify Cargo registry delivery before adopting Git-only dependencies. Target Python support follows parser and semantic fixtures, not helper-runtime support or a dependency version alone.

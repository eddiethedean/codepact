# Architecture

Status: the 0.1.0 inspector and 0.2.0 lifecycle checker are implemented and released; their qualification is recorded in the [0.1.0](../releases/0.1.0-evidence.md) and [0.2.0](../releases/0.2.0-evidence.md) release records. The inspector lowers structured Ruff AST declarations through a CodePact-owned binding and contract-view resolver. Salsa tracks per-file frontend results, per-module semantic analysis and semantic-only export summaries. Session cancellation, revisions and work/cache limits are implemented.

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
| python/codepact/ | Lightweight runtime declarations and typing metadata |
| crates/codepact/ frontend adapter | Pinned Ruff parser/AST APIs, ranges, target-version options and syntax diagnostics |
| Semantic index | CodePact declaration identities, scope/binding rules and captured aliases; evaluate Ruff semantic primitives through AM01 |
| Salsa database | Explicit file/settings inputs, tracked parsing/index/resolution/inspection queries and revision management |
| Contract and flow engine | Canonical metadata, provenance and diagnostics; CFGs/summaries from 0.2 |
| CLI host | File input, configuration, invocation errors and deterministic presentation; current binary remains in the same crate |
| Future editor host | Reuse the database and semantic APIs with revision-aware updates and cancellation |
| tests and benchmarks | Semantic counterexamples, incremental parity and execution-count evidence, packaging and performance |

Use internal module boundaries first; additional crates require a demonstrated build or API benefit. Keep Ruff and Salsa internals behind CodePact-owned interfaces. The Python helper package never initializes the analyzer. Delivery uses two Python distributions: codepact is a pure Python helper wheel built by Hatchling, with an exact dependency on codepact-cli at the same version. Maturin bin bindings package the compiled Rust inspector as the codepact command in codepact-cli. CLI wheels have py3-none platform tags because the executable does not depend on the Python ABI. Importing helpers does not launch the CLI. Separate source archives provide helper code and the CLI Rust workspace/manifests/Cargo.lock; Cargo remains an additional delivery route. D015 and the release matrix govern all phases.

## Planned TypePact host

[TypePact](typepact.md) is a separate planned host combining the CodePact library with pinned embedded Pyrefly behind an adapter. It coordinates configuration, source revisions and per-engine diagnostics; it does not add ordinary type semantics to CodePact or duplicate contract rules. Keep ASTs, resolvers and incremental databases private to each engine. The maintainer accepts upstream library API changes between pins, qualified through PF01 and per-upgrade parity/packaging evidence. The core CLI and helper distribution remain independently usable.

## Incremental computation

In-memory Salsa queries are required in 0.1. Expose a reusable analysis session and retain the one-shot inspection wrapper. Module exports are small semantic summaries; importers should not depend on irrelevant foreign AST details. Source text, target Python, resolver search paths, configuration and model versions are tracked inputs. Disk reads and module discovery happen in the host and enter the database explicitly.

Correctness requires identical semantic facts, diagnostics and coverage for fresh and updated sessions. Test unchanged query reuse and dependency isolation separately from output parity. Preserve deterministic output despite internal IDs or execution order. Queries return structured unknown/error/budget outcomes and never execute Python.

Salsa does not supply Python resolution or solve semantic cycles automatically. Use the bounded cycle policy in the migration plan; introduce recursive summary fixed points only with flow analysis. Persistent storage starts in 0.8 and must reject incompatible/corrupt caches. Do not persist Salsa allocation IDs as external identifiers.

## Operational constraints

Bound source bytes, graph traversal, declarations, work and retained memory. Attribute results to revisions, support cancellation without publishing stale diagnostics, and keep sessions usable after incomplete work. Analyze source/stubs as data; never evaluate annotations, execute decorators, import target modules or run project hooks. Honor roots/exclusions and detect path cycles when discovery is added in 0.2.

## Dependency qualification

AM01 records exact Ruff/Salsa versions or reviewed revisions, licenses, notices, Rust MSRV and build/archive evidence. Ruff semantic and ty internal crates are not assumed to be stable public integration APIs. Verify Cargo registry delivery before adopting Git-only dependencies. Target Python support follows parser and semantic fixtures, not helper-runtime support or a dependency version alone.

## Concurrent resource analysis extensions

The [concurrency plan](concurrency.md) extends CFGs and summaries with coroutine/task creation, captures, scheduling, suspension, completion and cancellation-aware cleanup. Keep task identities, handles, resource instances and borrow regions distinct in the shared engine; neither frontend gains independent concurrency semantics. Analyzer request cancellation and target-program cancellation are separate mechanisms.

Track concurrency-model versions, platform/start-method assumptions and policy as explicit inputs. Capture/join/cleanup summary edits must refresh dependent facts, coverage and related locations, with fresh/reused parity. Bound task/alias graphs and retain unknown outcomes on exhausted work. Multiprocessing/thread boundaries remain conservative without models; no target code or scheduler is executed to infer effects.

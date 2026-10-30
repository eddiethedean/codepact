# Architecture

## Dependency direction

~~~mermaid
flowchart TD
    S[Python source and stubs] --> P[Parser and name resolution]
    P --> A[Pythonic contract lowering]
    P --> B[Explicit contract lowering]
    A --> I[Canonical semantic IR]
    B --> I
    I --> C[CFG and function summaries]
    C --> E[Shared flow engine]
    E --> D[Diagnostics and coverage]
    D --> CLI[CLI]
    D --> LSP[Future language server]
~~~

Frontends translate declarations; they do not independently enforce ownership or lifecycle rules. The engine operates on canonical contracts and source provenance.

## Proposed repository layout

| Path | Responsibility |
| --- | --- |
| python/typepact/ | Lightweight public API, metadata and runtime variants |
| crates/typepact/ | Canonical model and reusable analysis engine |
| crates/typepact-cli/ | Filesystem discovery, configuration and command output |
| crates/typepact-python/ | Optional native bindings if justified |
| tests/fixtures/ | Input programs, summaries and expected diagnostics |
| tests/interop/ | Equivalent and mixed-interface cases |
| benchmarks/ | Versioned corpus definitions and measurement harness |
| docs/ | Specifications, tutorials, reference and decisions |

Keep runtime imports independent of native bindings. A standalone Rust CLI packaged with the Python distribution is the initial packaging candidate; prototype wheel delivery before selecting it over extension bindings. Avoid a mandatory Python-to-Rust call for every analyzed node.

## Components and contracts

The parser must preserve source spans, recover from incomplete files, support target-version syntax, and have a compatible license. Evaluate maintained parser libraries before choosing one; record upstream stability and update costs.

Name resolution creates module and symbol identities, handles import cycles, and resolves stubs without importing code. Type resolution needs enough ordinary typing information to locate contracts and calls; unsupported types become explicit unknowns.

Lowering creates versioned semantic IR. Flow analysis consumes CFG edges for normal, exceptional, return and suspension paths. Diagnostics contain stable codes, related locations, certainty, assumptions and suggestions. Presentation layers cannot change semantics.

## Incrementality and caching

Cache parsed modules by content and parser version. Key semantic results by dependency summaries, target Python version, configuration, model database version and analyzer version. Invalidate transitive dependents when public summaries change. A cache miss changes performance only, never diagnostics.

Recursive call components require bounded fixed points and widening. Exceeding a budget yields an incomplete-analysis diagnostic with location and reason. Persisted caches are disposable and must reject incompatible schemas.

## Operational constraints

Analyze source and stubs as data. Never evaluate annotation strings, import target modules, execute decorators or run project hooks. Limit source size, recursion, state explosion and cache growth. Honor exclusions and project roots; detect symlink cycles.

CLI and future editor service share the same engine and configuration resolver. Parallel execution must retain deterministic diagnostic order. Optional telemetry requires explicit opt-in and must exclude source text by default.

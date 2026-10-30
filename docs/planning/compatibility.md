# Compatibility plan

## Python typing

Use standard types, Protocol, unions, generics and Annotated metadata where possible. The Python typing specification permits tools to interpret Annotated metadata while consumers that do not understand it use the underlying type. TypePact-specific enforcement still requires TypePact.

Prototype public aliases against pinned mypy and Pyright versions before freezing syntax. Include imported aliases, re-exports, nested generics, forward references, overloads and annotation evaluation behavior. Do not promise support because an annotation happens to import successfully.

Ship inline typing with a py.typed marker or valid stubs as required by the distribution design. Stub syntax must fit supported interpreter/checker requirements. Preserve runtime signatures and introspection behavior.

## Proposed support matrix

| Surface | Initial target | Gate |
| --- | --- | --- |
| Python runtime helpers | CPython 3.11 through 3.14 | CI on each minor; reassess at release |
| Analyzed source | Current implementation target: 3.11–3.13 using Ruff target-version parsing | Syntax rejection/acceptance, ranges and semantic fixtures per version; 3.14 remains unclaimed until qualification |
| PyPy and free-threaded Python | Experimental | Separate runtime and packaging evidence |
| mypy / Pyright | mypy 1.19.1 / Pyright 1.1.411 | Compatibility corpus passes |
| Platforms | Linux, macOS and Windows | Install and CLI smoke checks on shipped wheels |
| Rust toolchain | Analyzer MSRV 1.96 selected by Ruff 0.0.12; requalify on dependency updates | Build on pinned minimum and current stable |

This is a proposed support range, not a current release matrix. Interpreter support for the helper package, syntax support in the analyzer, and binary availability must be published separately.

## Ecosystem integration

Pydantic and FastAPI compatibility begins with ordinary annotation handling, introspection and schema generation fixtures. Metadata must not silently become runtime validation. Result and Option are runtime wrappers and need explicit serialization adapters; no universal framework integration is implied.

Dataclasses, inheritance, decorators and context managers require fixtures before support claims. Third-party lifecycle models must record upstream package versions and be invalidated or reviewed when behavior changes.

Use public typing and stub formats. Avoid relying on private mypy or Pyright internals as the shared engine's foundation. Optional adapters may be developed separately if they reduce friction without creating conflicting semantics.

## Configuration proposal

~~~toml
[tool.typepact]
python-version = "3.11"
mode = "advisory"
include = ["src"]
exclude = ["build", ".venv"]
~~~

Define deterministic precedence: CLI overrides project configuration, which overrides documented defaults. Unknown keys and invalid versions are errors. Resolve paths relative to the configuration file. Future per-module strictness must have an explicit specificity rule.

## Migration

Adopt lifecycle checks first, baseline existing findings, annotate public boundaries, then increase strictness by module. Preserve ordinary checker usage throughout. Publish changed diagnostics, default severities and migration examples with every release that affects CI behavior.

## Analyzer dependency qualification

The [migration plan](analyzer-migration.md) requires a jointly compatible Ruff parser/AST component set and Salsa version, pinned with Cargo.lock. Evaluate Ruff semantic reuse against actual integration APIs. Record Rust edition/MSRV changes, license notices, target-version parsing and registry availability before adopting dependencies. Update manifests, CI and public source-build instructions together when the qualified toolchain changes. Keep parser acceptance separate from supported contract semantics, especially deferred annotations and lazy type aliases.

## Python helper and CLI distributions

The typepact distribution remains pure Python and pins its typepact-cli dependency, which supplies native platform wheels containing the Rust CLI (D015). Installation supports Python 3.11+; qualify helper behavior on 3.11–3.14 independently of the analyzer grammar target. See the eight-target release matrix and Rust prerequisites for source installs in [releases](releases.md).

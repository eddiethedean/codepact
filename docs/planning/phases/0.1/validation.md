# 0.1 validation and evidence matrix

Status: planned requirements. The commands, fixtures and CI suites must be created during implementation; no passing results exist yet.

## Required fixture matrix

| Fixture ID | Input / exercise | Expected result | Gates |
| --- | --- | --- | --- |
| V01 | Pythonic resource versus explicit Resource view | Equal lifecycle/base facts under fixture symbol mapping | F01 |
| V02 | Borrowed shorthand versus verbose Annotated metadata | Equal shared-access facts, underlying checker type retained | F01, F02 |
| V03 | Owned shorthand versus its verbose form | Equal owned-intent facts; no uniqueness claim | F01, F02 |
| V04 | Resource class plus Borrowed parameter | Independent dimensions compose; both declaration origins retained | F01, F06 |
| V05 | Incompatible access/lifecycle declarations | Declaration error with related spans; exit 1 | F01, F06 |
| V06 | Aliased imports, re-export within supported subset, local shadowing | Correct symbol identities; lookalike names not recognized | F03 |
| V07 | Valid and invalid syntax for each target version | Correct spans, error handling and incomplete coverage | F03 |
| V08 | Annotation expression/import/decorator with marker-file side effects | No marker created; unsupported expressions never executed | F03 |
| V09 | Missing import, star import and computed contract arguments | Explicit unresolved/unsupported coverage; no fabricated facts | F03, F06 |
| V10 | Oversized input, import cycle and traversal limit | Bounded termination with documented outcome | F03, F06 |
| V11 | Repeated API and CLI inspections | Equal semantic output and byte-stable CLI JSON | F06 |
| V12 | Decorated class and annotated ordinary object | Identity preserved, no proxy allocation or eager native loading | F02 |
| V13 | Checker reveals, valid assignments and deliberately invalid assignments | Base types preserved; invalid ordinary assignments rejected | F02 |
| V14 | Installed wheel, installed sdist and installed crate | Imports, version/help and inspection work outside source checkout | F04 |
| V15 | Pristine checkout, no preexisting build artifacts | Build and metadata checks pass; source archive includes required files | F04 |
| V16 | Supported-scope malformed declarations | Diagnostic result instead of panic or analyzer crash | F03, F06 |

V01 compares equivalent contract views. It must not test Resource metadata against Owned or Borrowed alone as if those dimensions were interchangeable.

## Compatibility record

Pin exact mypy, Pyright, build tooling and Rust toolchain versions in the test setup and evidence. Run the helper/checker fixtures across each claimed Python minor. Test the declared Rust MSRV and selected stable toolchain.

For the initial distribution proposal, validate the helper wheel on Linux, macOS and Windows if all three are advertised. Validate the Cargo-installed CLI on every separately advertised inspector platform. Do not infer native CLI support from a pure Python import result.

Record each combination as Pass, Fail or Unsupported. A narrower final matrix requires a decision and updates to compatibility documentation.

## CI acceptance

WP06 creates a reusable quality workflow or equivalent dependency graph, invoked for pull requests, main-branch changes and release tags. At minimum it must cover:

1. Rust formatting, lint/build/tests on selected toolchains.
2. Python import/identity tests and pinned typing fixtures.
3. Canonical lowering and CLI/API fixtures V01–V16.
4. Build/archive checks, clean artifact installation and Cargo packaging verification.

Use the same candidate revision and committed lockfiles. Separate packaging checks from source-tree imports so missing packaged files are detectable. Both publish jobs must wait for successful quality and artifact checks on that tag. A successful unrelated PR run is not release evidence.

The current release workflow covers only part of this list. This plan requires its dependency graph to be extended during implementation, without publishing a test version merely to exercise the gates.

## F05 prerequisite evidence

Record the selected license and dependency notice review, registry namespace control, and publisher identity settings. Match PyPI's repository/workflow/environment tuple to eddiethedean/typepact, release.yml and pypi. Confirm CARGO_REGISTRY_TOKEN is configured with the needed publishing authority without exposing its value.

Before first publication, inspect configuration and use non-publishing checks where available. Actual successful upload verification belongs to G8 after an authorized release tag; do not make a pre-tag gate depend on an upload that has not happened.

## Performance baseline

Measure representative small fixtures and a versioned synthetic declaration corpus. Record source bytes/declaration count, hardware, OS, tools, cold runtime import time, inspection elapsed time and peak memory. Repeat runs and report median and range. Absolute performance targets are set after measurements; verify budget-exhaustion behavior before release.

## Evidence layout

Create docs/releases/0.1.0-evidence.md from the [shared template](../../release-evidence-template.md) when a real candidate exists. Attach CI links, canonical snapshots, parser/alias decisions, exact support matrix, artifact hashes and clean-install logs.

Keep a row per F01–F08 plus G1–G8. Include candidate revision, reviewer and status. G8 remains pending until both published artifacts are downloaded and verified. Do not commit fabricated passing logs or mark decisions accepted on the strength of this plan.

## 0.2 handoff checklist

- [ ] Contract model and schema-v0 sample documented.
- [ ] Declared/unverified provenance retained through API and CLI output.
- [ ] All supported symbol-resolution rules covered by fixtures.
- [ ] Unresolved boundaries and parser limits documented.
- [ ] Installation walkthrough and baseline measurements reproducible.
- [ ] Work items for CFGs, instance identities, alias flow, method summaries and use-after-close created.

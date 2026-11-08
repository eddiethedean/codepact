# codepact-pyrefly-cli prototype

Status: **Planned prototype**, requested by the maintainer. No implementation or qualification gate is marked passed. Owner: maintainer to assign before implementation. This optional distribution combines upstream Pyrefly type checking with the existing CodePact contract engine. It may be prototyped alongside the lifecycle work; it is not a prerequisite for the main 0.x release sequence.

## Product and package contract

| Surface | Selected direction |
| --- | --- |
| PyPI distribution | `codepact-pyrefly-cli` |
| Executable | `codepact-pyrefly` (`codepact-pyrefly.exe` on Windows) |
| First candidate | Proposed Python prerelease `0.1.0a1`, independently versioned from the core packages |
| Type checking | Pinned upstream Pyrefly library, behind a CodePact-owned adapter |
| Resource checking | Reuse the `codepact` Rust library from the candidate revision |
| Packaging | Maturin binary wheels plus a complete buildable source archive |
| Publishing | Planned `.github/workflows/release-pyrefly.yml`, GitHub environment `pypi`, PyPI OIDC |

One invocation runs both checkers and reports ordinary Python typing errors and CodePact findings together. This is a composition prototype: Pyrefly owns ordinary Python typing semantics; CodePact retains its canonical resource contracts, certainty, provenance and coverage. Do not build a second general type system or infer new resource-safety guarantees from a successful Pyrefly run.

The package supplies its own executable and must coexist with installed `codepact`, `codepact-cli` and `pyrefly` distributions without replacing their commands. It embeds both Rust engines and does not require their separate CLI distributions at runtime. Python helpers can be installed separately as `codepact`; their existing exact `codepact-cli` dependency under D015 remains in force. Installing the combined checker does not by itself supply the Python annotation helpers. Check package metadata for dependency cycles and document this distinction in onboarding.

## Feasibility boundary

Pyrefly's [Cargo manifest](https://github.com/facebook/pyrefly/blob/main/pyrefly/Cargo.toml) declares a library and repository-local supporting crates. Its [library source](https://github.com/facebook/pyrefly/blob/main/pyrefly/lib/lib.rs) warns that the exposed library interface is unstable. These moving links motivate a spike; they are not a pinned dependency decision or evidence of registry-installable embedding.

Select an exact reviewed release/commit and record its library entry points, dependency graph, license notices, Rust toolchain, supported Python targets, configuration/environment discovery and artifact provenance. Assess the available release at implementation time; do not depend on upstream main. CodePact and Pyrefly may require different Ruff component versions. Keep AST/database identities inside each engine; do not assume similarly named Rust types can be shared.

First prove embedded checking on an in-memory fixture and a small multi-file project, including structured diagnostics and a failing type annotation. Then demonstrate the same binary finding a real CodePact lifecycle error. If usable structured APIs or reproducible source packaging cannot be qualified, record the blocker and amend the design before switching to subprocess orchestration. A wrapper around two installed CLIs must not be described as the completed embedded prototype.

## Proposed repository layout

| Planned path | Purpose |
| --- | --- |
| `crates/codepact-pyrefly-cli/` | New Rust binary package, shared CodePact engine dependency, Pyrefly adapter and combined report types |
| `pyrefly-cli/pyproject.toml` | Python distribution metadata and maturin binary configuration |
| `pyrefly-cli/README.md` | Installation, combined checking, limitations, version compatibility and helper-package guidance |
| `crates/codepact-pyrefly-cli/tests/` | Combined fixtures, upstream parity and operational regressions |
| `scripts/verify_pyrefly_distributions.py` | Separate artifact inventory, metadata, command-collision and source-install verification |
| `.github/workflows/release-pyrefly.yml` | Candidate builds, qualification and isolated publication for this distribution |

These paths describe future work. Add the crate and Python project to their workspaces only with a buildable implementation. Assess whether default workspace checks should include the heavier prototype, and keep focused checks for the core analyzer available. Initially make the new Cargo package non-publishable; qualifying PyPI wheels and source builds does not imply the dependency graph can be published to crates.io.

## Command and result behavior

Proposed commands, not available yet:

```text
codepact-pyrefly --version
codepact-pyrefly check [--root DIR] [--python-version VERSION] [--json] [FILE...]
```

- Run both engines by default, including when the first produces findings. Use readable output by default and a single versioned JSON envelope for `--json`; keep logs on stderr. `--version` and reports identify the wrapper, CodePact engine, Pyrefly version/revision and report schema.
- Establish a common project root, requested Python source target and input snapshot. Define source selection, dependency discovery, stubs, environment/search paths and config precedence. Preserve each engine's supported dependency expansion and report effective scope; do not claim identical coverage merely because entry files match.
- Respect engine-specific project settings. Document which wrapper flags override both checkers and reject contradictory or unsupported target settings clearly. Record source/configuration fingerprints so results cannot silently combine different revisions. If files change during checking, retry within a bound or return an explicit incomplete/error result.
- Preserve Pyrefly rule identities and CodePact `TP` codes, with an explicit `engine` field and engine-specific details. Normalize paths, severities and source ranges through a documented adapter while preserving original data. Test Unicode/CRLF and related locations; do not assume the engines use the same column units.
- Retain separate per-engine execution status, findings, scope and coverage/limitations alongside aggregate counts. If Pyrefly supplies no completeness guarantee, record that as unspecified; do not synthesize CodePact-style complete coverage. Keep CodePact certainty/provenance intact.
- Exit `0` when both engines completed and neither has findings that meet the configured failure threshold; `1` when both completed and either has threshold findings; `2` for invocation/configuration/execution failure, cancellation or a prevented run. Execution failure takes precedence over findings, and available partial results remain in JSON. Coverage is explicit and any strict-coverage failure policy must be documented separately; exit `0` alone is not a proof of completeness.
- Define advisory/strict behavior, suppressions and baselines per engine. A CodePact suppression must not hide a Pyrefly error or vice versa. Do not invent shared baseline semantics before they are designed.
- Bound analysis work and retained memory. Define recovery for upstream panics/errors and cancellation; a failed engine must never be represented as an empty successful result. Verify that environment discovery and checking do not import project modules or execute project hooks; any interpreter probing must be explicit and restricted to environment discovery.

## Work packages

| ID | Deliverable | Completion evidence |
| --- | --- | --- |
| PF01 — embedding spike | Pin Pyrefly, assess unstable APIs/toolchain/dependencies and build a minimal embedded check. | Reviewed dependency record, structured type-error fixture, clean locked build and packaging feasibility decision |
| PF02 — combined driver | Add the binary, common input/configuration adapter, both engine calls and versioned reports. | Clean, typing-only, CodePact-only and combined-error fixtures; per-engine identity/status/coverage preserved |
| PF03 — compatibility and workflows | Qualify Annotated metadata, helpers, imports, stubs, targets, suppressions, source locations and CLI parity. | Regression matrix plus a documented check–edit–recheck example fixing both kinds of finding |
| PF04 — package qualification | Build wheels/source archive, version mapping, notices, dependency inventory and installation checks. | Installed commands run outside the checkout; extracted archive builds without paths into a developer's repository |
| PF05 — trusted release lane | Add the dedicated workflow, artifact verifier, candidate evidence and prerelease handling. | Manual builds never publish; only matching package tags publish verified artifacts through the documented publisher identity |

PF01 precedes the dependency commitment and PF02; PF03/PF04 require a working combined driver. PF05 publication depends on all pre-publication prototype gates; registry download verification completes PF-V07 after upload. Record exact implementation issues and owners before execution.

## Acceptance and evidence

| Gate | Required evidence |
| --- | --- |
| PF-V01 — combined value | Fixtures cover valid source, wrong assignment/argument/return types, a supported CodePact lifecycle violation, both kinds together and unsupported resource syntax. Both engines run even when one reports findings. |
| PF-V02 — upstream/core parity | For the same source, scope and effective settings, compare against the pinned upstream Pyrefly checker and core CodePact results. Explain any presentation differences; no lost rules, certainty, coverage or related locations. |
| PF-V03 — typing compatibility | Ordinary classes, Annotated access aliases, decorators, helper imports, generics, stubs and supported Python targets behave as documented. Unsupported CodePact metadata remains visible as a boundary, not an inferred guarantee. |
| PF-V04 — operations | Invalid config, target mismatch, missing imports, syntax errors, source changes, cancellation, work limits and an engine failure preserve honest status and correct exit precedence. Source fixtures prove no project execution. |
| PF-V05 — reporting/consumers | Human and JSON reports agree; Unicode, CRLF and cross-file locations resolve correctly; IDE/agent consumers preserve engine identity and analysis boundaries. The initial prototype requires CLI/JSON fixtures, not new LSP/MCP servers. |
| PF-V06 — distribution | Clean wheel and extracted-sdist installations pass; license/dependency inventory and locked sources are complete. Co-installation with existing tools overwrites no commands and dependency metadata has no cycles. |
| PF-V07 — release/recovery | Tag/version validation, separate artifact inventory, OIDC identity, attestations and non-publishing manual builds are reviewed. Downloaded prerelease artifacts pass smoke checks; a partial upload is resumed without rebuilding or overwriting published bytes. |
| PF-V08 — cost | Record build duration, wheel/source sizes, startup, combined-check latency and peak memory against separate runs on a named corpus. Agree acceptable prototype budgets before measurements and explain duplicated dependencies. |

Use the [common release gates](release-gates.md) where applicable and retain a [candidate evidence record](release-evidence-template.md), including exact core/Pyrefly/wrapper revisions. PF-V07's downloaded-artifact smoke check is post-publication, like G8; all other checks are pre-publication. A failed download check leaves the release incomplete and triggers recovery. Main-package version alignment under D015 applies to the existing two packages, not this independently versioned prototype. Keep all gates pending until evidence is linked.

## Packaging and trusted publishing

The maintainer supplied a PyPI screenshot confirming the publisher configuration below on 2026-10-30. This records the configured identity; successful OIDC publication remains untested. See [PyPI's publisher setup documentation](https://docs.pypi.org/trusted-publishers/adding-a-publisher/).

| Publisher field | Value |
| --- | --- |
| PyPI project | `codepact-pyrefly-cli` |
| GitHub owner | `eddiethedean` |
| GitHub repository | `codepact` |
| Workflow filename | `release-pyrefly.yml` |
| GitHub environment | `pypi` |

The screenshot does not establish whether this is a pending publisher or an existing project's publisher. PyPI's [pending publisher flow](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/) can create a project on first publication. Workflow implementation, candidate qualification and a successful publishing run remain outstanding; no release gate is passed by this configuration alone.

The dedicated workflow should build candidates on manual dispatch and publish only on validated tags such as `codepact-pyrefly-v0.1.0a1`. Validate the tag against Python metadata, the corresponding Cargo prerelease version and recorded engine versions; specify/test the PEP 440 to Cargo version mapping. The tag prefix keeps prototype publishing distinct from the existing `v*` core releases. Reuse existing quality/build practices, with prototype-specific jobs and artifact selection; do not pass the new package into the existing two-project inventory verifier.

Start qualification with Linux glibc x86-64, macOS ARM64 and Windows x86-64 binary wheels plus one source archive. Pin compatible builders/toolchain and declare/test minimum platform versions and Python installer requirements. The core eight-target matrix is an expansion target, not an inherited compatibility claim; qualify musl and additional architectures separately. Prefer Python-independent binary wheel tags where verified; wheels require no compiler or separately installed Pyrefly at runtime. Source archives must contain all required CodePact and pinned upstream sources or registry-resolvable dependencies, lockfiles, manifests, build inputs and notices. Review any vendoring approach before acceptance and prove an extracted-source build.

Only the publishing job receives `id-token: write`, uses environment `pypi` and uploads the exact verified `codepact_pyrefly_cli` artifacts with attestations. Build jobs get no publishing credentials. Core package/crates.io publication is outside this workflow. Publish the first prototype as a prerelease after candidate qualification and a separate release decision; configuring trusted publishing alone does not authorize a release tag.

## IDE, agent and future scope

Reuse the contracts in the [editor plan](linting-and-editors.md) and [AI-agent plan](ai-agent-integration.md). Their future adapters may expose the combined report with engine labels and per-engine status. The first prototype is a combined CLI, with editor external-tool/agent JSON recipes; it does not merge two language servers or promise new MCP support.

Sharing internal inference facts, a single incremental database, cross-engine automatic fixes and replacing the default CodePact distribution require separate evidence and scope decisions after the prototype. Adoption of this package does not change the core analyzer's resource-focused mission.

## Inherited concurrent resource boundaries

As the embedded CodePact engine gains [CS01–CS07](concurrency.md), preserve its async model assumptions, capture/close/join sites, certainty and unsupported process/thread coverage in combined reports. PF-V02/PF-V05 parity must cover a supported async lifetime case and an unsupported handoff when those core capabilities are included. Upstream ordinary typing success cannot discharge task ownership or cleanup obligations. This adds no concurrency capability to the initial synchronous prototype and does not require a separate concurrency engine.

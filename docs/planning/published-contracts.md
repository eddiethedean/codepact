# Publishing CodePact contracts in Python packages

Status **0.7 installed-wheel must-use preview published; resource/ownership, manifests and later adoption work planned across 0.8–0.13**. Scope includes the Python helper, package
artifacts, the standalone CLI, Rust analysis sessions, and tools that embed
CodePact. The dependency checking and artifact formats are preview designs.

A library author can publish behavioral contracts with the same wheel that
ships the library. An application developer can then use CodePact to check calls
against the contracts of the packages they actually installed. Ordinary Python
callers continue to use the library through its regular API. The first useful
result is a discarded `@must_use` result caught across a wheel boundary. The
same boundary can later carry ownership, lifecycle, and protocol effects.

## Publishing and consuming a contract

Suppose the `acme-storage` distribution exposes `acme_storage.upload`.
The author marks `upload` with `@must_use` and includes an empty `py.codepact`
file in the `acme_storage` package directory. The public import can be a static
re-export from an implementation module in the same distribution.

```text
acme_storage/
    __init__.py
    _upload.py
    py.codepact
    py.typed
```

In `acme_storage/__init__.py`, the author writes
`from ._upload import upload as upload` to expose the annotated function.
`py.typed` is independent and belongs in the wheel if the package publishes
typing information under the Python typing specification. `py.codepact`
advertises CodePact contracts for the package and its eligible descendants.
The wheel is the source of that signal after installation. Build backends can
include the marker as package data, and a future build hook can generate it
from author configuration in `pyproject.toml`. Ordinary Python imports require
no marker processing. [Typing specification](https://typing.python.org/en/latest/spec/distributing.html#packaging-type-information) · [Wheel format](https://packaging.python.org/en/latest/specifications/binary-distribution-format/)

```python
from acme_storage import upload

upload("report.csv")  # TP3001 when dependency contracts are enabled
```

The primary location is the consumer call. A related location identifies the
installed declaration of `acme_storage.upload`, including the distribution
and installed version. The caller can handle the receipt in ordinary Python.
`discard(...)` remains an explicit way to intentionally abandon it. The rule
checks immediate handling, including forwarding or returning the result.
[Must-use semantics](../rules.md#tp3001-discarded-must-use-result)

## Package artifacts and public exports

### Inline declarations

An empty `py.codepact` plus distributed `.py` source is the first format.
The marker covers one import package within one installed distribution. Its
scope includes supported descendants owned by that distribution. A distribution
shipping multiple import packages marks each one independently. For a shared
namespace such as `acme`, place the marker in the owned portion such as
`acme/storage/py.codepact`. A marker at a shared namespace root is ambiguous.
The initial format requires a regular import package. Shared namespaces, single
modules, and binary-only providers receive individual acceptance coverage before
being advertised.

The exporter recognizes public module members, a statically resolvable
`__all__`, and explicit re-exports in the supported subset. In the absence of
`__all__`, it starts with non-underscore symbols defined in public modules and
explicit same-name re-exports such as `from ._upload import upload as upload`.
Private declarations can supply facts for an exported symbol, including its
canonical identity. The public name and canonical definition are both recorded.
For inline publication, take behavioral declarations from the selected `.py`
implementation. A `.pyi` file can continue to supply ordinary types. A later
stub format for contracts needs its own artifact binding and precedence rules.
Unresolved export lists, star imports, module `__getattr__`, and dynamic
rebinding leave affected exports unresolved. Local source, inline providers,
and manifests must agree on the identity of a supported re-export.

### Versioned manifest

When source alone becomes limiting, use the same marker and an optional
`codepact.contracts.json` beside it. The manifest becomes the package's
publication artifact when present. The importer uses the inline path when the
manifest is absent. An invalid, incompatible, stale, or incomplete manifest
gives an explicit coverage reason for the affected facts. An omitted symbol
in a valid manifest has no published contract for that public name. This keeps
the public interface stable when internal source carries extra annotations.

| Record | Required information |
| --- | --- |
| Header | Manifest schema version, producer version, distribution name and version, import scope, and target applicability |
| Symbol | Qualified public name, canonical definition identity, callable or resource kind, and supported signature selectors for binding effects |
| Contract | Typed contract kind and version, precise subject such as receiver, parameter or return, payload, and required capabilities |
| Evidence | Declaration origin and optional supporting evidence with scope, assumptions and package-relative location where available |
| Artifact binding | Digests of the source or shipped runtime files that supplied relevant evidence, using canonical package-relative paths |

For example, an export for `upload` could look like this. The containing
manifest supplies distribution identity, target applicability and file digests.

```json
{
  "public_name": "acme_storage.upload",
  "canonical_name": "acme_storage._upload.upload",
  "symbol_kind": "function",
  "contracts": [
    {
      "kind": "must-use",
      "version": 1,
      "subject": "return",
      "required_capabilities": ["synchronous-result-use"],
      "evidence": {
        "kind": "declared-unverified",
        "path": "_upload.py",
        "line": 4
      }
    }
  ]
}
```

Manifest versions evolve independently of the library, CodePact engine, and
checker report. Define deterministic ordering, unique records, normalized
distribution names and versions, and explicit Python/platform bounds where
relevant. Reject duplicate keys and contradictory records. An unsupported
schema makes the manifest unavailable. An unsupported mandatory capability
makes the dependent symbol or effect unavailable. Independent understood facts
can still be used when the schema establishes that independence.

Use the existing inspector and contract normalization to generate an artifact
from staged package inputs. Verify the final wheel's marker, distribution
identity, manifest and listed inputs against the wheel contents and `RECORD`
before upload. Hash the relevant packaged files independently of the manifest
and `RECORD` to avoid a circular digest. On install, compare applicable
contract inputs with the files actually present before using those facts.
Digests establish artifact consistency. Evidence kind and consumer policy
determine how a behavioral fact can be used. Binary or generated APIs can
publish data with their packaged artifact and accompanying evidence.
[Wheel integrity records](https://packaging.python.org/en/latest/specifications/binary-distribution-format/#the-dist-info-directory)

## Resolving the consumer environment

Enable installed-package analysis by project policy or an explicit invocation.
Start with ordered, explicit import roots for a selected environment and the
existing Python target setting. A conventional virtual environment shortcut
can follow once its POSIX and Windows layouts are qualified. Keep the chosen
environment distinct from the location of the CLI executable. A CLI installed
globally or through `uv tool` may be checking a project in a different venv.

The pilot command would take repeatable `--dependency-root` flags in search
order. A project could record an equivalent configuration with paths relative
to its `pyproject.toml`. CLI roots replace the configured list when supplied.
The section below is proposed syntax.

```toml
[tool.codepact.dependencies]
roots = [".venv/lib/python3.12/site-packages"]
required-providers = ["acme-storage"]
```

1. Resolve an import against the project's supported module roots and then
   the selected environment's ordered roots. The provider of the resolved
   module wins, even when a later copy of that module is marked. A package's
   submodule stays within its resolved package search path. Ambiguity outside
   supported resolution is a coverage boundary.
2. Connect the selected module and marker to the same distribution using
   installed `.dist-info/METADATA` and its file inventory. Distribution names
   and import names can differ. If ownership cannot be established or the
   files have conflicting owners, leave the relevant contract unresolved.
3. Open the selected provider for contract discovery only if it advertises
   contracts or the consumer has configured a matching model. Follow static
   re-exports and contract references on demand within explicit depth, file,
   edge, record and byte limits. For a re-export from another distribution,
   resolve its actual origin and use that provider's publication or an
   applicable reviewed model. A re-export alone confers no authority over
   another distribution's implementation.
4. Report relevant unresolved dynamic imports, cycles, opaque editable
   layouts, and exhausted budgets where they affect checked behavior.
   Support editable installs later through explicitly mapped source roots
   with verified distribution identity. Custom import hooks and executable
   `.pth` files are outside static resolution.

Read package source, manifests and metadata as data. The CLI performs no
target imports, annotation evaluation, environment startup, build execution,
or network lookup to resolve a contract. Bound filesystem reads to approved
roots and require safe regular files. Wheel commands also reject escaping or
duplicate member paths and cap archive expansion. Installed `RECORD` paths
can refer outside the import root, so they establish ownership only after path
validation. [Installed package records](https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file) · [Editable installations](https://peps.python.org/pep-0660/#what-to-put-in-the-wheel) · [Current analysis boundary](../security.md#what-the-analyzer-reads-and-does)

## Contract semantics across package boundaries

| Capability | First supported package boundary | Evidence needed from a supplier |
| --- | --- | --- |
| Result use | A direct resolved synchronous call discards a required result | A supported `@must_use`, alias, or equivalent manifest declaration |
| Lifecycle and cleanup | A factory returns a tracked resource, or a method closes it | Resource identity and an established return or receiver effect for the relevant exit. A resource type alone supplies no open state or cleanup duty. |
| Ownership and borrowing | A call requires owned, shared, or exclusive access. A modeled call consumes an argument or returns a tracked alias. | A supported signature and access declarations for entry checks. Transfer, escape, and alias effects follow the semantics of the qualified ownership mode and its modeled commit points. |
| Typestate and effects | A method requires an allowed receiver state and can change it | A resolved state machine, qualified entry requirement and an applicable exit effect. A protocol state named `Closed` supplies no lifecycle effect by itself. |
| Cleanup through context managers | A supported exit discharges duty on the tracked object | Entry aliasing, applicable exit effects and exception suppression behavior. Failed entry, raised exit and cancellation retain their own paths. |
| Async tasks and typed outcomes | A supported awaited call or outcome branch carries an established obligation | Qualified per-outcome records. Coroutine creation, cancellation requests and assignment to an outcome variable supply no completion proof. Tracking owned payloads follows its separate qualification work. |
| Value and field contracts | A supported caller argument or observed field violates a published constraint | Qualified scalar, relationship or field semantics with explicit applicability before those facts affect checks. |

Keep caller obligations and supplier guarantees distinct. A declared
`@must_use` tells the caller how to handle a result. A promise that a call
closes a resource can discharge a cleanup duty, so it needs checked source
or a reviewed applicable model by default. An explicitly configured
publisher assumption can enable selected declared effects with visible
provenance and conditional findings. A manifest claiming its own
implementation is checked does not elevate consumer trust on its own.
Verified-source evidence can be rederived from the exact dependency source
with the supported analysis. Its scope includes the preconditions and paths
actually checked. Reviewed models carry an explicit version and applicability
record. A known contradiction makes that promise unavailable to callers.

Represent normal, exceptional and cancelled exits separately. Missing or
untrusted effects weaken only the facts they can affect on feasible paths.
Preserve independently established violations and apply the current rules for
definite, possible and unknown findings. Contract decoding grants no extra
runtime validation. Existing ownership, advanced and future feature modes
still control which qualified semantics the engine applies.
[Contract declarations](../contracts.md#what-the-declarations-mean) · [Advanced contracts](phases/0.6/specification.md#calls-joins-and-completion) · [Typed outcomes](phases/0.7-typed-outcomes.md) · [Follow-on contract phases](phases/contract-extensions.md)

## Models, policy, and downstream reports

Record why each fact applies and where it came from. Use the selected package's
publication for its declared caller obligations. Allow an applicable reviewed
model to supply a missing effect or substantiate a declaration. Contradictory
facts make the affected dimension unknown and surface the conflict. Local
overrides are explicit, version-bounded replacements for a selected symbol
and contract kind. They report the replaced origin and are included in the
effective configuration and semantic inputs. An override cannot target a
different module that shadows the named distribution. Curated standard library
models continue to use their own interpreter and platform applicability.

The initial opt-in should offer an advisory mode with the existing strict
coverage policy available for CI. An optional list of required providers can
surface a missing publication for a resolved imported distribution that the
consumer expects to supply contracts. For an unmarked dependency without an
explicit expectation, retain the normal unresolved behavior when flow requires
an unavailable fact. The consumer owns severity and suppression policy.
Reports retain the underlying coverage reason and provider provenance when
a baseline or suppression changes a finding's disposition.

Use existing rules for caller findings and explicit coverage reasons for
provider problems. Include public and canonical symbol names, installed
distribution and version, origin kind, applicable digest, and available
declaration spans as evidence. Prefer package-relative locations in machine
reports. Dependency files are inputs to caller analysis. Author-facing
findings belong in author checks and artifact verification. Keep caller
fingerprints stable across relocation and semantically equivalent provider
upgrades. A changed requirement or evidence affecting the finding receives a
new semantic fingerprint, so baselines do not hide a changed contract. A
location-only change refreshes related locations and the report snapshot.
[Current report contract](../output.md#cli-adoption-report)

## Package and tool integration

| Surface | Proposed behavior |
| --- | --- |
| Python helper | Existing decorators and aliases supply inline declarations. `move` and `discard` are caller operations and have no publication record of their own. |
| Author tooling | Required in 0.8 under E10-A: `codepact contracts export` generates the manifest from staged inputs and `contracts verify` checks an actual built wheel against its publication. Required in 0.9 under E10-B: `contracts diff` compares public caller obligations and guarantees from two local artifacts. These are proposed command names; generation of API reference tables is an optional later use of the export. |
| Consumer CLI | Start with `codepact check --dependency-root PATH` and a matching configuration setting. Expose selected roots, target, and policy through `--show-settings`. `baseline update` uses the same effective inputs as `check`. A proposed `contracts show` can report the selected rule, source, and applicability for a resolved public symbol. Expose manifest/feature support in `capabilities`. |
| Rust API | Accept an explicit snapshot of externally supplied, resolved provider data or source and preserve the existing project-source entry points. Filesystem and environment discovery live in the host. Normalize providers to the same contract vocabulary as local modules before call analysis. |
| Salsa and persistent cache | Cache immutable provider facts by scoped identity, relevant file digests, schema, target, model and normalization versions. Include resolution order, missing providers and policy in applicable invalidation. Recheck dependent calls when facts or provider selection change. Retag locations and suppress stale results across environment changes. Verify incremental and fresh parity. |
| LSP, agent tools, and TypePact | Reuse the same environment selection and CodePact engine. Surface provider details on hover, related locations, structured checks and explanations. TypePact can contribute qualified type evidence for symbol selection while CodePact still decides which behavioral contract applies. |

Starting with 0.7, a library that imports CodePact declarations can depend
on `codepact` for its Python helpers. Developers who run the checker can
install `codepact[cli]` to get the helpers and their exact matching CLI, or
install `codepact-cli` directly for the standalone command. Projects upgrading
from an implicit CLI dependency should request the extra wherever they invoke
the command, including CI. Verify that migration in a fresh environment
because a package manager can leave the previously installed CLI in place
after a helper-only upgrade. A manifest-only author workflow can generate
and verify metadata at build time for a runtime package that needs neither
distribution. [Packaging decisions](decisions.md) · [0.7 plan](phases/0.7-typed-outcomes.md) · [Python packaging extras](https://packaging.python.org/en/latest/specifications/pyproject-toml/#dependencies-optional-dependencies)

### Author workflow release requirements

**0.8 owns the complete bounded export and verify workflow (E10-A).** Qualify
the installed CLI, not only internal APIs or hand-written fixture manifests:

1. Export a deterministic manifest from supported staged package inputs,
   retaining normalized contracts, evidence kinds, distribution/import-scope
   identity and relevant packaged-file digests.
2. Build an actual wheel using an explicit author or controlled fixture step.
   The exporter and verifier do not run the build backend or import or execute
   the target package.
3. Verify the built wheel's marker, manifest, distribution identity, relevant
   files and `RECORD` consistency. Reject stale, malformed, wrong-distribution
   and mismatched-file metadata. Unsupported schemas or mandatory capabilities
   retain their declared boundary; they cannot establish verified affected facts.
4. Install the provider outside its source checkout, then check a downstream
   caller through the selected dependency environment. Compare matching inline
   and manifest variants for two independent providers, including cleanup,
   ownership transfer and exceptional unknowns. Repeat after an upgrade or
   shadowing change through CLI/Rust and fresh/reused analysis.

Include a runtime package whose useful manifest requires neither the Python
helper nor the CLI as a runtime dependency. Retain reproducibility and invalid
artifact controls. Consumer checks must expose unavailable affected facts
rather than bypass a failed manifest with unchecked inline effects. The same
archive/path limits and static-reading boundary apply to author commands.

**0.9 owns the bounded artifact comparison workflow (E10-B).** The installed
CLI compares two actual local wheel publications and reports compatible
changes, new caller obligations, weakened supplier guarantees and insufficient
evidence in deterministic human-readable and machine-readable output. Pair
real downstream callers with compatible, caller-breaking and unknown controls;
an applicability or evidence loss cannot establish compatibility. This command
compares library publications independently of the CR source/report change-review
workflow. Both may reuse normalized facts, but qualification of one does not
qualify the other.

The [mandatory E10 checkpoints](phases/carry-forward-gates.md#mandatory-e10-checkpoints)
block their respective releases. E10's overall completion remains in 0.12 for
supported-editor parity. Command names remain provisional; these end-to-end
author capabilities and their release owners are required.

## Contract compatibility and upgrades

Compare changes from the caller's perspective. A stronger precondition, a
new must-use obligation, or a weaker cleanup guarantee can break analyzed
callers even if the Python signature is identical. A weaker precondition or
stronger guarantee can be compatible within its recorded target and trust
scope. Compare against public import paths so moving an implementation while
preserving its re-export and semantics produces an implementation move.
Loss of applicability, evidence, or analyzability gets a distinct result
because a compatibility conclusion may be unknown.

| Change | Example caller consequence |
| --- | --- |
| New obligation | A previously legal bare call now drops a receipt. |
| Narrower precondition | `send()` now requires an `Open` session. |
| Changed ownership or aliasing | `finish()` consumes a handle and the caller still uses it. |
| Weaker postcondition | A close operation previously guaranteed closure on raise. Those callers now need a recovery path. |
| Changed support or evidence | A platform-specific wheel no longer publishes a usable effect for the selected runtime. Review coverage before calling the upgrade compatible. |

Run a diff in author CI against the last released wheel for each supported
target, and attach user-facing migration guidance to changed obligations.
Consumer CI can run its ordinary check against an environment with a candidate
wheel, which answers whether that particular application exercises the changed
contract. Keep the diff deterministic and machine-readable so either project
can choose which categories fail its CI.

## Implementation and acceptance

| Stage | Work and acceptance evidence |
| --- | --- |
| 0.7 published install split and result-use preview | Qualify all three install paths. Build marked and unmarked pure-Python wheels. Install each outside its source checkout. Compare caller diagnostics for a public direct import, a renamed import, an explicit re-export, handled and ignored results, a same-named local symbol, an unmarked shadow, a second environment with a different package version, and target code that would leave a sentinel if executed. Confirm Rust and CLI parity on the same resolved inputs. |
| 0.8 reviewed resource and ownership foundations | Use a provider factory, a downstream wrapper, and an independent provider consuming or closing the same resource. Verify a tracked alias, cleanup duty after a failed call, exceptional unknown, no proof from declaration alone, and repair with the same ordinary Python types. Exercise conflicts, strict coverage, and fresh/reused parity. |
| 0.8 author export/verification and manifest integrity — E10-A | Complete the installed-CLI export → explicit wheel build → verification → downstream-consumption workflow. Compare inline and manifest facts for two independently installed providers. Qualify reproducible artifacts, distribution/file binding, stale and invalid metadata rejection, unknown capabilities, exceptional unknowns and CLI/Rust/fresh/reused parity after upgrades or shadowing. A manifest-only runtime package needs no helper or CLI dependency. |
| 0.9 author artifact comparison — E10-B | Compare two actual local contract artifacts through the installed CLI, retaining deterministic human and machine output. Qualify compatible changes, new caller obligations, weaker guarantees and insufficient evidence, independently of source/report review. |
| 0.12 cache/editor completion — E10-C | Requalify applicable artifact and author-tool evidence and complete cache/editor parity on the selected environment scope. Close E10 only after all its original clauses and mandatory checkpoints pass. |
| 0.13 readiness review | Rehearse author and consumer upgrades. Audit manifest evidence, baselines, fallback boundaries, target support, and invalidation after upgrades and shadowing. Measure cold and warm time and memory on a project with a large installed environment and a small reachable dependency set. Decide which preview surfaces qualify for 1.0. |
| Further qualification | Add selected binary providers, namespace and editable layouts, platform-dependent effects, and external models as each has retained acceptance evidence. Extend the published semantics as async, outcome and value contracts qualify. |

First test two separate projects and an installed wheel. An in-repository
multi-file fixture cannot establish package-data inclusion or environment
selection. Retain positive, negative, and incomplete-coverage cases for each
capability, along with package identities, platform and target support,
diagnostic evidence, and budget measurements. Establish the normalized
resource boundary before publishing the manifest format. Persistent caches
and editor integration complete in 0.12 after provider, obligation and comparison semantics qualify.
[0.7 plan](phases/0.7-typed-outcomes.md) · [0.8 foundations](phases/0.8-reviewed-concurrency-foundations.md) · [0.9 obligations/review](phases/0.9-obligations-and-change-review.md) · [0.12 integrations](phases/0.12-ecosystem-and-integrations.md) · [0.13 readiness](phases/0.13-stabilization.md) · [Release sequence](../../ROADMAP.md)


## Expanded release mapping

The [workstreams](contract-and-portfolio-workstreams.md) retain HTTPX and HTTPX2 as separate providers and SQLModel as an independently qualified adapter over selected SQLAlchemy behavior. Synchronous and separately promoted direct-await profiles enter in 0.9; advanced streaming/cancellation follows in 0.11. Framework, native-provider and first-party wrapper publication follows qualified evidence in 0.12. The [registry](phases/carry-forward-gates.md) retains every E10–E12 and S08–S09 requirement. E10-A blocks 0.8 on artifact integrity and author export/verification; E10-B blocks 0.9 on artifact comparison. Partial evidence cannot close E10 before E10-C editor parity in 0.12.

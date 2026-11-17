# Testing, performance and quality plan

This is the living verification strategy for the inspector, lifecycle checker,
ownership previews, and Python helpers. Historical phase qualification records
remain evidence for their named candidates. Current automation and future work
are distinguished below; a successful compatibility job does not establish
that every model's required conformance probes executed.

## Correctness layers

| Layer | Required evidence |
| --- | --- |
| Parser and resolver | Ruff AST/ranges, syntax versions, recovery, scopes, binding events, captured aliases, imports and cycles |
| Salsa queries | Fresh/reused parity; no-op and unrelated-query reuse; exports, missing modules, settings, revisions and budget invalidation |
| Contract lowering | Canonical snapshots from both APIs, conflicts and alias resolution |
| Flow engine | Transfer/join unit cases; loops, exceptions, cleanup and returns |
| Diagnostics | Golden outputs with codes, spans, related sites and certainty |
| Interoperability | Pythonic producer / explicit consumer and reverse combinations |
| Runtime API | Identity, signatures, pattern matching, conversion and exception behavior |
| Packaging | Clean install, import, CLI, wheel contents and typing markers |
| Compatibility | Pinned checker and framework fixtures per published matrix |

## Essential regression corpus

Cover use before/after close; alias close; conditional close; close raising; idempotent close; early return; cleanup in finally; context exit failure; resource escape; unknown call; move followed by exception; double move; overlapping borrows; returned closure; await/yield boundaries; guarded match; shadowed API names; malicious annotation expressions.

Every reported bug should add the smallest relevant fixture and, where meaningful, its cross-interface equivalent.

Qualification fixtures should normally include a violation, a source repair, a
valid control, and an unsupported boundary. Require complete supported-subset
coverage for valid controls. For intentionally incomplete cases, specify the
boundary and its evidence explicitly. Compare diagnostic multisets, certainty,
completion, trust, primary statements, and related origins as relevant to the
claim; absence of one error code is not evidence of complete analysis.

The Phase 0.6 preview includes a provider/re-export/consumer typestate project.
Its permutation test exercises all six orders through a buggy/fixed/buggy
history on every supported source target. Snapshot installation removes old
files so deleted providers cannot accidentally survive a repair or control.

## Properties and differential checks

`cargo test -p codepact --locked --test semantic_properties` runs an independent
bounded oracle over all 256 four-leaf close combinations, both alias mappings,
and source targets 3.11–3.13 (1,536 analyzer comparisons). Its concrete resource
store is independent of the analyzer's transfer and join implementations. The
oracle checks resource identity, use-after-close findings, surviving cleanup
duties, certainty, and source evidence. Additional cases check normal/exceptional
cleanup in `finally`, unknown effects, and syntax perturbations including Unicode.

Reproducible edit histories compare full fresh/reused reports after source,
provider, target, location, and input-order changes, provider deletion/restoration,
cancellation, reset, and no-op reuse. Known supported and missing-provider states
have independent expectations in addition to parity. The default is eight
histories of 24 steps; `CODEPACT_EDIT_HISTORIES` and `CODEPACT_EDIT_STEPS` select
larger bounded runs. Histories must be in 1–256 and steps in 10–256, so every
history exercises all ten edit categories and zero cannot silently disable a run.
Failures identify the seed, step, operation, or complete generated source.

These are exhaustive small-language cases and deterministic histories, not a
coverage-guided fuzzer or a proof of arbitrary Python soundness. Broader generated
grammars, automated shrinking, and targeted mutation testing remain follow-up
work. Preserve minimized failures as ordinary regression fixtures.

Ordinary checkers validate base typing compatibility, not CodePact ownership semantics. Differential results are evidence to investigate, not a substitute oracle for new rules.

## Performance

Create a versioned corpus with small modules, large dependency graphs, alias-heavy functions and recursive summaries. Record hardware, OS, analyzer revision, corpus revision, cold/warm cache state, elapsed time, peak memory and percentile incremental latency.

Proposed engineering budget after baseline: flag over 10% median regression on repeated controlled runs and investigate before release. Set absolute latency targets only after M1 measurements. Benchmark runtime import separately from analyzer execution and optional runtime enforcement.

The [Phase 0.7 capture script](../../scripts/capture_phase07_performance.py) runs ten adjacent baseline/candidate process pairs per comparable workload and alternates which version runs first. Use `CODEPACT_PHASE07_PROCESS_REPEATS` to set the pair count. Each process times seven independent sessions. For each metric, `paired_change` compares the two process medians within each pair and reports the median difference in microseconds, the median percentage change and the number of pairs in which the candidate was slower. Review the paired changes from repeated captures against the investigation threshold. The per-version pooled upper medians remain available for historical comparison. The raw replicates and `pair_order` record how to recompute both summaries.

## CI and release gates

Pull requests run formatting, static checks, the Rust/Python suites, exact typing
controls, documentation validation, packaging checks, and required model probes
on their exact runtimes. The stable `Required quality` aggregate fails if any
required job fails, is cancelled, or is skipped. Repository branch protection
must select that check to enforce it at merge time; defining the job alone does
not change repository settings.

The weekly schedule and manual workflow run execute an extended semantic corpus
with 128 edit histories of 64 steps, preserving logs and reproduction inputs.
The schedule also runs model conformance and dependency auditing. Release
automation retains the complete advertised artifact/platform matrix. Performance
captures remain explicit qualification work; automatic benchmark thresholds,
coverage-guided fuzz campaigns, and mutation campaigns are not current CI jobs.

Future parser and IR fuzzing must use bounded resources and produce controlled
failure reports rather than crashes. Existing side-effect sentinels continue to
enforce source-only analysis. Runtime conformance executes only repository-owned
probes, never user projects selected for static analysis.

### Exact runtime model qualification

[`runtime-models.json`](../../tests/conformance/runtime-models.json) registers
required test IDs for contextlib and TaskGroup on CPython 3.11.15, 3.12.13, and
3.13.16, plus the executor probes on 3.13.16. The workflow derives its runtime
matrix from this registry. Run one qualified environment explicitly:

```sh
uv run --no-project --python 3.13.16 python scripts/check_model_conformance.py \
  --python-version 3.13.16 --receipt target/test-evidence/conformance.json
```

The runner checks the interpreter before importing probes and requires exact
registered/discovered/started/passed test identities. Missing cases, skips,
expected failures, unexpected successes, and failed setup/teardown/subtests fail
qualification. JSON receipts include the actual environment and source hashes.
The ordinary floating Python 3.11–3.14 matrix remains a separate compatibility
check; its intentional version-specific skips do not supply model evidence.

### Exact typing controls

`scripts/check_typing_contracts.py` checks the valid fixtures and every registered
negative case under the pinned Mypy, Pyright, and Basedpyright versions. It
matches file, line, diagnostic rule, and severity, with case-specific fragments
for broad diagnostic categories. Missing, extra, duplicate, or misplaced errors
and checker failures cannot pass. The installed-wheel checks reuse the same
expectations while clearing source-tree import paths.

A release requires no unresolved critical soundness regression in claimed scope, no unexplained interoperability mismatch, and a reviewed compatibility report. Publish known failures and unsupported cases explicitly.

## Analyzer migration qualification

0.1 requires [V01–V24](phases/0.1/validation.md). Preserve regression cases for multiline declarations, quote-like string/comment contents, defaults/body calls, every annotation position, metadata conflicts, imported lookalikes, cross-file isolation, captured aliases and repeated class names. A valid 24,000-alias source below the byte limit must terminate without stack overflow; actual limits must be enforced before traversal.

Compare fresh and incrementally updated sessions after each edit, including an unchanged request, unrelated-file edit, exported-contract change, body-only edit, moved source spans, missing import creation, deletion, settings/target/model change and cancelled request. Use Salsa execution events to establish reuse separately from parity. Never weaken an assertion to unrelated names to make a fixture pass.

Record cold-session, warm no-op, local-edit and dependency-edit measurements separately. Disk-cache and editor cancellation stress tests expand this corpus in 0.8; basic in-memory query correctness is required from 0.1.

Use current test-run output and conformance receipts for execution inventory;
historical phase counts are not a current coverage metric. The suite covers
provider and model edits, missing-provider addition/removal, moved spans, no-op
reuse, session revisions, cancellation recovery, and budget cache stability.
Clean Cargo archive and installed distribution checks also exercise lifecycle
findings and verify source non-execution. These implementation regressions do not
replace exact-candidate qualification, compatibility review, or release decisions.

## Python helper and CLI distributions

Packaging checks require a pure helper wheel whose optional `cli` extra declares
the exact native `codepact-cli` dependency, eight CLI platform wheels, and separate
helper/CLI source archives. Local checks build through the sdists, install local
artifacts into fresh environments outside the checkout, and verify helper-only,
CLI-extra, standalone CLI, version/help, inspection, lifecycle, and source
non-execution behavior. Release CI builds all eight targets and validates
inventory/metadata. Historical evidence is retained under the release it qualifies.

## Next qualification work

Extend the runtime registry into a shared runtime/static case registry before
scaling reviewed library packs. HTTPX/HTTPX2 and SQLAlchemy/SQLModel need distinct
version applicability and controlled upstream probes. Add real-project adopted
boundaries with independently specified bugs, repairs, valid controls, and held-out
cases; synthetic success does not establish field precision or recall. Before/after
review must distinguish a semantic repair from lost coverage, changed policy,
suppression, excluded scope, or model applicability. Persistent obligations need
an independent state-machine oracle as that feature is implemented.

## Planned concurrent resource qualification

Add the full [concurrency validation corpus](concurrency.md) as CS01–CS07 land, retaining valid/invalid/unknown and mixed-interface cases. Test coroutine creation without execution, capture escape, join-before-close, close-before-join, transfer followed by scheduling failure, cancellation before start/during use/during cleanup, exceptional group exit, pending cancellation, overlapping borrows and suspension. Unmodeled detached work, process queues/executors, serialization, shared memory and thread offload must not produce isolation or completion evidence.

Repeat fresh/updated parity after capture/join/cancellation/model changes, source-span moves and task-budget exhaustion; integration/caching hosts must preserve related sites, certainty and incomplete coverage. Distinguish analysis-request cancellation fixtures from target-program cancellation fixtures. Optional controlled runtime model-conformance tests are separate from source-only analyzer tests and never execute users' target code. Pin interpreter/library/platform/start-method combinations for any semantic support claim; unsupported cases are boundary evidence, not verified process support.

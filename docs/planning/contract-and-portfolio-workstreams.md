# Contract and portfolio workstreams

Status: **Planned; future acceptance pending**. These are the existing product-plan work definitions, not new meanings for released APIs or historical gates. Use the qualified names **library LC**, **review CR**, **outcomes OH** and **portfolio PC**. Library LC01–LC14 here are distinct from the historical 0.2 LC01–LC07 lifecycle execution steps.

The released 0.7 must-use preview is the baseline. Persistent acknowledgment obligations, provisionally named MustHandle, are new work requiring OH01 semantics. Assignment or retention does not alone establish semantic handling.

## Release allocation

| Work | First delivery | Later qualification |
| --- | --- | --- |
| Library LC01–LC04, LC06, LC08 | 0.8 provider/provenance, file pack, consumer inspection and mandatory E10-A author export/wheel verification plus artifact/in-memory evidence | E10-B author artifact comparison in 0.9; persistent-cache/editor and E10-C completion in 0.12. |
| Library LC05, LC09–LC10 | 0.9 separate HTTPX/HTTPX2 and SQLAlchemy/SQLModel profiles | Each provider and configuration needs independent evidence. |
| Library LC07, LC12 | 0.9 separately promoted direct-await profiles with required AS07 operation-sharing controls | Advanced cancellation/streaming extends in 0.11. |
| Library LC11, LC13–LC14 | 0.12 framework, wrapper-publication and native-provider adoption | Promote only exact artifacts/configurations that qualify. |
| Review CR01–CR05, CR07–CR09 | CR01 descriptor foundations in 0.8; full bounded comparison and modernization/environment/scope provenance in 0.9 | Broader native/provider families follow their own models. |
| Review CR06 | 0.9 CLI/in-memory comparison parity | Cache/editor/MCP parity in 0.12. |
| Outcomes OH01–OH07 | OH01 semantics in 0.8; local tokens, variants, helper/forwarding effects and direct awaits in 0.9 | Advanced cancellation/cleanup profiles in 0.11. |
| Outcomes OH08 | Conditional on qualified TypePact TC03 and core evidence | Does not imply TC03 has shipped or block the independent TypePact alpha. |
| Outcomes OH09–OH10 | 0.12 qualified adapters and fields | PC02 is required before OH10. |
| Portfolio PC01 | 0.8 initial modern annotation/provider shapes | Extend only with package-specific evidence. |
| Portfolio PC02–PC05 | 0.11–0.12 ownership, generator and framework/native/distribution work | PC03 async-resource subset begins in 0.11. |
| Portfolio PC06 | 0.8 complete named inputs/caller scopes and budget boundaries | Broader portfolio/monorepo qualification completes in 0.12. |
| Portfolio PC07 | AS01–AS07, MP01–MP06, BR01–BR02 across 0.8–0.11 | CS06 integration in 0.12; CS07 audit in 0.13. |

A release's required subset is fixed before implementation in its model matrix. A deferred profile is explicitly unqualified; passing a related package, backend or start method does not qualify it.

## Reviewed library contracts

| Package | Deliverable | Acceptance |
| --- | --- | --- |
| LC01 | Normalize external facts and accepted provenance. | Inline publication, manifest, catalog, and project model produce consistent facts. |
| LC02 | Resolve actual providers and applicable artifacts. | Shadowing, aliases, version changes, patched files, and conflicts remain deterministic. |
| LC03 | Apply per-exit resource effects. | Factory, alias, cleanup, context, and response-state cases preserve independent duties. |
| LC04 | Standard-library file pack. | Real file conformance plus valid, invalid, and boundary fixtures. |
| LC05 | HTTPX and HTTPX2 synchronous packs. | Both actual libraries pass separate matrices and mixed-environment negative controls. |
| LC06 | Inspection commands and model-use ledger. | Clean reports explain model selection and failed applicability precisely. |
| LC07 | Async extension for both HTTP packs. | Direct awaits, async entry and exit, and cancellation paths are independently qualified; [AS07](concurrency.md#bounded-async-operation-sharing-and-as07) separately qualifies shared HTTPX and HTTPX2 requests and rejects close during known active use. |
| LC08 | Author tooling, incremental and distribution validation. | [E10-A](phases/carry-forward-gates.md#mandatory-e10-checkpoints) requires installed export/build/verify/consume evidence and artifact integrity in 0.8; E10-B requires installed artifact comparison in 0.9. Catalogs ship in actual artifacts; fresh and reused analysis agree; costs are recorded; E10-C editor parity completes in 0.12. |
| LC09 | SQLAlchemy synchronous foundation. | Separate ownership, transaction settlement, session reuse, result identity, and cleanup evidence; qualify exact artifacts and configurations. |
| LC10 | SQLModel synchronous adapter. | Own provider identity and exec adaptation pass independent conformance. The 0.9 ETLantic fixture consumes an exact-artifact reviewed external summary of session_scope for normal commit, Exception rollback and finally-close effects; generator-source verification and request_scoped_session framework lifetime checking remain LC11/PC03 in 0.11–0.12. |
| LC11 | SQL framework and generator extensions. | FastAPI dependency scope and PydanTable result/connection lifetimes are qualified separately. |
| LC12 | SQL asynchronous extension. | Direct awaits, sync-versus-async result families, awaited cleanup and cancellation pass their own gates. [AS07](concurrency.md#bounded-async-operation-sharing-and-as07) accepts per-task sessions and rejects known overlapping use of one AsyncSession even when lifetime and cleanup are correct; unknown sharing remains incomplete. |
| LC13 | First-party wrapper publication and consumer qualification. | Selected package artifacts ship their contracts; provider and downstream tests agree on ownership, retry, transaction, and outcome effects. |
| LC14 | Native-provider and shared-backing-resource models. | Actual native variants and APIs are qualified; stubs and context-manager syntax cannot invent cleanup effects. |

## Before-and-after change review

| Package | Deliverable | Acceptance |
| --- | --- | --- |
| CR01 | Comparison descriptor and normalized snapshot records. | Apply [revision 1 input and snapshot requirements](change-review.md#inputs-authority-and-snapshot-claims); source edits remain comparable while semantic-context changes are explicit. |
| CR02 | Policy, scope, assumption, and contract deltas. | Suppressions, exclusions, and provider changes cannot masquerade as source repairs; apply the [preservation prerequisites](change-review.md#preservation-pass-requirements). |
| CR03 | Conservative finding correspondence. | Line shifts match; duplicates and unsupported refactors remain ambiguous; ambiguous inheritance or repair cannot establish a preservation pass. |
| CR04 | Verdict policy and shared comparator. | Apply the [required verdicts and exit status](change-review.md#verdicts-and-exit-status); incomplete coverage, incomparable semantics and unresolved required correspondence cannot pass preservation; retain all nonpassing reasons. |
| CR05 | CLI and trusted-CI recipe. | Both reports use pinned semantics and a host-selected reference/policy; current-workspace claims validate the after snapshot; completed failed, review-required or inconclusive reviews exit 1. |
| CR06 | Incremental and integration qualification. | Full selected-scope checks and reused checks produce equivalent deltas, reasons and review conclusions under the shared comparator. |
| CR07 | Modernization and environment migration profiles. | Runtime ranges, analyzed targets, dependency artifacts, and changed agreements remain distinct from preserved-contract repairs; accepted migrations establish a new agreement rather than proof of unchanged contracts. |
| CR08 | Provider and native evidence on clean runs. | Artifact/model changes refresh affected callers and expose lost applicability even without a local source edit; missing required provider evidence prevents a preservation pass. |
| CR09 | Distribution scopes and budget comparability. | Root selection, sibling providers, caller closure, exclusions, and exhausted budgets remain visible in every verdict; all [required comparison controls](change-review.md#required-acceptance-cases) qualify before promotion. |

## Obligations that survive assignment

| Package | Deliverable | Acceptance |
| --- | --- | --- |
| OH01 | Freeze acknowledgment and API-boundary semantics. | Apply [revision 1 opt-in and production rules](persistent-outcomes.md#initial-declaration-and-compatibility-boundary): exact CodePact Result/Option callable returns with ordinary payloads; preserve unmarked outcomes and existing MustUse behavior; qualify variants, guards, forwarding/renewal and finally. |
| OH02 | Track tokens, aliases, temporaries, and exits. | Assignment, equality, overwrite, and deletion cannot erase or merge duties; unsupported external carriers produce an escape boundary instead of a false definite last-carrier finding; retain evaluation-order and compatibility controls. |
| OH03 | Apply finite-variant acknowledgment. | Each feasible path retains its correct pending variants; selected-arm acknowledgment follows successful pattern/guard selection, while effects established inside a guard survive its later false result or exception. |
| OH04 | Add helper summaries and qualified forwarding. | Argument evaluation, exceptional exits, aliases, outward contracts and finally preserve identity; forward one existing pending duty or create one outward duty at a successful marked return. |
| OH05 | Add reasoned discard and report events. | Waiver and suppression remain separate from acknowledgment and cleanup; existing immediate-use discard behavior is preserved. |
| OH06 | Qualify direct-await production and intratask cancellation. | Successful direct await can produce a duty; coroutine creation and cancelled-before-start cases cannot fabricate production or handling. |
| OH07 | Integrate provider effects and change review. | Contract removal, unknown helpers, and new waivers remain visible; all [required outcome controls](persistent-outcomes.md#required-acceptance-cases) qualify before promotion. |
| OH08 | Compose with TC03 resource payloads. | Matching preserves resource identity and never creates an implicit move or cleanup. |
| OH09 | Explicit custom-outcome adapters. | Custom receipts/reports require qualified identity, alternatives, acknowledgment, forwarding, and per-exit effects; database row results remain separate. |
| OH10 | Extended carriers for existing outcome obligations. | After PC02, selected field storage preserves one duty through reads, aliases, replacement, and supported methods; unknown mutation retains a boundary. |

## Portfolio modernization and adoption

| Package | Deliverable | Promotion gate |
| --- | --- | --- |
| PC01 | Modern annotations and package resolution. | Future/string annotations, relative imports, selected re-exports, and common declared class shapes resolve correctly or retain precise boundaries. |
| PC02 | Bounded field and cache ownership. | Created-versus-injected resources, aliasing, lending, replacement, eviction, failed cleanup, and renewed acquisition preserve responsibility. |
| PC03 | Lazy iterator and generator lifetimes. | Creation, first acquisition, suspension, full exhaustion, early abandonment, close, and delegated iteration have qualified effects. |
| PC04 | Framework ownership adapters. | FastAPI version/scope, application lifespans, and deferred Starlette cleanup have separately tested owner and completion boundaries. |
| PC05 | Distribution and native-provider qualification. | Installed wheels/manifests, native variants, optional dependencies, and dynamic compatibility layers select the intended reviewed evidence. |
| PC06 | Complete named scopes in monorepos. | Input and caller manifests, budget exhaustion, unresolved siblings, and scope growth remain visible; fresh and reused results agree. |
| PC07 | Task, thread, process lifetimes and bounded operation sharing. | Cancel/stop requests, actual termination, joined tasks, retryable cleanup and partial startup remain distinct. AS07 separately qualifies async operation permission and live-use cleanup through concrete provider profiles. |

## Portfolio rollout evidence

Record each selected package's installed distribution or complete named workspace scope, runtime support range, analyzed Python target, dependencies/native variants, modernization step, models, boundaries and adoption mode. A target/dependency upgrade changes the analysis context; it is not automatically a preserved-contract repair.

Before broad promotion, qualify a named async I/O pilot and a process-worker/ETL pilot, each with a useful defect/repair, correct control and unsupported-boundary control. Compare diagnostics, obligation identity, coverage and costs. ETLantic session helpers and PydanTable connection/result lifetimes remain independent LC10/LC11 evidence.

Candidate 0.11–0.12 corpus includes pytest-rxdist subprocess/shared-memory lifetimes, arraystream synchronous memoryview backing lifetimes, rapcsv native-worker/event-loop bridge completion, rapfiles native-handle cleanup evidence, and fullmetalcopy/pgcopyinsert SQL/COPY composition. These are model and adoption candidates, not claims of current coverage or confirmed defects. Distinguish arraystream's synchronous view lifetime from asyncio streams and pytest-rxdist's subprocess model from multiprocessing.Process.

Selected first-party packages publish contracts under LC13 only after wrapper behavior qualifies. A successful check of one package or backend does not qualify the entire portfolio.

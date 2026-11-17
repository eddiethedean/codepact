# Before-and-after change review

Specification revision: **1**. Status: **Planned; acceptance pending**.
CR01 descriptor decisions are required in 0.8. The bounded CR01–CR05 and
CR07–CR09 workflow and CR06 CLI/in-memory checkpoint belong to
[0.9](phases/0.9-obligations-and-change-review.md); cache/editor/MCP parity
completes in 0.12. Command names and schemas remain proposed.

## Inputs, authority and snapshot claims

Use **reference snapshot** for the before program; **baseline** retains its
existing meaning as finding-suppression input. Each report identifies the
analyzed source bytes, selected roots/files, normalized applicable contracts,
effective policy and origins, suppression inputs, engine/rule semantics,
Python target, selected provider artifacts, models/assumptions, findings,
coverage and relevant budgets.

Source edits are expected. A separate versioned comparison descriptor records
checker/rule semantics, target, experimental semantics, model capabilities,
normalization and comparison/matching algorithms. Source hashes remain in
snapshot identity; do not reinterpret existing fingerprints without a schema
and algorithm revision. The first implementation uses the same qualified
checker semantics on both sides; rerun both versions with one checker when
necessary before attributing a difference to a source patch.

Locally the developer selects the reference. In CI the trusted host workflow
selects the base revision and policy and generates reports with pinned
semantics. A report's hash or self-declared trusted field supplies no approval
authority. Capture immutable inputs or verify that a dirty workspace's relevant
inputs remained unchanged during analysis. Retain provider files actually used.

The pure comparator describes the two captured analyses. A claim about the
current workspace additionally requires matching the after snapshot to current
source, configuration and provider inputs. A valid historical comparison may
remain useful without that validation, but cannot claim a current-workspace pass.

## Independent changes and finding correspondence

Report policy, scope, contracts, assumptions, findings and coverage separately.
Compare effective configuration rather than formatting. Provider upgrades,
HTTPX-to-HTTPX2 migration, target changes and new waivers remain explicit even
when a local source diff is clean. Reuse normalized public-contract delta
semantics from the [artifact comparator](published-contracts.md).

Finding matching prefers a unique exact identity, then a unique condition in
the same resolved declaration/contract subject, then a verified identity-
preserving relocation. Similarity, ordinal position, line numbers and Git
rename hints can suggest correspondence for presentation but cannot prove it.
Duplicates and unsupported refactors remain ambiguous.

Retain operation/obligation correspondence anchors even at clean sites. Missing
after diagnostics alone cannot establish an after counterpart. A finding is
`resolved-under-preserved-contracts` only when completed analyses supply the
required evidence, relevant semantics and target are comparable, the operation
and obligation match unambiguously, applicable contracts/assumptions remain
preserved, required coverage is established and the raw finding is absent.
Suppression, weaker rules, exclusion or removed duties cannot establish repair.
Deleted functionality is classified as code removed for project review.

## Preservation pass requirements

The initial `preserve` policy requires all of the following:

1. Comparable qualified checker semantics and a stated target valid for both
   source versions, with the supported comparison evidence available.
2. Preserved relevant policy, contracts and applicable assumptions.
3. No lost analysis scope, including affected in-scope callers and providers.
4. Complete supported coverage in the deliberately selected scope on both
   sides; unknown, truncated or exhausted required evidence prevents a pass.
5. No newly disallowed findings or worsened disallowed conditions.
6. Any explicitly requested repair targets are resolved through supported,
   unambiguous correspondence and the preserved-contract conditions above.

Recheck the full selected root; changed-lines filtering may limit presentation
but cannot reduce semantic inputs. Name caller closure, siblings, exclusions
and budgets without claiming unknown downstream consumers were checked.

An unchanged inherited violation may remain: a regression review can pass
while the standalone after check fails on that inherited finding. Report this
distinction. Inheritance itself requires unambiguous supported correspondence;
ambiguity about whether a disallowed after finding is inherited, new or worsened
makes the preservation conclusion inconclusive. Ambiguous repair-target matches
cannot satisfy a requested repair condition.

## Verdicts and exit status

Facts remain independent of review policy. Retain all blocking reasons even
when a single verdict is selected.

| Verdict | Meaning | Review exit |
| --- | --- | --- |
| `passed` | The selected policy is satisfied within the stated scope and claim. | `0` |
| `failed` | A conclusive gate condition failed, such as a newly disallowed violation. | `1` |
| `review-required` | A known contract, policy, scope, waiver or assumption change needs deliberate project acceptance. | `1` |
| `inconclusive` | Evidence cannot establish the requested conclusion, including insufficient coverage or comparability. | `1` |
| Input/execution failure | Invalid input, unsupported schema or execution failure prevents comparison. | `2` |

A known disallowed violation can establish failure despite unrelated
uncertainty. Otherwise insufficient comparability or coverage prevents a
preservation pass; known changes and all nonpassing reasons remain visible.
Completing or rendering a comparison does not itself produce a passing review.
A supported report with truncated required evidence is inconclusive, whereas
an invalid schema is an input failure. Engine mismatch can produce insufficient
comparability and guidance to rerun both sides.

A completed `check` report with exit `1` remains a valid comparison input;
automation must retain a before report containing the violation to repair.
A check execution failure does not supply a completed analysis.

## Intentional migrations

Record supported Python runtime ranges separately from the analyzed target and
conformance runtime, alongside actual dependency artifacts, extras, platform/
native variants and driver/backend configuration. Unchanged constraints do not
prove the environment is unchanged. If no common qualified target exists,
retain separate checks and report insufficient preservation comparability.

Compare an intentional migration's agreements and affected callers explicitly.
A trusted workflow may record deliberate acceptance and establish a new
reference; it cannot turn changed-agreement evidence into an unchanged-contract
repair. The comparator does not infer acceptance or offer an accept-all shortcut.

## Required acceptance cases

| Work | Required controls |
| --- | --- |
| CR01/CR05 snapshots | Pinned host-selected reference and policy; valid historical comparison; current-workspace pass only with a matching after snapshot; stale/currently changed inputs cannot receive that claim. |
| CR02/CR04 policy | Suppression, weakened rule, removed contract, excluded code and provider migration remain distinct from repair and require the appropriate nonpassing preservation verdict. |
| CR03 correspondence | Clean line shifts and established relocations match; duplicate sites, ambiguous inheritance and ambiguous repair targets cannot establish preservation or repair. |
| CR04 coverage | Definite violation becoming unknown is not resolved; truncated evidence, lost applicability, lost scope and exhausted required budgets cannot produce a preservation pass. |
| CR04 verdicts | Exercise passing, failed, review-required and inconclusive results with all reasons retained; a conclusive failure survives unrelated uncertainty; inherited violations remain distinguishable from regressions. |
| CR05 execution | Completed nonpassing reviews exit `1`; invalid input/unsupported schema/execution failure exits `2`; completed passing reviews alone exit `0`. |
| CR06 parity | Full selected-scope and reused CLI analysis produce equal deltas, reasons and verdicts; later cache/editor/MCP paths use the same comparator. |
| CR07 semantics | Changed checker or target semantics cannot establish preservation without qualified comparability; migration acceptance never relabels changed agreements as preserved repair. |
| CR08/CR09 environment and scope | Provider/native changes refresh affected callers on clean source runs; named inputs, unresolved siblings, scope changes and budget exhaustion remain visible. |

Retain valid, invalid and unsupported controls under the exact candidate,
schema/algorithm versions and model matrix in release evidence. These cases
are required by the [review workstreams](contract-and-portfolio-workstreams.md).

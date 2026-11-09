# 0.4 pilot and onboarding protocol

Status: **AD01 accepted this protocol and the initial pinned project set on 2026-11-09. Partial scans of the initial projects and all preregistered expansion candidates are recorded in the [pilot results](pilot-results-2026-11-09.md); no findings have been labeled and A05 remains pending.** This protocol defines the evidence needed for [A05/A06/CS02](../0.4-adoption.md), not a promise that every Python project is analyzable.

The project inventory and frozen initial sample settings are recorded in the
[2026-11-09 pilot preregistration](pilot-inventory.md). Its selection is
distinct from the partial scan results; any labels must follow the
preregistered procedure.

## Selection before evaluation

Select at least three real projects at pinned revisions before examining the acceptance sample. Include a synchronous library, a service or CLI application, and at least one async application with resource cleanup/cancellation paths. These roles may overlap, but there must be three distinct projects. Record why each is representative, its size, layout, Python target, resource patterns and ordinary CI tools. Choose against a written candidate inventory rather than only repositories already known to produce good results.

Pilot access is not a distribution or model-support claim. Public projects are preferable for reproducible evidence; for private code retain an auditable internal artifact and a shareable aggregate/authorized minimal reproduction. Do not upload private source as part of running CodePact. Analyze source as data; never import the project or run its setup hooks through the analyzer.

Freeze the candidate checker revision, selected stable rules, severity/mode, target/models, file scope and sampling method before scoring. Archive effective settings, disabled rules, excluded paths, baseline and inline suppression counts, source/content hashes and output identities. Record existing declarations and any explicitly prepared contract/model annotations. Annotation preparation must reflect the project's actual API, remain reviewable and be distinguished from analyzer-inferred facts. New third-party models require their own reviewed scope amendment; a pilot does not silently qualify them.

## Evaluation runs

Run on the unmodified pinned source first. Retain full canonical findings before baseline/inline hiding, policy dispositions, coverage reasons and source scope. Advisory/strict runs should agree on canonical findings; record their different failure reasons/exits. Optional integration patches and baselines are separate adoption experiments and cannot erase the original observations.

If a pilot needs a checker fix or annotation correction, retain the before result and rationale. Freeze a new candidate and rerun affected pilots and the selection procedure consistently. Do not cherry-pick earlier good findings and later fixed findings into one apparent candidate score. Retain unsupported syntax, unavailable dependencies and incomplete coverage as separate boundary counts.

For the async application, distinguish intratask cleanup findings from TaskGroup/detached-task/process/thread boundaries. Record available creation/capture, resource duty, close/return, await/cancellation and escape evidence. Do not label an unknown handoff as a confirmed task-lifetime violation or require a CS03 repair in 0.4.

## Sample and labels

The acceptance population is all distinct canonical **definite violation findings** from the selected stable TP1xxx/TP2xxx/TP3xxx rules in the recorded pilot scope, including findings later acknowledged by baselines or inline directives. Severity overrides and thresholds do not change membership. TP4xxx source errors, possible violations and TP9xxx boundaries are reviewed separately; they cannot be counted toward the 30-finding minimum. Use the analyzer's documented exit/site grouping, not multiple display copies or repeated runs, to define distinct findings.

Use a census when practical. Otherwise use a reproducible stratified sample of at least 30 findings, with fixed seed/algorithm and strata by project and violation rule. Give every project and every represented rule a nonzero allocation when that population exists; record population and sample sizes and the allocation algorithm before drawing. Do not replace hard-to-label findings or add hand-written/injected violations to meet the acceptance minimum. If real findings are too few, expand the preregistered pilot set, rerun and disclose the change; otherwise A05 remains pending.

Label every sampled finding using source, declaration/model assumptions and related evidence:

| Label | Criterion | Counts as actionable? |
| --- | --- | --- |
| Actionable | A real supported contract violation with correct location/explanation, for which a justified source correction or correction of an erroneous contract is warranted | Yes |
| False positive | The claimed supported violation is absent, or its certainty/evidence overstates the model | No; triage correctness severity separately |
| Intentional / policy mismatch | The behavior is deliberate and the justified response is only acknowledgment, suppression or excluding the rule | No |
| Duplicate / unusable | A redundant finding or explanation/location defect prevents useful action | No |
| Unresolved / disputed | Available evidence or reviewer judgment does not establish actionability | No until resolved with recorded reasoning |

A cleanup declaration does not prove runtime effects; actionability is assessed against the explicit supported contract. Record rationale, smallest useful source excerpt/reference, contract provenance, issue link and any disagreement for every label. The accountable maintainer may label and resolve all findings alone; no second reviewer is required. If additional reviewers participate, retain their original labels and resolution rather than silently overwriting disagreement.

Pass A05 only when `actionable_count / sampled_definite_violation_count >= 0.90`, with denominator at least 30, three reviewed projects and the required disclosures. Use the actual sample denominator without rounding up the percentage. This is a bounded triage-quality target, not statistical proof of general precision or a recall measure. Publish per-project/per-rule counts alongside the overall fraction; disclose strata/selection bias and do not extrapolate to unknown code. A supported-scope correctness blocker holds A06 even if the aggregate ratio passes.

Separately review possible findings and unsupported boundaries for clarity and false-clean risks. Use controlled valid/invalid/unknown fixtures to detect missed findings, but do not mix synthetic fixture outcomes into the pilot ratio. Preserve findings from disabled/experimental-rule exploratory runs separately from the frozen default-rule acceptance sample.

## Clean onboarding and repair

Reproduce this walkthrough in a clean environment using exact candidate artifacts. Retain OS/interpreter/checker/package versions, install route, commands, exits, reports and source diffs:

1. Install the matching helper/CLI and confirm version/capabilities. Resolve project root, source target and effective configuration without project execution.
2. Run a useful check, save JSON even on exit 1, and explain one supported violation and one remaining boundary. Verify source hashes and rule documentation.
3. Make the smallest justified source/contract correction and rerun with the same policy, target and model settings, including affected dependents. Record cleared/new findings, coverage changes and relevant existing project-test results. Running project tests is a separate explicitly chosen development step, not analyzer behavior.
4. Demonstrate that adding a suppression/baseline, expanding exclusions or weakening rules/threshold is a policy change and fails the repair demonstration. Show one unresolved case with an explanation instead of forcing a fix.
5. Generate a baseline through the explicit update command, inspect its entries/diff, and show that an additional real/controlled new finding is visible. Keep this controlled demonstration separate from pilot scoring.
6. Exercise advisory/strict CI handling for exit 0, finding exit 1 and execution/configuration failure 2. Reproduce the external-tool recipe and opt-in agent instruction snippet without installer/checker edits to AGENTS.md or project policy.

Do not recommend automatic close insertion, cancellation suppression, shielding, task joins or ownership moves without established behavioral preconditions. The first supported repair can use an inherited lifecycle violation or the bounded synchronous must-use rule. Neither a zero exit nor unchanged source with a weaker policy establishes a repair.

## Scorecard and retained evidence

Create one scorecard per pilot and one aggregate record with:

- Project identity/revision/license/access, selection rationale, source size/layout, source/target/model settings, annotation preparation and artifact hashes.
- Selected/disabled rules and scope/exclusions, canonical findings by code/certainty, suppressed counts, coverage reasons and unsupported async/process patterns.
- Acceptance population/sample counts, method/seed/strata, complete labels/rationales/disagreements, exact ratio and issue dispositions.
- Check–edit–recheck reports/source diffs, policy identities, remaining boundaries, tests, clean installation/CI recipes and first-use friction.
- Cold/warm/edit timing, output size and memory method under the [operational plan](validation.md#operational-qualification), with maintainer/date and reproducible artifact locations.

Link scorecards to the exact-candidate release record and V04 ledger. Missing projects, missing labels, fewer than 30 eligible findings, less than 90% actionable or an untriaged supported-scope blocker holds the corresponding gate. Scope/rule changes require renewed evaluation; retain old results as history instead of presenting them as current evidence.

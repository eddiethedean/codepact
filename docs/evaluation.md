<!-- release-recipes -->

# Evaluate CodePact for your team

Use CodePact 0.10.0 for a scoped trial of annotated Python resources. Evaluate
the default lifecycle and must-use checks first. Try the opt-in ownership
and [reviewed model profiles](reviewed-contracts.md) on a known subset.

| Evaluation question | Evidence or action |
| --- | --- |
| What runs locally? | The native analyzer parses selected Python source. The Python helpers carry typed metadata. Review [data handling and artifact contents](security.md). |
| Can we install it on our runners? | Check [wheel and runtime support](availability.md#platform-and-runtime-evidence). The Python installer requires 3.11 or later. Source grammar targets are 3.11–3.13. |
| Can we verify the release? | Use the [artifact and offline installation procedure](release-verification.md#verify-and-install-wheels) and the [platform matrix](availability.md#platform-and-runtime-evidence). |
| What compatibility should we plan for? | Pin the CLI and helpers together and validate your report schema. APIs, diagnostics, and schemas remain provisional during 0.x. Review [upgrades](upgrading.md) and the [support terms](support.md). |
| What should we expect at scale? | The [performance guide](performance.md) summarizes controlled workloads and limits. Measure your own representative root, cold CI run time, and coverage before setting a gate. |
| How does it fit our existing checks? | Keep type checks, linting, and tests. Use CodePact to check supported flow through declared cleanup and result-use contracts. Compare against the [known bug and repair cases](rules.md#run-a-bug-and-its-repair). |

## Run a pilot

Use the [bug, repair, and boundary controls](pilot.md) to verify installation.
Then follow the [existing-project walkthrough](adopting.md#evaluate-one-package)
on one owned package.

| Record | Decide |
| --- | --- |
| Exact CLI and artifact, runner, source target, and resolved settings | Whether you can reproduce the same check in CI. |
| Selected files, exclusions, and recurring coverage reasons | Which code the gate can cover and which boundaries the owner accepts. |
| Confirmed findings, reviewed exceptions, and annotation and triage time | Whether the checker adds enough value to expand the pilot. |
| Repeated cold CI wall time and report size | Whether the job meets the team's runner and retention constraints. |
| Report recipients, retention, and escalation owner | Whether support and privacy requirements are met. Review [data handling](security.md) and [maintenance terms](support.md). |

Keep advisory mode while the scope and exceptions are under review.
Promote the check to a required gate when a team owner can reproduce the
result, accepts the remaining coverage boundaries and warning threshold, and
has a process for reviewing baseline changes. Published
[synthetic qualification](synthetic-projects.md) establishes controlled
behavior. The pilot establishes usefulness for your project.

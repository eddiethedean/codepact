# Evaluate CodePact for your team

Use CodePact 0.9.0 for a scoped trial of annotated Python resources. Evaluate
the default lifecycle and must-use checks first. Try the opt-in ownership
and [reviewed model profiles](reviewed-contracts.md) on a known subset.

| Evaluation question | Evidence or action |
| --- | --- |
| What runs locally? | The native analyzer parses selected Python source. The Python helpers carry typed metadata. Review [data handling and artifact contents](security.md). |
| Can we install it on our runners? | Check [wheel and runtime support](availability.md#platform-and-runtime-evidence). The Python installer requires 3.11 or later. Source grammar targets are 3.11–3.13. |
| Can we verify the release? | Check the artifact for your runner against the registry digest and review the [platform and runtime evidence](availability.md#platform-and-runtime-evidence). |
| What compatibility should we plan for? | Pin the CLI and helpers together and validate your report schema. APIs, diagnostics, and schemas remain provisional during 0.x. Review [upgrades](upgrading.md) and the [support terms](support.md). |
| What should we expect at scale? | The [performance guide](performance.md) summarizes controlled workloads and limits. Measure your own representative root, cold CI run time, and coverage before setting a gate. |
| How does it fit our existing checks? | Keep type checks, linting, and tests. Use CodePact to check supported flow through declared cleanup and result-use contracts. Compare against the [known bug and repair cases](rules.md#run-a-bug-and-its-repair). |

## Run a pilot

Follow the [existing-project walkthrough](adopting.md#evaluate-one-package).
For the pilot record, include the exact CLI version and settings, selected
source files, time spent adding contracts, confirmed findings, common coverage
reasons, and CI wall time. Review exclusions and baseline entries with the
package owners. Start enforcing policy when the useful findings and scope
justify the ongoing annotation and triage work.

Use synthetic bug, repair, and boundary cases to check the installation.
Measure usefulness in your own project during the pilot.
[Synthetic corpus and qualification](synthetic-projects.md)

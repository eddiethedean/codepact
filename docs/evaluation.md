# Evaluate CodePact for your team

CodePact 0.5.1 is suitable for a scoped trial of annotated Python resources.
Evaluate the default lifecycle and must-use checks first. Treat ownership and
TaskGroup behavior as opt-in preview scope.

| Evaluation question | Evidence or action |
| --- | --- |
| What runs locally? | The native analyzer parses selected Python source. The Python helpers carry typed metadata. Review [data handling and artifact contents](security.md). |
| Can we install it on our runners? | Check [wheel and runtime support](availability.md#platform-and-runtime-evidence). The Python installer requires 3.11 or later. Source grammar targets are 3.11–3.13. |
| Can we verify the release? | The tag workflow validated the release jobs and artifact set. Use the [0.5.1 publication record and digests](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.5.1-evidence.md#publication-record) to verify downloaded distributions. |
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

The release was qualified with synthetic bug, repair, and boundary cases.
Production-project usefulness still needs to be measured in each pilot.
[Synthetic corpus and qualification](synthetic-projects.md)

# Configuration and capabilities

Use `codepact check --show-settings --json` to see the effective policy
and each setting's origin. Use `codepact capabilities --json` to see which
rules, options, and targets the installed binary actually supports.
The [CLI reference](cli.md#project-configuration-and-scope) defines precedence;
the [first-package pilot](pilot.md) provides a runnable starter policy.

## Check policy

Add these keys inside `[tool.codepact]` in `pyproject.toml`. Unknown keys and
invalid values fail with exit `2`.

| Key | Default | Effect |
| --- | --- | --- |
| `config-version` | Required when the table is present | Set to integer `1`. |
| `root` | Directory of a loaded CodePact table, otherwise the working directory | Scope for project discovery and logical module names. A configured path is relative to its configuration file. |
| `python-version` | `"3.11"` | Analyze source targeting `"3.11"`, `"3.12"`, or `"3.13"`. [Compatibility and fit](compatibility.md) |
| `include`, `exclude` | `["**/*.py"]`, `[]` | Match root-relative paths with `/`. Exclusions take precedence. |
| `enabled-rules` | Rules in the selected scope | An explicit list replaces the configurable rule selection. Mandatory diagnostics continue to report. |
| `mode` | `"advisory"` | `"strict"` also fails on incomplete coverage and active possible findings. |
| `fail-on` | `"error"` | Set `"warning"` to gate active warnings too. |
| `baseline` | Unset | Path to an explicitly created baseline, relative to the configuration file. |
| `severity` table | Empty | Override eligible rules with `"error"`, `"warning"`, or `"info"`. Certainty stays independent. |

For example, place `TP3001 = "error"` inside `[tool.codepact.severity]` to
gate definite discarded must-use results at the default threshold. The
rule is a warning without that override. Use the
[policy and exits table](cli.md#policy-and-exits) to decide how to gate
possible findings and incomplete coverage.

## Feature switches

“Available since” identifies the first release containing the feature. The
[rule catalog](rules.md) lists the corresponding diagnostic codes, first
versions, and default selection. Choose the mode only after confirming the
feature fits your project.

| Capability | Available since | Opt-in and applicability |
| --- | --- | --- |
| Lifecycle and synchronous must-use checks | Lifecycle 0.2, must-use 0.4 | Default scope. Annotate supported resources and synchronous result-use contracts. |
| Ownership, explicit moves, and selected TaskGroup effects | 0.5 | `experimental-ownership = true`. TaskGroup also needs the applicable default-task-factory assumption. |
| Exclusive access and finite state protocols | 0.6 | `experimental-advanced-contracts = true` with ownership enabled. See [advanced contracts](ownership.md#advanced-contract-preview-06). |
| Finite outcome matches | 0.7 | Enable the opt-in with `experimental-finite-matches = true`. |
| Installed providers | Inline 0.7, manifests and reviewed models 0.8 | Set `roots` under `[tool.codepact.dependencies]` or pass `--dependency-root`. Installed artifacts must fit the selected provider profile. |
| Reviewed standard-library effects | 0.8 foundations | Set `[tool.codepact.runtime-profile]` to the actual modeled target. Apply ownership, advanced contracts, and runtime assumptions per model. |
| Persistent outcomes | 0.9 | Default rule selection. A callable or provider opts in through a supported `MustHandle` contract. |
| Work-result observation | 0.9 | Applicable runtime and ownership profile. Task, future, or worker completion and result observation are independent. |
| Declaration policy | 0.9 | Add `[[tool.codepact.declaration-policy]]` entries. Each selects a dotted `module`, optional `function`, a `requirement`, and `mode` of `preview` or `enforce`. See [policy selectors](adopting.md#require-a-declaration-on-a-selected-api). |
| Queues, IPC, and guards | 0.10 | Applicable [0.10 runtime profile](concurrency.md). The `async-guards` table and `spawn-main-script` setting require 0.10. |

Ownership, advanced contracts, finite matches, the default-task-factory
assumption, and explicitly requesting foundations are off when unset.
`experimental-foundations = true` requests the foundations report even
when you have no provider or runtime inputs.

## Model inputs

| Table | Keys | Scope |
| --- | --- | --- |
| `[tool.codepact.dependencies]` | `roots`, `required-providers`, `project-contracts`, `uv-lock` | Ordered installed package roots, required distribution names, project-model files, and an optional compatible lockfile. Configured paths are relative to the config file. See [installed providers](consume-providers.md). |
| `[tool.codepact.runtime-profile]` | `implementation`, `python-version`, `os`, `architecture`, `event-loop`, `start-method`, `assume-standard-process-wait` | The first four keys identify the analyzed runtime, separately from the source target. Loop and start method settings select applicable effects. The wait assumption defaults to false. See [runtime models](runtime-models.md). |
| `[tool.codepact.runtime-profile]`, 0.10+ | `spawn-main-script` | A normalized `.py` path relative to the analysis root, such as `main.py`. Qualifies the direct script passed to Python for explicit spawn. |
| `[[tool.codepact.async-guards]]`, 0.10+ | `module`, `coroutine`, `resource`, `lock`, `method` | A statically resolved top-level coroutine, local allocations for the protected resource and exact lock, and a source-resolved method. See [guard examples](concurrency.md#guard-a-resource-method). |

Source-only checks and foundations checks expose different report versions.
Use the [schema table](schemas.md) and read model applicability in
`foundations.model_uses` before relying on an effect.

<!-- release-recipes -->

# Compatibility and fit

Check these versions separately before installing or making a CI check required.

| Version | CodePact 0.9.0 | Where to set or verify it |
| --- | --- | --- |
| Python that installs the wheel or runs the helper package | Python 3.11 or later. Helper and installer checks run on CPython 3.11–3.14. | Run `python --version` in the installation environment. The native CLI itself runs without importing your application. |
| Python source target | 3.11, 3.12, or 3.13. Defaults to 3.11. | Set `python-version` in `[tool.codepact]` or pass `--python-version`. Verify it with `check --show-settings --json`. |
| Runtime modeled for selected standard-library effects | CPython 3.13.16 on Linux x86-64 with source target 3.13 and the assumptions for the selected model. | Set a truthful [runtime profile](runtime-models.md) for the application. The host running the checker can be different. |

The [platform matrix](availability.md#platform-and-runtime-evidence) lists the
wheel minimums. Select the source target that matches the code you want to
analyze. On an application that requires Python 3.14 syntax, plan the pilot
around a package written for one of the three supported source targets.

## Common project patterns

| Pattern | `check` | `inspect-contracts` |
| --- | --- | --- |
| Direct declarations with a local absolute import | Supply the defining and consuming modules under the selected root and include policy. | Name both files and set the logical root. |
| Relative or star imports, supplied-module cycles, string or postponed contract annotations | These can leave coverage incomplete. Read the specific reason instead of interpreting green advisory output as coverage. | These can leave declaration facts unresolved. |
| Named re-exports | Selected statically resolved re-exports work. | Re-export chains can remain unresolved. |
| Installed providers | Configure explicit dependency roots and confirm applicability in the foundations report. | Supply declaration source files directly. |
| Resource aliases in fields or containers, dynamic dispatch, custom allocation | Effects can become unknown. Start the pilot at a simple annotated API boundary. | Declaration facts alone give no evidence about body effects. |

Use [supported syntax](supported-syntax.md) for the exact forms and
[limitations](limitations.md) for the supported flow and budgets. If your
first report is incomplete, diagnose the reason or try a narrower supported
package before enabling strict mode. [Run a first-package pilot](pilot.md)

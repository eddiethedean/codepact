# 0.4 pilot project inventory and preregistration

Status: **The initial project set, candidate revision, rules, scope and sampling
method were frozen on 2026-11-09 before their analysis.** A dated amendment below
adds all preregistered expansion candidates after two completed initial scans
returned no eligible findings and a third failed before analysis. This is not a
pilot result or A05 pass.

## Frozen CodePact evaluation

| Setting | Frozen value |
| --- | --- |
| Candidate revision | `620656b8a8a28ac57a29e589164dc9b58c8141d8` |
| Source target | Python 3.11 for every repository |
| Effective project settings | No project config; `--no-config`; root is the pinned checkout root |
| Rules | `TP1001`, `TP1002`, `TP1003`, `TP2001`, `TP3001` (all default stable rules) |
| Policy | Advisory mode; fail-on error; no baseline; no custom suppressions, excludes or severity overrides |
| File scope | Full default discovery of Python source under the checkout; retain built-in directory skips and report their scope effects |
| Execution | Read source as data. Do not install project dependencies, import project code, run project hooks or execute project tests as part of analysis. |
| Annotation preparation | None before the first scan. Any later annotation preparation must be a separate, source-attributed amendment reflecting documented project contracts; it invalidates this preregistration and requires a frozen rerun before scoring. |

`620656b` is the CodePact source revision tested by the green [Quality run
42656145988](https://github.com/eddiethedean/codepact/actions/runs/42656145988).
The selected rule set is taken from the candidate's default registry. The pilot
uses the checker without repository-specific policy so results are comparable.

## Candidate inventory and selection

The inventory was assembled from repository metadata, primary project
descriptions, language, active status, license and pinned default-branch
revisions. No CodePact result, finding count or source-level acceptance sample
was examined when selecting the initial set.

| Repository and pinned revision | Role / selection reason | Selection |
| --- | --- | --- |
| [psf/requests](https://github.com/psf/requests/commit/3c7baaa05ab4aa49cc819ce3a819d260de19c157), Apache-2.0, Python | Established synchronous HTTP library; represents the synchronous library role. | **Selected** |
| [pypa/pip](https://github.com/pypa/pip/commit/4b71f36fb4dfbc87ce65ac4710eeccf81d379f55), MIT, Python | Python package installer with a user-facing command-line workflow; represents the CLI application role. | **Selected** |
| [Kludex/uvicorn](https://github.com/Kludex/uvicorn/commit/a99c1b76ee9caea67ff98e2ccc457bd372db193f), BSD-3-Clause, Python | ASGI web server; represents the asynchronous server/application role and cancellation/cleanup boundaries. | **Selected** |
| [pallets/flask](https://github.com/pallets/flask/commit/49943b5a94413a530761350f04e896f76e8d620e), BSD-3-Clause, Python | Synchronous web application framework; inventory alternative for service-oriented patterns. | Expansion candidate 1 |
| [pallets/click](https://github.com/pallets/click/commit/8abb77a8432e03ed14a95f972b44f905279415a7), BSD-3-Clause, Python | Command-line toolkit; inventory alternative for CLI patterns. | Expansion candidate 2 |
| [fastapi/fastapi](https://github.com/fastapi/fastapi/commit/534961812bef29ad80d2f7c399e52c688e8a0d57), MIT, Python | Asynchronous web framework; inventory alternative for async application patterns. | Expansion candidate 3 |

Keep the first three selected repositories in the evaluation regardless of
results. If their full definite-finding population is below 30, add **all three**
expansion candidates in the listed order before collecting labels; do not select
only repositories with more findings. If the expanded population is still too
small, A05 remains pending and the inventory must be extended before another
frozen run.

## Frozen labeling and sampling procedure

1. Analyze the full frozen source scope once per repository and retain raw
   reports, effective settings, coverage, source tree SHA, report SHA-256 and
   analyzer command/output. Do not edit the source tree during this run.
2. The eligible population is every distinct canonical **definite violation**
   finding from `TP1001`, `TP1002`, `TP1003`, `TP2001` and `TP3001`. Include
   findings even if an upstream source comment or policy would acknowledge
   them. Do not count possible findings, TP4xxx declaration errors, TP9xxx
   boundaries, duplicate display locations, repeated runs or synthetic
   fixtures.
3. Label the full population when it contains at most 300 findings. Above 300,
   retain the full population counts and draw a 30-finding stratified sample.
   Strata are repository × rule among nonempty strata. Reserve one finding for
   each nonempty stratum, then allocate remaining slots proportional to
   remaining stratum population (`population - 1`) using the largest-remainder
   method, capped at each stratum's remaining population and iteratively
   redistributing any excess; ties are broken by repository name then rule
   code. Sort strata by repository and rule. Within each stratum, sort canonical
   `repository|rule|fingerprint` keys and shuffle them using one Python 3.11
   `random.Random(20261109)` instance shared across sorted strata. No finding
   may be replaced because it is difficult to label.
4. The accountable maintainer labels each finding actionable or not actionable
   against the source, declaration/model assumptions, resolved evidence and
   the accepted support boundary. Preserve the complete label, rationale,
   source reference, provenance, uncertainty and any later correction. Label
   possible findings and unsupported boundaries separately.
5. Report the unrounded ratio, denominator, per-repository/per-rule counts,
   coverage and disagreements. A05 passes only with at least three reviewed
   projects, at least 30 eligible definite findings, at least 90% actionable,
   and no untriaged supported-scope blocker. No denominator change or threshold
   reduction is allowed after observing labels.

## Pre-analysis project facts to retain

The following facts were recorded from the pinned metadata, README, project
documentation, test configuration and CI workflow files before the first
CodePact scan. They characterize the initial frozen sample; they cannot be used
to replace a project after seeing findings.

| Project snapshot | Python support and source layout | Documented resource / async patterns | Test and CI route; explicit analysis boundaries |
| --- | --- | --- | --- |
| Requests, commit `3c7baaa05ab4aa49cc819ce3a819d260de19c157`, tree `0c8b989695d73c087123936657c0fbfe3034471f` | `>=3.10`; `src/requests`: 19 Python files / 216,088 bytes; `tests`: 15 / 173,837 bytes. | Documentation says `Session` is a context manager and closes on exit; streamed `Response` objects require consumption or explicit close and recommends `with` for guaranteed closure. Synchronous project. | Tox runs `pytest tests` across configured Python 3.10–3.14 environments. Runtime dependencies include charset-normalizer, idna, urllib3 and certifi. Their installed package sources are outside this scan; tests and project setup are not run. |
| pip, commit `4b71f36fb4dfbc87ce65ac4710eeccf81d379f55`, tree `9eb70d44014780ec918e709c8dea122da2466c98` | `>=3.10`; `src/pip`: 441 Python files / 4,993,145 bytes; `tests`: 250 / 1,847,580 bytes; `tools`: 5 / 14,402 bytes. | Documentation describes package download, filesystem and archive workflows. The reviewed developer docs do not establish a project async resource-lifetime contract. Package/process/network interactions may depend on external tools and are not executed or modeled as supported by this pilot. | CI uses Nox/pytest for `tests/unit` and `tests/functional`; Python and platform matrices vary in its workflow. External dependencies and vendored libraries are not qualified independently. Nox, tests, package installs and network operations are not run. |
| Uvicorn, commit `a99c1b76ee9caea67ff98e2ccc457bd372db193f`, tree `8fbdd8d45f2ad93f09b5524237d769a524cee086` | `>=3.11`; `uvicorn`: 45 Python files / 279,829 bytes; `tests`: 39 / 349,742 bytes. | Documentation describes ASGI lifespan startup/shutdown resource setup, a sibling lifespan task, waiting for pending requests/background tasks at shutdown, connection limits and cancellation-related behavior. TaskGroup/detached-task ownership remains outside CodePact's accepted scope. | CI covers Python 3.11–3.15 and Windows/Linux/macOS using `scripts/install`, `scripts/check`, `scripts/build`, `scripts/test` and `scripts/coverage`. Runtime dependencies include click and h11; optional protocol, event-loop and watcher dependencies remain external boundaries. No project scripts, tests or dependencies are run. |

The initial source checkouts were clean at these exact revisions. The selected
target is Python 3.11 for all three. No CodePact output had been examined when
this initial set was frozen. The individual scorecards and raw reports belong
in the 0.4 pilot evidence folder and must link back to this preregistration and
the exact candidate revision.

## Expansion amendment — 2026-11-09, before expansion scans

The two completed initial reports (Requests and Uvicorn) contained zero
definite findings from the five selected stable rules. The pip invocation
failed before analyzing files: full default discovery included
`build-project/build-project.py`, but the candidate requires every path
component to be a Python identifier when deriving a module name. Therefore the
initial eligible population is **not known** and the preregistered
`population < 30` condition cannot be asserted. To avoid labeling or selecting
from an incomplete population, this amendment includes **all three**
alternatives already listed above before any scan of those alternatives and
retains every initial project and result. The reason for this nonselective
expansion is the incomplete initial analysis; it does not establish A05 or
qualify the path-mapping behavior. Reports from the original candidate remain
separate from any future candidate rerun.

| Project snapshot | Python support and full-discovery Python files | Documented resource / async patterns | Test and CI route; explicit analysis boundaries |
| --- | --- | --- | --- |
| Flask, commit `49943b5a94413a530761350f04e896f76e8d620e`, tree `6751ab7eae06e6f97a157860470fd588920a734b` | `>=3.11`; `src`: 24 files / 350,348 bytes; `tests`: 41 / 217,229 bytes; `examples`: 17 / 20,292 bytes; `docs`: 1 / 3,386 bytes. | Docs describe request/application teardown and closing resources. Async views run in a per-request event loop; unfinished spawned tasks are cancelled when the view completes. | GitHub Actions uses `uv run --locked --no-default-groups --group dev tox run`, including pytest environments and typing checks, across Python and OS matrices. Runtime dependencies, project setup and tests are not run; external extension behavior is not qualified. |
| Click, commit `8abb77a8432e03ed14a95f972b44f905279415a7`, tree `84808be7a7674fe64b97f058372ae36a14b9eca6` | `>=3.10`; `src`: 17 files / 453,243 bytes; `tests`: 59 / 522,706 bytes; `examples`: 14 / 29,648 bytes; `docs`: 1 / 1,996 bytes. | Docs describe `Context.with_resource` and `Context.call_on_close`; registered resources are closed when the CLI exits. | GitHub Actions uses tox/pytest and typing checks through locked uv environments over Python and OS matrices. Runtime dependencies, project setup and tests are not run; third-party command/resource behavior is not qualified. |
| FastAPI, commit `534961812bef29ad80d2f7c399e52c688e8a0d57`, tree `3cec183c579a4efb69b34bf737c45631f5ed9271` | `>=3.10`; `fastapi`: 52 files / 834,473 bytes; `tests`: 649 / 3,434,897 bytes; `docs_src`: 485 / 223,100 bytes; `scripts`: 34 / 134,206 bytes. | Docs recommend an async-context-manager lifespan for startup resource setup and shutdown cleanup; legacy startup/shutdown handlers are also documented. | GitHub Actions runs pytest with uv across Python/OS and dependency-source matrices. Runtime dependencies, project setup and tests are not run; external ASGI server, Starlette and Pydantic behavior is not qualified. |

These checkouts were clean at the pinned revisions; their trees and metadata
were captured before analysis. The same frozen candidate `620656b8a8a28ac57a29e589164dc9b58c8141d8`, Python 3.11 target, rules, policy, full default discovery, execution boundaries, eligibility definition, and sampling method apply to all six projects. No findings, labels, or source-level samples were examined in any expansion project at amendment time. The pip failure is retained as a candidate execution failure; no path exclusion or project-specific configuration is introduced to hide it.

The outcomes and retained report artifacts for these six runs are recorded in
the [2026-11-09 pilot results](pilot-results-2026-11-09.md). They do not
complete the acceptance sample or establish A05.

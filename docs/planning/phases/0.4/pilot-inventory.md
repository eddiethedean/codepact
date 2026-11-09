# 0.4 pilot project inventory and preregistration

Status: **Project set, candidate revision, rules, scope and sampling method
frozen on 2026-11-09 before any candidate analysis of the selected repositories.**
This is a preregistration, not a pilot result or A05 pass.

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

Before the first analyzer run, record each pinned checkout's README/metadata,
license, declared Python support, source layout and size, test/CI commands,
resource and cancellation patterns visible from project documentation, and any
explicit unsupported dependency boundaries. These characterize the selected
sample; they must not be used to replace projects after seeing findings. The
individual scorecards and raw reports belong in the 0.4 pilot evidence folder
and must link back to this preregistration and the exact candidate revision.

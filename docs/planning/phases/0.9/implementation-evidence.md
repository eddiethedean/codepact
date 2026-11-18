# 0.9 implementation evidence

The [phase plan](../0.9-obligations-and-change-review.md) and
[validation matrix](validation.md) describe qualification for CodePact 0.9.0.
The comparison baseline is [published 0.8.0](../../../releases/0.8.0-evidence.md).

| Evidence | Reproduction |
| --- | --- |
| V09-01–06 | `cargo test --locked -p codepact --test phase09_outcomes` and `PYTHONPATH=python python -m unittest discover -s tests/python -p test_outcomes.py` exercise opt-in delivery, local carriers, variant acknowledgment, helpers, waivers and installed publication. The review regression pairs a successful acknowledgment with unknown helper effects and a new waiver. |
| V09-07–09 | `cargo test --locked -p codepact --test phase09_work`. The exact CPython 3.13.16 Linux x86-64 run of `scripts/check_model_conformance.py --python-version 3.13.16` passed 40 probes with zero skips, including seven from the new work suite. [Runtime registry](../../../../tests/conformance/runtime-models.json) |
| V09-10–14 | Run `tests/conformance/phase09_provider_probe.py` with the [fixture requirements](../../../../tests/conformance/phase09-provider-requirements.txt). All six runtime cases passed on CPython 3.13.16, Linux x86-64, glibc 2.41 and SQLite 3.46.1. Static CLI replay against the pinned wheel sources recognized 40 symbols across 10 selected distributions. It verified HTTP client use after close, session overlap and rejection of an altered HTTPX installation while HTTPX2 stayed applicable. The Quality workflow retains the combined receipt. |
| V09-15 | `cargo test --locked -p codepact --test phase09_policy` exercises independent preview and enforcement, declaration repair, baseline matching, stale policy and strict incomplete coverage. |
| V09-16–21 | `cargo test --locked -p codepact --test phase09_review` and `python scripts/check_phase08_artifacts.py --cli <installed-command>`. Local replay from the 0.9.0 macOS arm64 CLI wheel covered the four E10-B delta classes, rejected stale artifacts and checked independent installed callers. |
| V09-22 | The helper and macOS arm64 CLI wheels built locally. The hosted release build qualifies the full native wheel matrix and source archives. |
| V09-23 | The local capture compared 24 alternating process pairs per inherited workload and measured two new review workloads. [Performance evidence](performance.md) |

The standalone provider probe records transitive runtime versions. Its runtime
controls execute only the lab fixtures and installed upstream packages. The
analyzer replay checks source without creating the fixture SQLite database.

Release gate signoff and publication evidence belong in the exact-candidate
release record. [Release gates](../../release-gates.md)

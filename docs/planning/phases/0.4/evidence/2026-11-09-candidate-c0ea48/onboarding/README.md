# Exact-candidate onboarding and repair evidence

Status: **Reproduced on build-only Release run 96 artifacts; partial V04-22/23
and A06 evidence only.** This walkthrough does not pass A06, the frozen pilot,
or any full V04 matrix row.

## Run identity

- CodePact source candidate: `c0ea48a2fae771512e7c0010ba235ea9f7136e8d`.
- Build-only workflow: [Release run 96](https://github.com/eddiethedean/codepact/actions/runs/42715489947).
- Artifacts: exact run-96 macOS arm64 CLI wheel and pure-Python helper wheel;
  both installed in a clean CPython 3.11.15 virtual environment with no
  dependencies. `codepact --version` reported `codepact 0.4.0`.
- Host: Darwin arm64. The project fixtures below contain no third-party
  dependencies and were analyzed as source without importing or executing
  project code.
- CLI settings and capabilities are retained in `settings.json` and
  `capabilities.json`. The settings report resolves the project root and
  configured advisory/error policy; the capability response advertises Python
  source targets 3.11–3.13.

## Reproduced workflow

1. `lifecycle-before.py` produces one definite, active TP1001 at line 21.
   `lifecycle-before.json` reports completed analysis, complete coverage for
   the supported subset, and policy fingerprint
   `063222763dd2036f5a086bfee15bc847a12d29853743ca6dfaa694189475f8ec`.
2. `lifecycle-after.py` removes the post-close use. The exact wheel reports no
   diagnostics, completed analysis, complete supported-subset coverage, and
   the same policy fingerprint. This is a source repair, not a policy change.
3. `policy-only-source.py` retains the original source. `policy-only.json`
   retains TP1001 with disposition `rule-disabled`; the exit is zero and the
   policy fingerprint changes to
   `3427d5aa23966fba58d89ef847de5480b2498676a0744af626bdea8404b71b0f`. It is
   therefore distinguishable from a repaired source result.
4. `unsupported-boundary.py` produces two possible TP1002 findings and one
   unknown TP9001; coverage is incomplete with an explicit unknown-call
   reason. The completed report is not presented as clean.
5. The controlled baseline contains one entry. `baseline-reuse.json` records
   one matched/suppressed finding. After adding `baseline-new.py`,
   `baseline-new-finding.json` retains the original as
   `suppressed-baseline` and reports the new file's TP1001 as active.
6. `input-error.json` is a failed-execution envelope for a missing source
   operand, distinct from a completed check with zero findings.

The reports, baseline, source fixtures and capabilities/settings output are
retained alongside this record. SHA-256 values are over the exact file bytes.

| File | SHA-256 |
| --- | --- |
| `capabilities.json` | `f2122a2fd21f924d6f809e5e98b425451bc0b945d197e570af660b71e5d4c2f` |
| `settings.json` | `50089205d777e6115dc90717548b772372c341bd975069eae91b4e6d94617063` |
| `lifecycle-before.json` | `e5dfe3d39edb8de040b680ca4a40bde46545428e32898643630e90be40f5d026` |
| `lifecycle-after.json` | `46fb278bcc1b6702a101929f5de6f31d6a01ea7d647e16e5867297f78e6c424f` |
| `policy-only.json` | `d71796e538db6cfc6aa680ba00c59f252fdf9740ff182e6c8d0731b94e7111e9` |
| `unsupported-boundary.json` | `7a146f65ce4ae7708a656e823ecbc8e088dbdfef5fa81ad8409afd7eef398f63` |
| `input-error.json` | `2a2f9a34c3e7645ac4cfe3e127e9d45bf4bf0141de952f57e2fe9b41d2988016` |
| `baseline.json` | `f7c2f37a68fe0a1612b038d9b9defbc109407e72a026af4d375d2221bae917d9` |
| `baseline-update.json` | `208c169ad3047967c2360930f544a905477a79147a3b524e5667c64fd3f652c4` |
| `baseline-reuse.json` | `bd17cffa1abf0e77410537f383f58065996a24ff84ef509130b0a3503487cd65` |
| `baseline-new-finding.json` | `e012c828ed63e6ed2209e361444a4560f0a545685f355c3db58abfa1c29b1df6` |
| `lifecycle-before.py` | `68676585c7e0af91be8dabbc0211119e46aafe0f32689a43820f1fca1d840587` |
| `lifecycle-after.py` | `15db5214dd6fe94b7d59299e4c83f7d9b65138d4c6f40878bd52a87272610390` |
| `policy-only-source.py` | `68676585c7e0af91be8dabbc0211119e46aafe0f32689a43820f1fca1d840587` |
| `unsupported-boundary.py` | `96e1c1031e241fd71cdf7c58c7ade6f0c44126f27a6c55c5c1b98054b369923a` |
| `baseline-existing.py` | `68676585c7e0af91be8dabbc0211119e46aafe0f32689a43820f1fca1d840587` |
| `baseline-new.py` | `68676585c7e0af91be8dabbc0211119e46aafe0f32689a43820f1fca1d840587` |

The source hashes are expected to match for the initial violation, the
policy-only run and both baseline examples; those controls intentionally use
the same source bytes. The repaired source has a different hash.

## Limits of this evidence

This demonstrates first-use install, settings/capabilities discovery, a
supported repair, a policy-only negative control, an unresolved-boundary
interpretation, baseline create/reuse with a new finding, and a structured
input failure on one host/interpreter. It does not cover the complete CLI
matrix, async pilot, V04-17/19/24/26, the inherited V03 evidence set, project
diversity or maintainer documentation review. A06 and G6 therefore remain
pending.

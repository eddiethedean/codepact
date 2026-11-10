# 0.4 synthetic preview qualification — AD01-A2

The maintainer explicitly selected this release scope on 2026-11-10: **amend
0.4 to a synthetic preview and leave real-world usefulness unqualified**.
This replaces the field-pilot exit criterion for 0.4; it does not turn the
previous unsuccessful field evaluation into a passing pilot. Its six frozen
projects, original selection protocol and raw results remain historical evidence.
No real-project precision, recall, actionability rate or adoption claim follows.

## Acceptance criteria before final-candidate scoring

The accepted manifest SHA-256 is `87ba6167485f8eacfbb7fa527f8ff7a74678cb6f9344bab1324e3879424ce003`.
Its 33 cases and 20 variants are unchanged by AD01-A2.

Freeze the corpus manifest and source/config hashes, checker revision, target
versions, rules and invocation before the final candidate evaluation. The corpus
was developed using earlier checker runs; it is an engineered regression suite,
not an unseen sample or a statistical estimate of real-world quality.

The current independently authored manifest specifies 33 planted violations in
eight bug/repair project pairs and four unsupported-boundary projects. All
20 project variants participate. Project shapes include synchronous libraries,
ETL/CLI applications, plugins, and an async worker; coverage of an async example
does not qualify general task ownership or unsupported cancellation behavior.

A05 passes only when all of the following hold on the frozen candidate:

1. Every manifest bug matches its specified rule, certainty, file and enclosing
   function for Python targets 3.11, 3.12 and 3.13. Findings are matched to the
   independent oracle rather than inferred from checker output.
2. Every paired source repair clears the expected violations without hiding them
   through changed policy, rules, suppression or baselines. Fresh/reused sessions
   and reversed input order agree, including current locations and dependents.
3. Every unsupported project retains its declared boundary diagnostics and
   coverage explanation. Unsupported behavior is never classified as verified
   clean or counted as a definite supported violation.
4. All variants pass Ruff and strict Pyright with the declared ordinary types.
   Those controls demonstrate the selected contract-specific distinction, not
   that every linter or type checker necessarily misses all analogous defects.
5. Read-only/non-execution checks pass; retained commands, exits, source hashes,
   complete reports and candidate identity permit reproduction. Replay the final
   candidate distribution artifact, not only a development executable.
6. User documentation maps representative corpus examples to the feature sets,
   explains source repairs and limitations, and identifies this as synthetic
   preview qualification. Real-world usefulness remains unqualified.

Record the census of planted cases, project/target outcomes, boundary outcomes,
static-tool results and artifact identity. Do not describe the planted-case
success ratio as a real-project precision, recall or actionability statistic.
A mismatched oracle, policy-only repair, missing boundary or artifact mismatch
holds A05. Other V04, operational, compatibility and distribution requirements
remain in force. G7 is a separate explicit decision on the final concrete
candidate; this scope decision does not authorize a tag or publication.

## Retained field work

The [original field protocol](pilots.md) and [frozen inventory](pilot-inventory.md)
remain available for a future real-project evaluation. The [retained replay](evidence/2026-11-10-candidate-eb093c/pilot/README.md)
completed three scans with zero eligible definite findings and failed before
analysis on three other projects. No field sample was labeled. These observations
remain insufficient for the original 30-finding/90%-actionable criterion.

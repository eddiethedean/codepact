# Release evidence record template

Copy this template to a versioned release record when a candidate exists. Placeholders are not approvals or test results.

## Candidate

- Version and phase:
- Candidate commit:
- Previous release / comparison base:
- Phase owner:
- Release reviewer:
- Supported Python/checker/platform matrix:
- Artifact names and hashes:
- Scope changes and accepted decision links:

## Common gates

| Gate | Status | Evidence | Reviewer / date |
| --- | --- | --- | --- |
| G1 Scope | Pending | | |
| G2 Correctness / interoperability | Pending | | |
| G3 Compatibility | Pending | | |
| G4 Operational quality | Pending | | |
| G5 Distribution | Pending | | |
| G6 User readiness | Pending | | |
| G7 Pre-tag decision | Pending | | |
| G8 Publication completion | Pending | | |

## Phase-specific gates

Copy every gate ID from the applicable phase plan into this table.

| Gate ID | Status | Command, fixture or review evidence | Revision / environment | Reviewer |
| --- | --- | --- | --- | --- |
| Replace with actual gate | Pending | | | |

## Findings and decisions

- Blocking issues and resolution:
- Accepted nonblocking issues, owners and follow-up:
- Unsupported patterns and trusted assumptions:
- Pilot sample and diagnostic quality (when applicable):
- Performance baseline and candidate comparison:
- Required migration and changed default diagnostics:
- Reused evidence with applicability justification:

## Publication record

- Pre-tag authorization and reviewer:
- Tag and target commit:
- Workflow run:
- PyPI version and artifact verification:
- crates.io version and artifact verification:
- Clean download/install/CLI smoke checks:
- Partial publication or recovery actions:
- Release notes:
- Final status: Pending / Held / Complete

Mark Complete only after G8 and every required gate pass. Hold the release while evidence or blockers remain unresolved.

## Analyzer architecture evidence

- AM01–AM06 status and links (0.1 and affected changes):
- Ruff parser/AST/semantic components, exact versions or revisions, notices and Salsa version:
- Declared Rust MSRV and clean registry/archive installation evidence:
- Supported target-version grammar and annotation evaluation policies:
- Declaration identity, rebinding and captured-alias fixtures:
- Fresh/updated session parity and query execution event reports:
- No-op, local-edit and exported-dependency edit benchmarks:
- Missing-file, configuration, model and target-version invalidation:
- Cycle/work budgets, cancellation recovery and revision publication:
- Persistent-cache format/recovery and editor parity (from 0.8):

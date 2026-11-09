# AI-agent integration

Status: **Planned**. Owner: maintainer to assign before implementation. This proposal extends the shared analyzer and [linting plan](linting-and-editors.md); it does not claim shipped agent tooling. CLI/reporting and workflow foundations belong to [0.4 adoption](phases/0.4-adoption.md); MCP delivery and client qualification belong to [0.8 ecosystem](phases/0.8-ecosystem.md). Earlier prototypes follow the [roadmap dependency policy](../../ROADMAP.md#sequence-and-dependencies).

## User outcome

An AI coding agent can discover CodePact's supported checks, inspect resource contracts, locate and explain findings, edit the user's code through its existing editing tools, and rerun analysis to verify the result. Findings include enough evidence to distinguish a definite violation from a possible violation or unsupported analysis boundary. A clean supported-subset result must not be presented as a general resource-safety proof.

CLI agents, IDE agents and MCP clients use one semantic engine and the same project settings. Initial integration works through the CLI without an account, hosted model or network service. CodePact remains a deterministic analyzer; the agent supplies reasoning and code edits.

## Delivery sequence

| Work item | Deliverable | Depends on |
| --- | --- | --- |
| AI01 — machine contract (0.4) | Publish versioned JSON schemas, capability discovery, canonical diagnostic fields and error/exit behavior for noninteractive consumers. Extend the shared lint reporting surface rather than introducing a separate checker. | LE01 and the qualified lifecycle analysis |
| AI02 — repair context (0.4) | Provide rule explanations, bounded evidence and repair guidance with semantic preconditions and limitations. | AI01, canonical diagnostics and provenance |
| AI03 — agent workflow (0.4) | Publish a check–edit–recheck recipe and an opt-in project instruction snippet suitable for AGENTS.md or equivalent configuration; demonstrate both a successful repair and an unresolved boundary. | AI01–AI02, project discovery/configuration policies |
| AI04 — MCP adapter (0.8) | Provide a local stdio server exposing read-only analysis and explanation tools backed by the same session/configuration APIs as CLI and LSP. | AI01–AI03, session revision/cancellation and distribution qualification |
| AI05 — integration qualification (0.8) | Verify shell, IDE and MCP workflows with pinned client versions; publish setup, limitations, compatibility and recorded evaluation results. | AI04 and the validation gates below |

Names and new fields remain proposals pending API/schema review. Existing `inspect-contracts` JSON and the developing `check` command are the starting point; agent-specific capabilities must be documented separately from released behavior. Protocol support must not delay CLI-based adoption.

The local 0.4 candidate implements the [accepted adoption specification](phases/0.4/specification.md)'s checker schema 2, capability/explanation commands, snapshot/finding identities and bounded CLI output without pagination or stored snapshots. The [V04 matrix](phases/0.4/validation.md) maps CLI AI-V01–AI-V04, and the [pilot protocol](phases/0.4/pilots.md) defines the supported-repair and unresolved-boundary walkthroughs. AD01 scope acceptance is recorded; full workflow qualification remains pending. MCP and editor-client delivery remain in 0.8.

## Machine-readable analysis contract

- Publish a JSON Schema and fixtures for reports, with engine/schema versions and capability metadata describing supported Python targets, rules, configuration and optional tools. Unsupported schema versions or options produce actionable failures.
- Preserve rule code, severity, certainty, message, primary and related locations, provenance and coverage reasons. Add machine-readable explanation/repair identifiers and rule documentation links without forcing consumers to parse prose.
- Include project identity, effective configuration/model fingerprints, analyzed scope, and a source snapshot identifier or content hashes. Define how these relate to session revisions so an agent can detect that a finding predates its edits. Never expose Salsa allocation IDs as persistent identifiers.
- Define deterministic finding fingerprints and their behavior across edits; keep them distinct from rule codes and invocation/request IDs. State when a changed finding becomes a new identity and how baselines interact with it.
- Separate execution status, findings and coverage. A completed run with violations is an analysis result; invalid configuration, cancellation and work-budget exhaustion have explicit outcomes. Neither an empty diagnostic list nor a zero exit status alone establishes full coverage.
- Allow bounded summaries and paginated details to conserve agent context. Report total/returned counts, truncation and continuation data. Bind pages to an immutable snapshot or reject continuation after changes; omitted findings must never look like a clean report.
- Keep output deterministic for identical inputs; send operational logs to stderr. Publish the CLI's 0/1/2 policy and ensure wrappers preserve reports on finding-related nonzero exits.

## Explanations and repair guidance

Explain the relevant declaration, resource state/transition, origin and supported path evidence when available. Show why a rule fired and what remains unknown; do not invent an execution trace or strengthen declared-unverified metadata into a verified runtime guarantee.

Each rule guide should include a minimal invalid example, a valid alternative, applicable preconditions and counterexamples where the apparent fix changes behavior. Distinguish structured, verified edits from advisory suggestions. Automatic closing, deleting uses, weakening annotations and inserting suppressions are not default repairs. An agent must be able to report that it cannot establish a safe repair.

Source snippets are optional, bounded and attributed to their file/snapshot. Treat comments, strings, filenames and diagnostic excerpts as untrusted source data, not instructions for the agent. The analyzer never imports target modules, evaluates annotations or runs project hooks; local adapters send no source to a model service.

## Check–edit–recheck workflow

1. Resolve the project root, installed CodePact version, supported capabilities and effective configuration. Capture the initial findings, coverage and source snapshot through structured output.
2. Read the relevant rule explanation and related source locations. Use the agent's ordinary editing tools to make the smallest justified change within the user's requested scope.
3. Rerun checks on the updated project and affected dependents, using the same target/settings. A changed-file filter may limit presentation but must not silently skip dependencies needed for analysis.
4. Compare findings and coverage against the initial result. Confirm the intended finding cleared, identify new findings and report unresolved boundaries or execution failures. Expanding exclusions, weakening rules, adding baselines or suppressing findings is a policy change, not evidence of a corrected program.
5. Run the project's relevant existing tests where appropriate and report what the check actually established. Bound retries; when repair fails or evidence is insufficient, return the remaining finding and reason rather than continuing indefinitely.

The planned project instruction snippet documents these steps, version selection and report handling. It must be opt-in; installation does not rewrite a project's AGENTS.md, agent settings, baselines or suppressions. It complements existing project instructions and does not grant new authority to an agent.

## MCP adapter and client targets

Use the official [MCP tool specification](https://modelcontextprotocol.io/specification/latest/server/tools) and [transport specification](https://modelcontextprotocol.io/specification/latest/basic/transports) to qualify a supported protocol revision and client matrix at implementation time. Start with local stdio and structured tool results; protocol errors and tool execution failures remain distinguishable from successfully reported CodePact findings. Support negotiated capabilities and bounded cancellation/recovery as required by the selected revision.

| Proposed tool | Purpose | Contract |
| --- | --- | --- |
| `codepact_capabilities` | Discover engine/schema versions, supported analysis and available tools. | Read-only; versioned output |
| `codepact_check` | Check files/project scope and return findings, coverage and snapshot identity. | Read-only; same semantics/settings as CLI; bounded detail and continuation |
| `codepact_inspect_contracts` | Read declared contracts and provenance for a selected source scope. | Read-only; preserve inspection assurance boundaries |
| `codepact_explain` | Retrieve canonical rule guidance and evidence for a finding in a known snapshot. | Read-only; reject stale finding references or clearly request reanalysis |

Validate tool arguments and enforce configured workspace roots, path containment including symlinks, exclusions and work budgets. Tool annotations describe behavior; actual implementation must enforce the read-only contract. Do not accept arbitrary shell commands, install packages or modify source/configuration through these tools. Agents retain their existing edit permissions and workflow.

Qualification targets are a shell-based agent using JSON, an IDE agent consuming the shared editor diagnostics, and at least two independent MCP clients. Record exact client/server/protocol versions and platform combinations before claiming compatibility. Recipes must work alongside existing Python tooling. Hosted services, remote transports and autonomous write/fix tools require separate scope and evidence.

## Validation and release evidence

| Gate | Required evidence |
| --- | --- |
| AI-V01 — contract/parity | Schema fixtures and equivalent CLI/tool reports agree on facts, codes, spans, severity, certainty and coverage; include valid, invalid, unknown and unsupported-version inputs. |
| AI-V02 — repair workflow | A recorded check–edit–recheck clears a real supported violation without weakening policy or hiding new findings; ambiguous repairs remain unresolved with an explanation. Recheck affected dependents and compare coverage. |
| AI-V03 — revisions/output limits | Concurrent edits, stale references, cancellation, time/work limits and paginated/truncated output never present old or partial findings as a complete clean result. |
| AI-V04 — boundaries | Root/symlink escapes, source containing instruction-like text and side-effectful Python fixtures preserve data attribution, workspace limits and the no-execution/read-only guarantees. |
| AI-V05 — clients/artifacts | Clean setup recipes pass for every claimed shell/IDE/MCP combination; missing binaries, incompatible schemas/protocols and server restart have documented recovery. |
| AI-V06 — usefulness | Record a reproducible valid/invalid/unknown corpus, task selection, tool/client/model versions where applicable, repair success, new regressions, false clean claims, remaining boundaries, latency and output size. Keep deterministic contract tests separate from variable live-agent evaluations. |

For 0.4, A08 requires AI01–AI03 and AI-V01–AI-V04 for CLI workflows, plus a recorded CLI repair walkthrough. For 0.8, E09 requires AI04–AI05 and all AI-V01–AI-V06, including parity with released editor behavior. Agree evaluation budgets and acceptance thresholds before running the corpus; missing evidence keeps gates pending. Link results to the candidate [release evidence record](release-evidence-template.md).

## Concurrent repair workflows

Inherit [CS02/CS06](concurrency.md) reporting and model assumptions. Agent tools preserve task/resource origins, capture/close/transfer/join sites, cancellation effects, certainty and unknown process/thread boundaries. In 0.4, demonstrate a supported lifecycle or synchronous must-use repair and an unresolved async/process boundary using inherited 0.3 evidence. Qualify a structured-task repair alongside 0.8 CLI/editor/tool parity only after CS03 establishes that scope. Rechecking must not turn an unsupported boundary into a verified repair. Do not recommend or automatically insert cancellation suppression, shielding, joins, moves or close calls without verified behavioral preconditions; a wrapper or cancellation request alone proves neither completion nor cleanup.

# CodePact linting and IDE diagnostics

Status: **Planned**. This is a proposal, not shipped editor support. Owner: maintainer to assign before implementation. Command/reporting foundations belong to [0.4 adoption](phases/0.4-adoption.md); the language server and supported editor integrations belong to [0.8 ecosystem](phases/0.8-ecosystem.md). Prototypes may begin earlier under the [roadmap dependency policy](../../ROADMAP.md#sequence-and-dependencies).

## User outcome

Python developers see CodePact findings as source underlines and entries in their IDE's Problems or inspection view. Each finding identifies CodePact, its rule code, severity and explanation, with navigation to related resource declarations, close calls or ownership transitions when available. Fixing the source removes the finding. The same source and settings produce the same findings in the CLI and editor.

CodePact supplements ordinary Python type checking. Editor adapters reuse the Rust analyzer; they do not implement separate resource rules. Initial rules expose only the analysis qualified in earlier releases, including declaration errors, supported lifecycle violations and explicit analysis boundaries. Ownership and other later rules become available only as their semantic gates pass.

The companion [AI-agent integration plan](ai-agent-integration.md) reuses these diagnostic and configuration contracts for structured check–edit–recheck workflows and local MCP tools. Editor and agent adapters must agree on findings, source revisions and analysis boundaries.

## Delivery sequence

| Work item | Deliverable | Depends on |
| --- | --- | --- |
| LE01 — lint command contract (0.4) | Specify a `codepact check` reporting surface with human-readable and versioned JSON output, plus a location-per-line format for external-tool consumers. Reconcile with the 0.2 check interface rather than adding a duplicate command. | Qualified lifecycle checker and 0.4 reporting/configuration policies |
| LE02 — diagnostic adapter (0.8) | Convert canonical diagnostics and coverage into editor diagnostics; retain codes, severity, certainty, related locations and links to rule explanations. | LE01, stable candidate diagnostic/schema contract |
| LE03 — shared language server (0.8) | Add a proposed `codepact lsp` stdio entry point using the existing Salsa session and shared configuration resolver. | LE02, revision/cancellation and project-discovery APIs |
| LE04 — editor clients (0.8) | Provide a thin VS Code extension, a JetBrains integration and documented Neovim/Emacs LSP configurations. | LE03, supported-client/version qualification |
| LE05 — distribution and qualification (0.8) | Verify installed commands, client/server version compatibility, clean setup and the validation matrix below on declared platforms. | LE01–LE04, E01–E07 and release artifact gates |

Command names and options remain proposals until their API review. The current 0.1 `inspect-contracts` command remains a declaration inspector; this plan does not claim it already performs flow linting or serves LSP.

The local 0.4 candidate implements the [accepted adoption specification](phases/0.4/specification.md)'s configuration/exit policy, checker schema 2 and tab-separated location format. [V04-05–08/18/25](phases/0.4/validation.md) still require full qualification for LE01 and CLI LE-V01/LE-V02; LSP comparisons and client setup remain 0.8 requirements. AD01 scope acceptance is recorded; technical qualification remains pending.

## Lint command and configuration

- Specify deterministic rule codes, ordering, severity and explanation URLs. Human and machine reports use the same underlying findings; retain the existing contract-inspection interface separately.
- Use 0.4's exit policy: `0` for below-threshold findings, `1` for actionable findings under the selected policy, and `2` for invocation/configuration or execution failure. Keep stdout parseable for machine consumers and send operational messages to stderr.
- Define the location-per-line grammar, including escaping paths with spaces or colons, source encoding and column units. Provide a sample problem matcher or external-tool configuration. Consumers unable to parse it reliably should use JSON.
- Share project roots, Python source target, includes/excludes, rule selection, severity overrides, suppressions and baselines with the CLI. Document configuration precedence and validate unknown keys. Editor display preferences must not change semantic certainty.
- Preserve coverage and certainty. Incomplete analysis must remain visible even when no violation is found; display a coverage summary or explicit boundary diagnostic. A cancelled or failed run must not appear as a successful clean check.

## Language server behavior

Use the [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) as the protocol baseline. Negotiate capabilities and position encoding; implement document synchronization and push diagnostics first. Pull diagnostics and code actions may follow after their own evidence.

- Handle initialization, shutdown and exit; reserve stdout for framed protocol messages and stderr for logs.
- Analyze unsaved document contents on open/change and reconcile saved files, close events, deletes and renames. Debounce edits, cancel superseded work and publish results only for the current document version and project revision.
- Recheck affected callers when imported resource contracts change. Track filesystem/configuration changes, multiple workspace roots and files sharing the same basename. Retain an explicit mapping from analyzer paths to document URIs rather than guessing from display labels.
- Convert the analyzer's one-based UTF-8 byte spans to zero-based negotiated LSP positions with exclusive ends; support the UTF-16 fallback. Test non-ASCII text, emoji, multiline spans and CRLF source. Preserve related locations across files.
- Identify diagnostics with source `codepact`, canonical rule codes and severities. Preserve certainty in the message or accompanying details; expose coverage independently of the severity threshold. Clear obsolete findings after fixes and invalidate results for deleted or excluded files.
- Keep bounded work and retained-memory policies. Recover after cancellation or invalid configuration without leaving misleading old findings; expose an actionable server/configuration failure in the client.
- Analyze source as data without importing target modules, running project hooks or evaluating annotations. Respect editor workspace trust when selecting a project-provided executable.

## Editor targets

These are qualification targets; none is claimed supported until installation and behavior are verified. Record exact editor, client, server, OS and architecture versions in release evidence.

| Editor | Planned delivery | Acceptance focus |
| --- | --- | --- |
| VS Code | Thin extension that launches the shared server; follow the [official language-server guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide). | Inline findings, Problems view, related locations, executable selection and coexistence with Python type checkers and Ruff |
| PyCharm / IntelliJ IDEA with Python support | Thin plugin using available LSP APIs; qualify supported products, editions and minimum versions against the [JetBrains LSP documentation](https://plugins.jetbrains.com/docs/intellij/language-server-protocol.html). | Python-file diagnostics and navigation; document product/API restrictions and evaluate an external-tool fallback where needed |
| Neovim | Document setup through its built-in LSP client. | Underlines, diagnostic list, root detection and executable discovery |
| Emacs | Document Eglot setup for the same server; evaluate coexistence with an existing Python server and a Flymake command adapter where needed. | Source diagnostics, related-location access and a usable configuration alongside existing Python tooling |

Do not claim automatic compatibility for every LSP-capable editor. Each recipe must be reproduced; adapters should coexist with existing language tools without duplicate CodePact reports. Notebook cells, remote/browser-only clients and automatic installation of server binaries require separate scope and evidence.

## Validation and release evidence

| Gate | Required evidence |
| --- | --- |
| LE-V01 — parity | Golden CLI/JSON/LSP comparisons on identical source/settings: codes, messages, locations, severity, certainty, suppressions and coverage agree. Include valid, invalid and unsupported examples. |
| LE-V02 — positions | Unicode, emoji, CRLF, multiline spans, paths with spaces, duplicate basenames and cross-file related locations resolve to the intended source. |
| LE-V03 — edits | Unsaved edits, rapid changes, dependency/config changes, cancellation, close/reopen, rename and delete never publish superseded findings; fixing a violation clears it. |
| LE-V04 — clients | A clean setup walkthrough for every claimed editor demonstrates one real CodePact violation, related-location navigation, a fix, and coexistence with existing Python tools. Record any client limitations. |
| LE-V05 — failures | Invalid configuration, missing/incompatible executables, malformed requests and server restart produce useful errors and recover without stale clean status or protocol corruption. |
| LE-V06 — responsiveness | Record cold startup, warm local/dependency edits, cancellation recovery and peak memory on a named corpus. Agree latency/work budgets before implementation and retain measurements. |
| LE-V07 — artifacts | Installed CLI/server and editor packages work on each declared target; protocol/schema incompatibilities fail clearly with upgrade guidance. |

Attach these results to 0.4 A01/A03/A07 for the command foundations and 0.8 E03/E07/E08 for editor delivery. Keep all gates pending until evidence exists. Rule explanation pages and editor setup/troubleshooting guides must accompany the first supported release. Automatic resource-closing fixes remain deferred until their semantic preconditions are proven.

## Concurrent resource diagnostics

As [CS02/CS06](concurrency.md) land, preserve task origin, resource owner, capture/transfer, close, suspension and join/escape related sites where modeled. CLI and editor must agree on certainty, unsupported async/process boundaries, model assumptions and source revision. In 0.4, qualify the inherited 0.3 coroutine/cleanup evidence and unsupported handoffs; do not fabricate task/join sites. After CS03 qualifies task ownership, include a supported structured-task lifetime violation and an unsupported process handoff in 0.8 CLI/LSP parity fixtures. A green editor state must not turn unknown concurrency coverage into a safety claim; fixes that add joins, shielding or cleanup need verified behavioral preconditions.

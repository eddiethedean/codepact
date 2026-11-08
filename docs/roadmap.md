# Roadmap

CodePact 0.x grows from declaration inspection toward broader resource lifecycle analysis. The predecessor TypePact published contract inspection in 0.1.0 and bounded lifecycle checking in 0.2.0. CodePact 0.3.0 is the first candidate under the new names; later phases have no promised delivery dates.

| Release line | Planned focus |
| --- | --- |
| 0.1.x | Parse supported declarations and report contract facts, provenance, diagnostics, and coverage |
| 0.2.x | Published bounded lifecycle flow checking and use-after-close analysis |
| 0.3.x | Cleanup obligations and bounded async cancellation/cleanup |
| 0.4.x | Adoption tooling and useful previews |
| 0.5.x | Ownership transfer, lexical borrowing and structured task lifetimes |
| 0.6.x | Suspension-aware exclusive access, typestate, and effects |
| 0.7.x | Typed outcomes and finite exhaustiveness |
| 0.8.x | Ecosystem integrations, shared lint language server and IDE diagnostics |
| 0.9.x | Stabilization and a 1.0 readiness decision |

The roadmap describes direction, not arbitrary Python ownership proofs or memory-safety guarantees. The CodePact 0.3 candidate's fixed code passed local checks and Quality CI; maintainer qualification and current-candidate distribution evidence remain pending. Read [availability](availability.md) for the current package boundary and [release notes](release-notes.md) for published behavior and the unreleased 0.3 candidate. Each phase must define its supported subset and evidence before a release claim is made.

Planned linting work adds command/reporting foundations in 0.4, followed in 0.8 by inline CodePact diagnostics and problem-list entries through a shared language server. Target editors are VS Code, PyCharm/IntelliJ, Neovim and Emacs; support depends on verified client/version combinations. These features are not available in 0.1.

Planned AI-agent integration adds structured reports, rule explanations and check–edit–recheck recipes in 0.4, followed by local read-only MCP tools and verified agent client workflows in 0.8. Agents will share the CLI's analysis and retain explicit certainty, source revisions and incomplete-coverage signals. These features remain planned.

An optional `codepact-pyrefly-cli` prototype is also planned, with its own `codepact-pyrefly` command combining Pyrefly's ordinary Python type checking and CodePact's resource checks. It will have independent prereleases and qualification gates alongside the main roadmap; no combined package or command is available yet.

For the detailed planning documents, see the [repository roadmap](https://github.com/eddiethedean/codepact/blob/main/ROADMAP.md) and [planning index](https://github.com/eddiethedean/codepact/tree/main/docs/planning).

Planned concurrent resource safety starts with bounded async cleanup/cancellation in 0.3, structured task ownership and borrow escapes in 0.5, and suspension-aware exclusivity and protocol states in 0.6. Reporting, outcome adapters, integrations and stabilization inherit explicit cancellation and coverage requirements. Initial task models target `asyncio.TaskGroup`; multiprocessing and thread handoffs remain unsupported escape boundaries until separately qualified. This adds no race-freedom or deadlock-freedom guarantee and does not change the synchronous behavior of the historical TypePact 0.2.0 release. See the [concurrency plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/concurrency.md) for scope and acceptance gates.

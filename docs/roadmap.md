# Roadmap

CodePact 0.x grows from declaration inspection toward broader resource lifecycle analysis. The predecessor TypePact published contract inspection in 0.1.0 and bounded lifecycle checking in 0.2.0. CodePact 0.3.0 is the first published release under the new names; later phases have no promised delivery dates.

| Release line | Status and focus |
| --- | --- |
| 0.1.x | Parse supported declarations and report contract facts, provenance, diagnostics, and coverage |
| 0.2.x | Published bounded lifecycle flow checking and use-after-close analysis |
| 0.3.x | Published: cleanup obligations and bounded async cancellation/cleanup |
| 0.4.x | Adoption tooling and synthetic preview |
| 0.5.x | Ownership transfer, lexical borrowing and structured task lifetimes |
| 0.6.x | Suspension-aware exclusive access, typestate, and effects |
| 0.7.x | Typed outcomes and finite exhaustiveness |
| 0.8.x | Ecosystem integrations, shared lint language server and IDE diagnostics |
| 0.9.x | Stabilization and a 1.0 readiness decision |

The roadmap describes direction, not arbitrary Python ownership proofs or memory-safety guarantees. CodePact 0.3.0 passed local checks, Quality CI, release qualification, publication, and registry verification; phase-specific gates and G1–G8 evidence are recorded in the release evidence. Read [availability](availability.md) for the current package boundary and [release notes](release-notes.md) for published behavior. Each phase must define its supported subset and evidence before a release claim is made.

The local 0.4 candidate implements CLI reporting foundations. The shared language server, inline CodePact diagnostics and problem-list entries remain planned for 0.8. Target editors are VS Code, PyCharm/IntelliJ, Neovim and Emacs; support depends on verified client/version combinations.

The local 0.4 candidate implements structured reports, rule explanations and an opt-in check–edit–recheck recipe. Local read-only MCP tools and verified agent client workflows remain planned for 0.8. Any agent consumer must preserve certainty, source identity and incomplete-coverage signals.

An optional `codepact-pyrefly-cli` prototype is also planned, with its own `codepact-pyrefly` command combining Pyrefly's ordinary Python type checking and CodePact's resource checks. It will have independent prereleases and qualification gates alongside the main roadmap; no combined package or command is available yet.

For the detailed planning documents, see the [repository roadmap](https://github.com/eddiethedean/codepact/blob/main/ROADMAP.md) and [planning index](https://github.com/eddiethedean/codepact/tree/main/docs/planning).

Planned concurrent resource safety starts with bounded async cleanup/cancellation in 0.3, structured task ownership and borrow escapes in 0.5, and suspension-aware exclusivity and protocol states in 0.6. Reporting, outcome adapters, integrations and stabilization inherit explicit cancellation and coverage requirements. Initial task models target `asyncio.TaskGroup`; multiprocessing and thread handoffs remain unsupported escape boundaries until separately qualified. This adds no race-freedom or deadlock-freedom guarantee and does not change the synchronous behavior of the historical TypePact 0.2.0 release. See the [concurrency plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/concurrency.md) for scope and acceptance gates.

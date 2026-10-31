# Roadmap

TypePact 0.x grows from declaration inspection toward broader resource lifecycle analysis. The 0.1 release is the contract-inspection foundation; later phases are planned and have no promised delivery dates.

| Release line | Planned focus |
| --- | --- |
| 0.1.x | Parse supported declarations and report contract facts, provenance, diagnostics, and coverage |
| 0.2.x | Lifecycle flow checking and use-after-close analysis |
| 0.3.x | Cleanup obligations and additional resource tracking |
| 0.4.x | Adoption tooling and useful previews |
| 0.5.x | Ownership transfer and lexical borrowing |
| 0.6.x | Exclusive access, typestate, and effects |
| 0.7.x | Typed outcomes and finite exhaustiveness |
| 0.8.x | Ecosystem integrations, shared lint language server and IDE diagnostics |
| 0.9.x | Stabilization and a 1.0 readiness decision |

The roadmap describes direction, not arbitrary Python ownership proofs or memory-safety guarantees. Each phase must define its supported subset and evidence before a release claim is made.

Planned linting work adds command/reporting foundations in 0.4, followed in 0.8 by inline TypePact diagnostics and problem-list entries through a shared language server. Target editors are VS Code, PyCharm/IntelliJ, Neovim and Emacs; support depends on verified client/version combinations. These features are not available in 0.1.

Planned AI-agent integration adds structured reports, rule explanations and check–edit–recheck recipes in 0.4, followed by local read-only MCP tools and verified agent client workflows in 0.8. Agents will share the CLI's analysis and retain explicit certainty, source revisions and incomplete-coverage signals. These features remain planned.

An optional `typepact-pyrefly-cli` prototype is also planned, with its own `typepact-pyrefly` command combining Pyrefly's ordinary Python type checking and TypePact's resource checks. It will have independent prereleases and qualification gates alongside the main roadmap; no combined package or command is available yet.

For the detailed planning documents, see the [repository roadmap](https://github.com/eddiethedean/typepact/blob/main/ROADMAP.md) and [planning index](https://github.com/eddiethedean/typepact/tree/main/docs/planning).

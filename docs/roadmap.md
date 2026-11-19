# Roadmap

CodePact 0.x grows from declaration inspection toward broader resource lifecycle analysis. The predecessor TypePact published contract inspection in 0.1.0 and bounded lifecycle checking in 0.2.0. CodePact 0.3.0 was the first release under the CodePact name. CodePact 0.10.0 was published on November 18, 2026.

| Release line | Status and focus |
| --- | --- |
| 0.1.x | Parse supported declarations and report contract facts, provenance, diagnostics, and coverage |
| 0.2.x | Published bounded lifecycle flow checking and use-after-close analysis |
| 0.3.x | Published: cleanup obligations and bounded async cancellation/cleanup |
| 0.4.x | Published preview of adoption tooling and synchronous must-use checking |
| 0.5.x | Release with opt-in ownership transfer, lexical borrowing, and structured task lifetimes |
| 0.6.x | Suspension-aware exclusive access, typestate, and effects |
| 0.7.x | Typed outcomes, helper and CLI install split, and an installed-wheel must-use preview |
| 0.8.x | Reviewed resource contracts across packages, wheel manifests, and bounded standard library models |
| 0.9.x | Published persistent duties, before-and-after review, and reviewed HTTP and SQL profiles |
| 0.10.x | Published opt-in asyncio queues, multiprocessing IPC, synchronization guards, and direct-script spawn. [Release notes](release-notes.md) |
| 0.11.x | Planned async resource cleanup, worker shutdown, executor bridges, and bounded shared-memory ownership |
| 0.12.x | Broader model coverage, persistent caching, and editor and agent integrations |
| 0.13.x | Stabilization and a 1.0 readiness decision |

CodePact 0.8.0 extends the marked installed-wheel support introduced in 0.7
with resource contracts, manifest export and verification, and project models
for exact dependency versions. CodePact 0.9.0 adds artifact comparison and
before-and-after source review. [Reviewed contracts](reviewed-contracts.md) ·
[Published contracts design](https://github.com/eddiethedean/codepact/blob/main/docs/planning/published-contracts.md)

The roadmap describes direction, not arbitrary Python ownership proofs or memory-safety guarantees. CodePact 0.5.0 adds experimental ownership to the 0.4 adoption preview. Qualification is synthetic and field usefulness remains unqualified. Read [availability](availability.md) for the current package boundary and [release notes](release-notes.md) for published behavior. Each phase must define its supported subset and evidence before a release claim is made.

The shared language server, inline diagnostics, read-only MCP tools, and
verified client workflows are planned for 0.12. Target editors are VS Code,
PyCharm/IntelliJ, Neovim, and Emacs. Editor and agent consumers need to preserve
certainty, source identity, and incomplete coverage.

The repository roadmap tracks a separate TypePact prototype that combines
Pyrefly type checking with CodePact resource checks.

Follow-on proposals cover validated values, relationships between values,
state-dependent field guarantees, and declared dependency boundaries.
[Follow-on phase plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/contract-extensions.md) ·
[Repository roadmap](https://github.com/eddiethedean/codepact/blob/main/ROADMAP.md)

The reviewed runtime profiles cover selected local asyncio tasks and explicit-spawn
Process and Pool lifecycles. CodePact 0.9.0 tracks result observation for supported
tasks, futures, pool jobs, and thread-pool jobs independently of completion and
cleanup.
[Concurrency plan](https://github.com/eddiethedean/codepact/blob/main/docs/planning/concurrency.md)

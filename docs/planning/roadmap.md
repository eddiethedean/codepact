# Feature roadmap

Milestones are completion gates, not calendar promises. Estimates should follow the first parser and compatibility prototypes.

| Milestone | Deliverables | Dependencies / exit gate |
| --- | --- | --- |
| M0 — feasibility | Ruff AST adapter, declaration identities, Salsa session, IR v0, model policy | Both APIs normalize contracts; rebinding and incremental parity/reuse fixtures pass; AM01–AM06 qualified |
| M1 — vertical slice | CLI, resource lowering, call summaries, use-after-close | Mixed-interface example and alias/branch counterexamples pass |
| M2 — useful preview | Cleanup edges, must-use, configuration, JSON output, baseline | Pilot triage and packaging gates met |
| M3 — ownership experiment | Explicit move, owned handles, lexical shared borrow | Escape handling and known-alias conflicts documented and tested |
| M4 — advanced contracts | Exclusive borrowing, finite typestate, richer effects | Conservative interprocedural summaries and suspension handling |
| M5 — ecosystem and stability | Result/Option adapters, editor integration, broader models | Stable schemas, compatibility matrix and 1.0 readiness review |

Runtime Result/Option may be developed after M1 in parallel with M2 work, but their inclusion must not delay correctness of the resource slice. General lifetime relations and trait machinery remain research after M4.

## Priority policy

Prefer changes that improve diagnostic trust, interoperability, adoption and measurable performance. A new public API needs a real use case and a canonical lowering. Block feature expansion when the current analysis silently accepts known unsound cases.

## Preview scope

The first preview must name the exact supported resource patterns, parser versions, alias forms, external summaries and unsupported constructs. Default rules should have low triage cost. Experimental ownership rules remain opt-in until pilot evidence supports broader use.

## Deferred decisions

Ruff AST and Salsa are the accepted analyzer direction. Qualify exact component versions, semantic primitive reuse, Rust MSRV and Cargo packaging in AM01. MIT and a pure Python codepact package depending on matching native codepact-cli wheels is selected under D015, with the Cargo crate retained. Result variance and exact transition syntax need their relevant RFC/evidence. Do not reserve roadmap versions as promises of delivery dates.

## Versioned release sequence

The [root ROADMAP](../../ROADMAP.md) is authoritative for 0.x release order and acceptance. M0–M5 above remain feature groupings; the root roadmap maps them to 0.1–0.9 with dedicated phase plans and required gates.

## Concurrent resource safety

Follow [CS01–CS07](concurrency.md) within the existing release sequence: cleanup/cancellation foundations in M2, structured task ownership and borrow escapes in M3, suspension-aware exclusivity and protocol effects in M4, then model/integration qualification and stability in M5. The initial target is bounded `asyncio.TaskGroup` resource lifetimes. Multiprocessing and thread handoffs require conservative escape boundaries; semantic process support needs a separate reviewed model proposal. This does not expand M1 or promise race/deadlock freedom.

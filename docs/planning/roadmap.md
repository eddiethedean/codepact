# Feature roadmap

Milestones are completion gates, not calendar promises. Estimates should follow the first parser and compatibility prototypes.

| Milestone | Deliverables | Dependencies / exit gate |
| --- | --- | --- |
| M0 — feasibility | Alias prototype, parser choice, IR v0, model policy | Both APIs normalize one resource contract; compatibility evidence recorded |
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

Choose parser, native packaging approach, Rust MSRV, alias implementation, Result variance, exact transition syntax and license in M0 or the relevant RFC. Do not reserve roadmap versions as promises of delivery dates.

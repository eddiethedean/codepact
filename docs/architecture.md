# Architecture

CodePact has two Python distributions and one Rust analyzer. The `codepact`
distribution provides typed helpers and metadata. Its matching
`codepact-cli` dependency supplies the native executable. The Rust crate
exposes the shared engine for applications that provide source snapshots
directly.

## From source to report

```text
check: config + source discovery      inspect-contracts: explicit files
                 |                                  |
                 +-------- UTF-8 source files ------+
                                      |
                              Ruff parser and AST
                                      |
                    declaration and import resolution
                             |                  |
                      bounded flow pass         +---- inspection JSON
                             |
               rule policy, audit, and reporting ---- check report + exit
```

The CLI resolves the project policy and file selection before checking.
Logical module names come from paths under the selected root. Declaration
inspection receives only the named files. Both modes parse source locally.
Helper imports and resource declarations are interpreted from source syntax.

When dependency roots are configured, the CLI resolves the winning import
and reads bounded installed artifact inputs. The analyzer selects contract
facts from those inputs, keeping their public names, canonical identities,
and provenance. Wheel verification and installed discovery share metadata
parsing and digest verification, with checks for their respective layouts.
[Reviewed contracts](reviewed-contracts.md)

Pinned Ruff components parse the selected source grammar and preserve
diagnostic spans. Semantic queries resolve supported class and alias
identities, imported helpers, lifecycle metadata, and access markers.
Inspection returns contract facts, declaration diagnostics, and coverage.

The checker's bounded flow pass tracks open, closed, and unknown resource
states, aliases, and cleanup obligations. Resolved calls can transfer effects
across supplied modules. Scope and effect limits leave a visible coverage
reason. CodePact 0.5.0's opt-in ownership scope adds tracked moves and lexical
borrows. Its separate TaskGroup assumption enables a bounded model of child
task lifetimes. [Analysis boundaries](limitations.md) ·
[Experimental ownership](ownership.md)

After analysis, the adoption layer applies rule selection, severity,
suppressions, and baseline matching. It combines those decisions with the
snapshot and model identities in the report. CLI rendering uses the report
to produce JSON, text, or concise output and the corresponding exit status.
Rust callers can use canonical results or the adoption report builder.
[Report reference](output.md) · [Rust integration](rust-api.md)

## Locate a change

| Change | Start in |
| --- | --- |
| Parsed syntax, bindings, or declaration facts | `crates/codepact/src/frontend/, crates/codepact/src/lib.rs` |
| Stateful checks, calls, cleanup, and concurrency | `crates/codepact/src/flow/` |
| Installed providers, artifact verification, and model selection | `crates/codepact/src/foundation/` and `foundation.rs` |
| Rule policy, root and configuration resolution, or report construction | `crates/codepact/src/adoption/` and `adoption.rs` |
| Before-and-after semantic comparison | `crates/codepact/src/review/` and `review.rs` |
| CLI rendering and command behavior | `crates/codepact/src/cli/` and `main.rs` |
| Incremental queries and invalidation | `crates/codepact/src/db.rs` |

Most analyzer fixtures are in `crates/codepact/tests/`. For a worked path
from a diagnostic to a fixture, see [Make a first rule change](development.md#make-a-first-rule-change).

## Incremental sessions

Salsa tracks parsing, declaration summaries, and flow queries with their input
dependencies. `AnalysisSession` synchronizes each full source snapshot and
can reuse work for unchanged files. A change to a relevant provider summary
can invalidate dependent checks. Cancellable session methods return an error
when they cannot publish a complete current result. The CLI starts a new
process for each invocation. [Sessions and cancellation](rust-api.md#incremental-analysis-session) ·
[Performance](performance.md)

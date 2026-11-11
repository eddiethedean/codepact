# How contract inspection works

The CLI and Rust library share a bounded pipeline for turning Python declarations into contract facts. This page explains how the inspector reaches its result; use the [first check](quickstart.md) to learn lifecycle checking.

## Try declaration inspection

Download {download}`the declaration-only sample <_examples/connection.py>`
as `connection.py` in the [quick-start project](quickstart.md). Run:

<!-- smoke: inspection -->
```sh
uv run --locked codepact inspect-contracts connection.py
```

Expect exit `0` and inspection schema 0 JSON. The result shows
`connection.Connection` closed by `close` and shared access on
`inspect_connection.conn`. Compare the
{download}`complete report <_examples/inspection.json>`. With module-level
executable calls, the inspector can still return facts and exit `1` for
incomplete coverage. Use `check` for lifecycle findings in executable code.

## Input and parsing

The CLI loads explicitly supplied UTF-8 files and derives logical module names from their paths. The Rust API accepts equivalent `SourceFile` values directly. Neither interface imports or executes the analyzed Python project.

Pinned Ruff parser and AST components parse the selected Python 3.11, 3.12, or 3.13 grammar. The frontend converts supported module-level statements into typed declarations, preserving source ranges for imports, decorators, assignments, parameters, and returns. Syntax failures become diagnostics with incomplete coverage.

The selected source target also controls Unicode identifier validation. This is independent of the Python version used to install the helpers or the interpreter running the application.

## Declaration identities and rebinding

The semantic layer recognizes CodePact APIs through their imported identity. An unrelated local function named `resource` does not become a CodePact decorator merely because its name matches. Imported aliases of supported APIs can retain their canonical meaning until rebound.

Supported assignments capture a class or alias view at the point of declaration. Later rebinding of the original name does not change the class and metadata already captured by an earlier alias. Conversely, new annotations use the bindings that are visible at their own declaration point.

Lifecycle and access facts compose independently. Compatible metadata can be inherited from the underlying class or captured alias view; incompatible lifecycle methods or access qualifiers produce `TP4002` with relevant source locations.

## Imports across supplied modules

CodePact plans direct absolute imports between supplied files, analyzes providers before their consumers, and gives consumers semantic export summaries. Classes and supported assignment aliases can carry lifecycle and access metadata across these boundaries.

The inspector does not discover files or search installed packages. Relative imports, import re-export chains, ambiguous module identities, and supplied-module cycles produce incomplete coverage. The [multi-file tutorial](project-guide.md) shows a complete supported example.

## Incremental queries

Salsa tracks parsing and per-module inspection separately. Consumers depend on semantic export summaries, allowing results to remain reusable when a provider edit does not change a relevant export. Source locations can refresh independently of unchanged consumer facts.

`AnalysisSession` maintains the database across calls. Each inspection synchronizes it to the full supplied source snapshot. Results from `inspect_cancellable` include a revision; cancellation returns an error without publishing a partial report. The session remains reusable after cancelled work. The one-shot `inspect` function creates a session and uses the same queries.

Sessions retain at most 1,000,000 bytes of active source and periodically rebuild cached state after 1,000 source mutations. Requests also bound files, statements, expression conversion, import edges, and emitted contracts. See [input and work limits](supported-syntax.md#input-and-work-limits) and the [session API](rust-api.md#session-operations).

## Coverage and assurance

Unsupported declarations are visible in diagnostics and coverage rather than silently treated as resolved. Some boundaries can invalidate bindings conservatively, so later declarations in the same file can also become unresolved. Useful facts may still be emitted alongside incomplete coverage; the result is not a complete analysis of the whole source in that case.

All contracts carry `declared-unverified` trust. The inspector records metadata without proving that the implementation honors it. Function body flow, cleanup obligations, use-after-close, ownership transfer, and exclusive access enforcement are outside declaration inspection. See the [JSON reference](output.md), [limitations](limitations.md), and [roadmap](roadmap.md).

Implementation and qualification plans are maintained in the [repository planning directory](https://github.com/eddiethedean/codepact/tree/main/docs/planning).

# Supported syntax

The inspector handles a deliberately limited set of Python declarations. It parses source with the selected Python grammar, resolves only the supported static relationships, and marks coverage incomplete when a relevant construct falls outside that set.

The declaration sections describe published 0.1.0. The
[lifecycle preview](lifecycle.md) adds the checking subset below in a source
checkout for the upcoming 0.2.0 release. Its phase qualification remains pending.

## Recognized declarations

- Module-level classes.
- A literal `@resource(closed_by="method")` from `typepact`.
- Simple class and assignment aliases, including supported `typing.Annotated`, `Borrowed`, `Owned`, and `BorrowedMut` forms.
- Function parameter and return annotations that resolve to supported classes or aliases.
- Lifecycle metadata using `Resource(closed_by="method")` and access metadata using `SharedBorrow()`, `OwnedAccess()`, or `ExclusiveBorrow()`.
- Direct absolute imports of classes and supported assignment aliases between source files supplied in the same invocation.
- Imported names and aliases of the supported TypePact symbols, including `from typepact import rust` and `from typepact import contracts`.
- Literal `None` parameter and return annotations, which emit no resource contract.

Class lifecycle and function access declarations compose independently. Compatible declarations are merged; conflicts include source locations and make coverage incomplete.

## Import resolution

Imports are resolved only among the files explicitly passed to the command. `--root` assigns package-qualified names to files. Without it, each file's stem supplies the logical module name. Every supplied file must have a distinct module name.

The inspector resolves direct absolute imports when the imported module is supplied.
The preview checker discovers `.py` files below its root and applies declaration
resolution to that snapshot. Neither mode imports installed packages or executes
Python source. See [CLI input behavior](cli.md).

## Lifecycle preview subset

| Pattern | Checking behavior |
| --- | --- |
| Direct construction through a recognized resource view | Creates an open abstract resource on normal continuation |
| Local alias assignment and rebinding | Tracks shared objects and detached local names |
| Straight-line synchronous code and ordinary returns | Applies supported effects in source order |
| `if`/`else` joins | Preserves possible lifecycle states rather than assuming every path closes |
| Resolved local functions and resource methods | Applies supported body effects to passed resource objects |
| Close through a Borrowed parameter | Reports TP2001 within supported bodies |
| Unknown effects, recursive calls, unsupported flow | Reports incomplete coverage where encountered |

This table describes implemented patterns, not completion of every phase 0.2
validation case. General exception handling, loops, async, fields and containers,
returned aliases, dynamic dispatch, and full import/re-export semantics remain
outside the qualified checking subset. Consult [limitations](limitations.md).

## Python version support

The Python helper package supports Python 3.11 and later. The analyzer source grammar currently supports Python 3.11, 3.12, and 3.13. Select the target with `--python-version`; the default is 3.11. Target-specific Unicode identifier rules are applied during analysis.

## Input and work limits

The analyzer bounds memory and work per request:

| Limit | Maximum |
| --- | ---: |
| Source bytes per file | 1,000,000 |
| Source bytes per request | 1,000,000 |
| Input files | 10,000 |
| Module-level statements per file | 20,000 |
| Converted AST declarations per request | 50,000 |
| Supplied-module import edges | 50,000 |
| Emitted contracts | 10,000 |
| Expression nesting per statement | 128 |
| Converted expression nodes per statement | 50,000 |

The Rust `AnalysisSession` also bounds retained source to 1,000,000 bytes and periodically resets its incremental database after 1,000 source mutations. See the [Rust API](rust-api.md#incremental-analysis-session).

## More detail

The [inspector implementation guide](inspector.md) contains additional details on the parser, semantic summaries, and incremental session behavior. Known unsupported constructs are listed in [limitations](limitations.md).

# Supported syntax

The inspector handles a deliberately limited set of Python declarations. It parses source with the selected Python grammar, resolves only the supported static relationships, and marks coverage incomplete when a relevant construct falls outside that set.

The declaration sections describe the published inspector. Version 0.2.0 adds
the lifecycle-checking subset below; the [release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md)
records its qualification.

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

For `inspect-contracts`, imports resolve only among the explicitly supplied files. For `check`, the checker discovers `.py` files under `--root` and resolves direct absolute imports plus acyclic named re-exports of supported classes, aliases, and synchronous helper functions. `--root` assigns package-qualified names to files; without it, each file's stem supplies the logical module name. Duplicate module names are reported. Neither mode imports packages or executes Python source.

Relative imports, star imports, dynamic imports, import cycles, and ambiguous
modules remain outside the checking subset or produce visible incomplete
coverage. See [CLI input behavior](cli.md).

## Lifecycle-checking subset

| Pattern | Checking behavior |
| --- | --- |
| Direct construction through a recognized resource view | Creates an open abstract resource on normal continuation |
| Local alias assignment and rebinding | Tracks shared objects and detached local names |
| Straight-line synchronous code and ordinary returns | Applies supported effects in source order |
| `if`/`else` joins | Preserves possible lifecycle states rather than assuming every path closes |
| Resolved local functions and undecorated resource methods | Applies supported body effects to passed resource objects |
| Positional-only, positional-or-keyword, and keyword-only parameters | Validates binding before applying a body; a definitely invalid call ends its normal path with a TP9001 boundary |
| Multiple possible resource receivers | Evaluates each receiver from the same incoming state and joins only normal outcomes |
| Explicit `raise` | Reports an exception boundary and ends the normal path, including when called from a return value, condition, or argument |
| Close through a Borrowed parameter | Reports TP2001 within supported bodies |
| Acyclic named re-exports | Preserves supported resource and callable identities across local modules |
| Decorated methods, custom `__new__`, unknown effects, recursive calls, unsupported flow | Reports incomplete coverage and weakens affected resource state |

Expanded arguments (`*args` and `**kwargs`), values captured in variadic
containers, and omitted defaults that may hold resources remain call-binding
boundaries. Scalar literal defaults can be omitted when applying a body, but
default expressions at definition time still make coverage incomplete.
Executable class-body statements and method decorator expressions also create
definition-time boundaries.

This table describes the published 0.2.0 checking subset, not completion of
every phase 0.2 validation case. General exception handling, loops, async,
fields and containers, returned aliases, dynamic dispatch, relative and star
imports, import cycles, and ambiguous modules remain outside the released
subset. Consult [limitations](limitations.md).

## Unreleased 0.3 candidate behavior

The current worktree extends cleanup checking across supported synchronous exit
paths. These behaviors are implemented and covered by candidate tests, but are
not release-qualified until the maintainer completes the [V03 matrix](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/validation.md)
and release gates.

- `try`/`except`/`else`/`finally`, explicit returns and raises, and ordinary
  `while` flow retain cleanup duties across modeled exits. Unsupported or
  uncertain effects keep coverage incomplete.
- `for` over a finite tuple of resolved names and scalar literals is unrolled
  exactly up to 16 items, using the tuple values captured before the first
  iteration. Unresolved names, other iterators and larger tuples keep an
  incomplete-coverage boundary. Other supported loop paths use up to 16
  fixed-point passes; widening cannot establish cleanup.
- A direct `Owned[T]` return hands an open local duty to the caller after
  `finally` completes. Returning Borrowed, already-closed, or unsupported
  values does not fabricate an ownership handoff.
- `with contextlib.closing(value)` and `async with
  contextlib.aclosing(value)` are recognized only for exact supported imports
  and Python targets 3.11–3.13. They use the checked close-body effects; an
  unknown manager cannot discharge a duty. Target 3.14 is not yet modeled.
- Resolved local coroutine calls retain captured resource aliases. A direct
  `await` applies the supported body summary; an unawaited coroutine does not
  perform cleanup. Cancellation and task scheduling remain visible boundaries
  where the checker cannot prove cleanup.

The close declaration supplies a normal state transition with
`declared-unverified` provenance; it is not runtime proof. A checked source
close body contributes its explicit raising paths before that normal
transition. External or unresolved close behavior remains incomplete.

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

Lifecycle checking has additional conservative flow limits:

| Flow limit | Maximum |
| --- | ---: |
| Nested `if` regions | 128 |
| Possible objects for one local alias at a join | 64 |
| Tracked resource objects in one function or module body | 4,096 |
| Nested call/effect evaluation | 8 |
| Lifecycle statement and expression transfers per analysis request | 100,000 |

The 0.3 candidate also caps loop fixed-point analysis at 16 passes, exact
static tuple iteration at 16 items, and pending coroutine records at 128 per
flow state. Exceeding a bound widens affected state and reports incomplete
coverage. These 0.3 values are candidate defaults, not accepted release
budgets; see the [phase plan](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3-cleanup.md).

When a flow limit is reached, the checker drops the affected exact facts,
weakens reachable resource state, and reports a TP9001 boundary with incomplete
coverage. It does not truncate an alias set and continue as if the remaining
objects were exact. These are the accepted 0.2.0 defaults; the phase plan and
[performance record](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.2/performance.md)
contain their qualification fixtures and measurements.
The request-wide flow limit is split evenly among files; unused work in one file
is not reassigned. An edit in one file therefore cannot shift another file's
budget and invalidate its cached result when that file is otherwise unaffected.

The Rust `AnalysisSession` also bounds retained source to 1,000,000 bytes and periodically resets its incremental database after 1,000 source mutations. See the [Rust API](rust-api.md#incremental-analysis-session).

## More detail

The [inspector implementation guide](inspector.md) contains additional details on the parser, semantic summaries, and incremental session behavior. Known unsupported constructs are listed in [limitations](limitations.md).

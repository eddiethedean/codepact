# Current limitations

Published TypePact 0.2.0 provides declaration inspection and a bounded synchronous
lifecycle checker. The release was qualified for its documented subset; it does
not model every Python lifecycle pattern or establish general resource safety.

The lifecycle checker tracks direct construction, local aliases, ordinary synchronous calls, local helpers, `if`/`else` joins, and acyclic named re-exports from local modules. It can report use after close and closing a `Borrowed` parameter. Decorated methods, including `staticmethod` and `classmethod`, do not have modeled Python binding and produce an incomplete-coverage boundary. Method parameter annotations and function defaults are also outside the modeled summary; their declarations produce incomplete-coverage boundaries. Unsupported call argument expressions weaken tracked resources and produce boundaries. A custom `__new__` allocation also produces a boundary because its effects are not modeled. The checker does not precisely model loops, exception handlers, `finally`, context managers, async suspension, dynamic dispatch, arbitrary fields/containers, or returned resource aliases. Unsupported regions can produce boundaries and weaken tracked state; unmodeled alias and boundary combinations are not proven safe. Recursive calls recover to unknown summaries. Unknown methods and calls do not establish purity. A clean run does not prove leak freedom, cleanup on every exit, or correctness of declared resource models.

Flow work is bounded to 128 nested `if` regions, 64 possible objects for one
local alias at a join, 4,096 tracked objects in one function or module body, and
8 nested call/effect evaluations. Exhaustion produces an incomplete-coverage
boundary and weakens affected facts. The defaults are documented in the
[supported syntax guide](supported-syntax.md#input-and-work-limits); their
measured qualification is recorded in the [0.2.0 release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md).

Repeat close has no general policy and is reported as a coverage boundary rather
than a dedicated double-close error. Project checking scans all `.py` files below
the root; explicit filenames do not limit that scan. Use a narrow root and follow
the [lifecycle tutorial](lifecycle.md).

## Static analysis boundaries

The published 0.1.0 declaration inspector reports incomplete coverage for relevant constructs including:

- String and forward-reference annotations, including `from __future__ import annotations`.
- Relative imports, star imports, import re-export chains, ambiguous supplied modules, and module import cycles.
- Class inheritance, metaclass behavior, non-TypePact class decorators, and decorated functions.
- Computed contract expressions, dynamic decorator arguments, and unresolved resource bases.
- Module-level control flow, nested classes, nested or generic function declarations, and other unsupported declaration shapes.
- Eager named expressions and class-body `global` declarations that could rebind module names.
- Resource or access contracts nested inside unsupported container annotation forms.
- Unsupported Python source target versions.

The [supported syntax guide](supported-syntax.md) describes both modes. The
checker handles some module statements that the inspector intentionally leaves
outside its declaration-only scope. Incomplete inspection causes exit `1`.
Incomplete checking and possible findings alone can accompany exit `0`; definite
violations and declaration errors cause exit `1`. Always read coverage reasons
and diagnostic certainty. An unsupported construct does not imply invalid Python.

## Declared models and runtime behavior

The inspector does not inspect function bodies to check whether a resource is closed, whether a value is used after closing, whether all paths clean up, or whether ownership and exclusive access are obeyed. The `resource` decorator only records metadata on the class. `Owned`, `Borrowed`, and `BorrowedMut` express intent; they do not create Rust-style ownership, borrowing, uniqueness, or runtime checks.

Every fact is marked `declared-unverified`. Complete coverage means the supported declaration subset was analyzed, not that resource safety was established.

For lifecycle checking, complete coverage refers to the supported normal-flow
subset under those declared models. The designated `close` method supplies a
normal state transition even when its illustrative body is empty. It does not
certify that the runtime method releases a resource. Borrowed checks do not
provide general lifetime or escape proofs.

The current, unreleased 0.3 worktree implements a bounded exit-flow candidate:
it tracks `finally`, supported exception handlers, `while`, finite tuple `for`
loops, Owned return handoff, exact imported, module-qualified, and re-exported
`contextlib.closing`/`aclosing` identities, and
resolved coroutine calls that are directly awaited. A checked source close
body contributes explicit raising paths; only its normal completion receives
the declared close transition. The declaration remains `declared-unverified`.
Loop fixed-point analysis and static tuple iteration are capped at 16 passes
and 16 items respectively; each flow state retains at most 128 pending
coroutine records. Unknown iterators, external close effects,
unrecognized managers, repeated coroutine creation/await, task scheduling,
unsupported escapes and exhausted budgets can leave coverage incomplete.
Source-defined `__del__` methods are not assumed to run or to complete cleanup;
an outstanding duty remains possible with an explicit incomplete-coverage
boundary, even when the finalizer calls the declared closer. Inheritance is not
resolved for finalizer effects, so an outstanding duty on a resource class
with a non-`object` base also remains possible with an incomplete-coverage
boundary.
Async-generator finalization and general task lifetime analysis remain outside
this candidate. See [supported syntax](supported-syntax.md#unreleased-03-candidate-behavior)
and the [0.3 validation map](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/validation.md);
this candidate description does not qualify the release.

## Provisional interfaces

The Python helper API, CLI options, diagnostic set, and JSON schema may change during the 0.x series. JSON currently reports schema version 0. Diagnostic wording can improve, and new diagnostics can affect CI workflows. Pin compatible package versions and review release notes when upgrading.

See the [roadmap](roadmap.md) for planned phases and release acceptance gates.
Planned work does not imply a release date.

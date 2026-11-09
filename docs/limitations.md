# Current limitations

The published TypePact 0.2.0 release provided declaration inspection and a
bounded synchronous lifecycle checker. CodePact 0.3.0 adds cleanup checks across
supported exits. Its phase qualification is recorded in the release evidence.
Neither release models every Python lifecycle pattern or establishes
general resource safety.

The published 0.2 checker tracks direct construction, local aliases, ordinary synchronous calls, local helpers, `if`/`else` joins, and acyclic named re-exports from local modules. CodePact 0.3 adds bounded cleanup across the supported exits described below. In CodePact 0.3.0, decorated methods, including `staticmethod` and `classmethod`, still lack modeled Python binding and produce an incomplete-coverage boundary. Method parameter annotations and function defaults are outside the modeled summary; their declarations produce incomplete-coverage boundaries. Unsupported call argument expressions weaken tracked resources and produce boundaries. A custom `__new__` allocation also produces a boundary because its effects are not modeled. Dynamic dispatch and arbitrary field/container ownership remain unsupported. Unsupported regions can produce boundaries and weaken tracked state; unmodeled alias and boundary combinations are not proven safe. Recursive calls recover to unknown summaries. Unknown methods and calls do not establish purity. A clean run does not prove leak freedom, runtime leak freedom, or correctness of declared resource models.

CodePact 0.3.0 caps total nested control-flow depth at 64 (the historical
0.2 limit was 128 nested `if` regions), possible objects for one local alias
at a join at 64, tracked objects in one function or module body at 4,096,
nested call/effect evaluations at 8, loop passes at 16, context-exit partitions
at 256, and pending coroutine records at 128. Exhaustion produces an
incomplete-coverage boundary and weakens affected facts. See the
[supported syntax guide](supported-syntax.md#input-and-work-limits); the 0.3
measurements include the released correctness fixes. The accountable
maintainer reviewed and accepted them for G4 on 2026-11-08; results and scope
are in the [0.3 performance record](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/performance.md).

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

CodePact 0.3.0 implements bounded exit-flow analysis:
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
Module-scope open duties and resource values stored in `global` bindings
receive explicit lifetime/escape boundaries; module completion is not treated
as interpreter shutdown or automatic cleanup. Assignments through `nonlocal`
remain outside the modeled subset because they require nested function
declarations. Fully closed module-scope resources do not receive that lifetime
boundary.
Async-generator finalization and general task lifetime analysis remain outside
the supported subset. See [supported syntax](supported-syntax.md#codepact-030-cleanup-behavior)
and the [0.3 validation map](https://github.com/eddiethedean/codepact/blob/main/docs/planning/phases/0.3/validation.md).

## Provisional interfaces

The Python helper API, CLI options, diagnostic set, and JSON schema may change during the 0.x series. JSON currently reports schema version 0. Diagnostic wording can improve, and new diagnostics can affect CI workflows. Pin compatible package versions and review release notes when upgrading.

See the [roadmap](roadmap.md) for planned phases and release acceptance gates.
Planned work does not imply a release date.

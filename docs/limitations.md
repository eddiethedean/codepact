# Current limitations

Published TypePact 0.1.0 provides contract inspection. The source checkout adds
an unreleased `check` command for a bounded synchronous lifecycle subset. Phase
0.2 qualification remains pending; green repository checks do not establish that
every required lifecycle validation case has passed.

The lifecycle preview tracks direct construction, local aliases, ordinary synchronous calls, local helpers and `if`/`else` joins. It can report use after close and closing a `Borrowed` parameter. It does not precisely model loops, exception handlers, `finally`, context managers, async suspension, dynamic dispatch, arbitrary fields/containers, or returned resource aliases. Unsupported regions can produce boundaries and weaken tracked state; qualification for every alias and boundary combination remains pending. Recursive calls recover to unknown summaries. Unknown methods and calls do not establish purity. A clean run does not prove leak freedom, cleanup on every exit, or correctness of declared resource models.

Repeat close has no general policy and is reported as a coverage boundary rather
than a dedicated double-close error. Project checking scans all `.py` files below
the root; explicit filenames do not limit that scan. Use a narrow root and follow
the [lifecycle tutorial](lifecycle.md).

## Static analysis boundaries

The declaration inspector reports incomplete coverage for relevant constructs including:

- String and forward-reference annotations, including `from __future__ import annotations`.
- Relative imports, star imports, import re-export chains, ambiguous supplied modules, and module import cycles.
- Class inheritance, metaclass behavior, non-TypePact class decorators, and decorated functions.
- Computed contract expressions, dynamic decorator arguments, and unresolved resource bases.
- Module-level control flow, nested classes, nested or generic function declarations, and other unsupported declaration shapes.
- Eager named expressions and class-body `global` declarations that could rebind module names.
- Resource or access contracts nested inside unsupported container annotation forms.
- Unsupported Python source target versions.

The [supported syntax guide](supported-syntax.md) describes both modes. The preview
checker handles some module statements that the inspector intentionally leaves
outside its declaration-only scope. Incomplete inspection causes exit `1`.
Incomplete checking and possible findings alone can accompany exit `0`; definite
violations and declaration errors cause exit `1`. Always read coverage reasons
and diagnostic certainty. An unsupported construct does not imply invalid Python.

## Declared models and runtime behavior

The inspector does not inspect function bodies to check whether a resource is closed, whether a value is used after closing, whether all paths clean up, or whether ownership and exclusive access are obeyed. The `resource` decorator only records metadata on the class. `Owned`, `Borrowed`, and `BorrowedMut` express intent; they do not create Rust-style ownership, borrowing, uniqueness, or runtime checks.

Every fact is marked `declared-unverified`. Complete coverage means the supported declaration subset was analyzed, not that resource safety was established.

For preview checking, complete coverage refers to the supported normal-flow
subset under those declared models. The designated `close` method supplies a
normal state transition even when its illustrative body is empty. It does not
certify that the runtime method releases a resource. Borrowed checks do not
provide general lifetime or escape proofs.

## Provisional interfaces

The Python helper API, CLI options, diagnostic set, and JSON schema may change during the 0.x series. JSON currently reports schema version 0. Diagnostic wording can improve, and new diagnostics can affect CI workflows. Pin compatible package versions and review release notes when upgrading.

See the [roadmap](roadmap.md) for planned phases and release acceptance gates.
Planned work does not imply a release date.

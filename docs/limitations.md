# Current limitations

The published TypePact 0.1 line is a contract-inspection release. The current working tree adds an unreleased `check` command for a bounded synchronous lifecycle subset. It records supported declarations and reports where analysis cannot establish complete coverage. Neither mode proves that resources are managed safely in arbitrary Python programs.

The lifecycle preview tracks direct construction, local aliases, ordinary synchronous calls, local helpers and `if`/`else` joins. It can report use after close and closing a `Borrowed` parameter. It does not model loops, exception handlers, `finally`, context managers, async suspension, dynamic dispatch, arbitrary fields/containers, or returned resource aliases. Those constructs weaken affected facts and appear as incomplete coverage. Unknown methods and calls do not imply purity. A clean run does not prove leak freedom or runtime cleanup.

## Static analysis boundaries

The analyzer currently reports incomplete coverage for constructs including:

- String and forward-reference annotations, including `from __future__ import annotations`.
- Relative imports, star imports, import re-export chains, ambiguous supplied modules, and module import cycles.
- Class inheritance, metaclass behavior, non-TypePact class decorators, and decorated functions.
- Computed contract expressions, dynamic decorator arguments, and unresolved resource bases.
- Module-level control flow, nested classes, nested or generic function declarations, and other unsupported declaration shapes.
- Eager named expressions and class-body `global` declarations that could rebind module names.
- Resource or access contracts nested inside unsupported container annotation forms.
- Unsupported Python source target versions.

The [supported syntax guide](supported-syntax.md) describes the subset TypePact recognizes. If a construct is outside that subset, a status of 1 and `coverage.status` of `incomplete` are expected outcomes, not proof that the source is invalid Python.

## No flow or runtime enforcement

The inspector does not inspect function bodies to check whether a resource is closed, whether a value is used after closing, whether all paths clean up, or whether ownership and exclusive access are obeyed. The `resource` decorator only records metadata on the class. `Owned`, `Borrowed`, and `BorrowedMut` express intent; they do not create Rust-style ownership, borrowing, uniqueness, or runtime checks.

Every fact is marked `declared-unverified`. Complete coverage means the supported declaration subset was analyzed, not that resource safety was established.

## Provisional interfaces

The Python helper API, CLI options, diagnostic set, and JSON schema may change during the 0.x series. JSON currently reports schema version 0. Diagnostic wording can improve, and new diagnostics can affect CI workflows. Pin compatible package versions and review release notes when upgrading.

See the [roadmap](roadmap.md) for planned phases. Planned work does not imply a release date or a feature commitment beyond the current 0.1 scope.

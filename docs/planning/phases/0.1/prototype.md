# 0.1 prototype: canonical contract inspection

Status: proposed acceptance specification. The current prototype implements portions of this interface; Ruff/Salsa migration and the expanded semantics remain requirements, not completed functionality.

## Developer walkthrough

Installing typepact installs its pure Python helpers and resolves the exactly matching typepact-cli dependency. The developer creates an ordinary Python module containing a resource declaration and access-annotated parameter, then inspects it:

~~~console
python -m pip install typepact==0.1.0
# The wheel already installs the native typepact command.
typepact inspect-contracts examples/contracts.py
~~~

The Rust package must expose a usable library inspection entry point, plus the typepact binary. Exact Rust function/type names are settled in WP03; callers need to supply source text, a logical file path and a target Python version without initializing a Python interpreter.

The command accepts an explicit file and optional --python-version. Project discovery and pyproject analysis configuration arrive later. Version/help output is required. No typepact check safety command is shipped in this phase.

## Pythonic input

~~~python
from typepact import resource
from typepact.rust import Borrowed

@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass

def inspect(conn: Borrowed[Connection]) -> None:
    pass
~~~

The decorator records a declared lifecycle contract on Connection. The parameter adds shared access intent and links to that resource contract. Method implementations are not verified in 0.1, so their contract provenance remains declared/unverified.

## Explicit lifecycle input

~~~python
from typing import Annotated
from typepact.contracts import Resource

class Connection:
    def close(self) -> None:
        pass

ConnectionResource = Annotated[Connection, Resource(closed_by="close")]
~~~

This provisional low-level alias carries lifecycle metadata for the resolved base class. Its contract applies to uses of the alias; it must not silently change every occurrence of bare Connection globally. The inspector emits a contract view with a base symbol and lifecycle dimension. The Pythonic declaration supplies the class default view. Equivalent views normalize equally when their explicit fields match.

WP01 must validate that this spelling is understood by ordinary checkers as Connection. If another spelling is chosen, update examples, fixtures and accepted decisions together.

Borrowed[Connection] and Annotated[Connection, SharedBorrow()] are the proposed pair for shared-access equivalence, with SharedBorrow provisionally exported from typepact.contracts. Owned and Borrowed alone contain no lifecycle transition information.

## Composition and conflicts

A borrowed parameter can combine the class's default resource contract with a shared-access dimension. Resource metadata on an alias can also be composed with access metadata for that alias. Lower each source independently, then merge compatible dimensions in one engine.

Contradictory lifecycle metadata or multiple incompatible access qualifiers must yield a declaration error with both declaration locations. Do not select metadata based on import/decorator order. Unrecognized metadata remains opaque and does not acquire TypePact meaning.

## Canonical fields

| Field | 0.1 meaning |
| --- | --- |
| schema_version | 0 for the provisional inspection envelope |
| assurance | contract-inspection-only |
| base_symbol | Resolved module/class identity, separate from display name |
| lifecycle | Optional declared closed_by method; normalize absent versus specified fields consistently |
| access | unspecified, owned, shared or exclusive intent; enforcement deferred |
| source | File path and documented span coordinates |
| provenance | Origin API and declared/unverified status |
| coverage | complete-for-supported-subset or incomplete, with reason and location |
| diagnostics | Declaration, syntax and unsupported-boundary findings |

Exclusive metadata may be recognized as intent if the selected API includes it; no 0.1 gate requires exposing BorrowedMut publicly. Required access surface is Owned and Borrowed.

WP03 must freeze a concrete JSON example and coordinate convention before WP04 snapshots are accepted. Unknown or unsupported constructs must not be represented as valid fully resolved contracts.

For equivalence tests, compare base-symbol identities after an explicit fixture module-name mapping, lifecycle and access fields. Exclude only source locations and API-origin provenance. Preserve declared/unverified trust status, unresolved facts and uncertainty. Never discard semantically relevant differences to force equality.

## Supported resolution subset

Support direct imports and imported-name aliases from known TypePact modules, typing.Annotated, module-level class declarations, class decorators, simple type aliases and parameter/return annotations in the chosen syntax versions. Resolve local shadowing before recognizing API symbols.

Direct imports among explicitly provided local modules are required for the 0.1 query/dependency fixtures. Resolve them without executing modules. Bound traversal to supplied fixture/project roots; report unresolved modules. Only the TypePact `resource` class decorator with a literal `closed_by` value is supported; other class or function decorators, dynamic decorator arguments, star imports, arbitrary runtime factories, metaclass-generated types and computed annotation expressions are unsupported. Eager named expressions in declaration headers and `from __future__ import annotations` also produce incomplete coverage. When unsupported constructs could rebind module names, discard the affected module binding view so later declarations cannot reuse stale symbols.

Do not resolve contracts by identifier spelling alone. A local function named resource or a class named Borrowed must remain ordinary Python.

## Inspection outcomes

| Exit | Meaning |
| --- | --- |
| 0 | All supported declarations inspected; no declaration errors or unresolved relevant boundaries |
| 1 | Inspection completed with declaration errors or relevant unsupported/unresolved constructs |
| 2 | Invocation, file access, configuration or fatal parser/engine failure prevented inspection |

Recoverable syntax errors produce diagnostics and exit 1 with incomplete coverage; unrecoverable parse failure produces exit 2. Document which parser failures are recoverable. JSON coverage and assurance remain present on every structured result; invocation failures can use stderr.

A 0 exit code means successful declaration inspection only. It does not establish that method bodies honor declarations or that resource use is safe.

Order contracts by file/span and diagnostics by file/span/code. Output must be identical across repeated runs on the same inputs and options. No absolute machine-specific temporary path belongs in canonical snapshots.

## Execution and resource limits

Read Python as text. Never import it, evaluate annotation strings or invoke decorators/build hooks during inspection. Limit source bytes, import traversal depth and declaration count with documented defaults selected in WP05. Exceeding a budget must terminate with an explicit incomplete result, not a success claim.

Build hooks used to build TypePact itself are separate from inspection of user programs. Keep fixture files with side effects outside any import-driven test discovery.

## Frontend and session contract

Use the [migration plan](../../analyzer-migration.md) for Ruff AST traversal, binding-event semantics, captured declaration identities and Salsa queries. Multiline declarations, ordinary/triple-quoted strings, comments and calls in function bodies must not change which supported annotations are inspected. Resolve eager alias right-hand sides before rebinding their left-hand names. Earlier views retain their original class and metadata after class/import/function rebinding. Deferred/lazy annotations require explicit supported semantics or incomplete coverage.

A reusable session accepts file and settings updates and returns revision-associated inspection results. The one-shot API wraps it. Updated and fresh sessions must agree on facts, diagnostics and coverage. No-op requests reuse tracked work; unrelated edits do not rerun unrelated semantic queries. Do not expose database-local IDs as persistent public identity.

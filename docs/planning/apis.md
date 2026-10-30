# Interoperable API design

All signatures in this document are proposed. The canonical contract model in [the analyzer plan](semantic-analyzer.md) determines meaning.

## Public surfaces

| Capability | Pythonic expression | Explicit expression | Relationship |
| --- | --- | --- | --- |
| Resource lifecycle | `@resource(closed_by="close")` | `Owned[T]`, `Borrowed[T]` at boundaries | Lifecycle and ownership are separate contract dimensions |
| Shared access | Non-consuming parameter contract | `Borrowed[T]` | Same access contract when explicitly established |
| Exclusive access | Scoped exclusive contract, spelling deferred | `BorrowedMut[T]` | Same exclusivity obligation |
| Return usage | `@must_use` | `MustUse[T]` | Same result-use obligation |
| State | `@transition` | `State[T, S]` | Shared state machine, syntax provisional |
| Structural interface | `typing.Protocol` | Trait adapter, research | Reuse structural typing where semantics match |
| Absence | `T | None` | `Option[T]` | Different representations; conversion required |
| Failure | Exceptions | `Result[T, E]` | Different control flow; conversion required |

Ownership metadata should use standard Annotated-compatible aliases when the compatibility prototype proves they work on supported checkers. Do not implement subscriptions solely with arbitrary runtime magic that ordinary type checkers cannot interpret. Use typed aliases/stubs, and retain a verbose Annotated form as the fallback design.

## First vertical slice

~~~python
from typepact import resource
from typepact.rust import Borrowed

@resource(closed_by="close")
class Connection:
    def status(self) -> str:
        return "ready"

    def close(self) -> None:
        pass

def inspect(conn: Borrowed[Connection]) -> str:
    return conn.status()

conn = Connection()
inspect(conn)       # accepted in the supported subset
conn.close()
inspect(conn)       # TP1001: use of closed resource
~~~

The decorator declares a contract; the no-op close body is illustrative. Real implementations must fulfill their declared lifecycle behavior. The analyzer must verify supported bodies or mark externally supplied summaries as trusted.

Initial resource state is open after successful construction. Close changes state on normal return. An exceptional close leaves state unknown unless an explicit summary defines it. Methods require open state by default, with explicit allowances for close and approved post-close queries. Double close is an error only for contracts declared non-idempotent; repeat-close policy must be configured in the resource model.

Borrowed parameters cannot consume or close the object. Whether methods mutate it must be represented by method summaries. Absence of such a summary cannot imply purity.

## Ownership and moves

Proposed syntax: `consume(move(value))`, where consume takes `Owned[T]`. The helper returns the same Python object and expresses a transfer for analysis. Once transfer occurs, the caller's tracked owned handle is invalid. Transfer at argument evaluation persists even if the callee raises. A move never automatically closes or deallocates an object.

Ordinary assignment retains Python aliasing semantics. It must not silently become a Rust move. The analyzer tracks aliases, rejects a transfer when known live aliases violate the contract, and reports uncertainty for unresolved aliases. An Owned annotation alone cannot manufacture evidence of uniqueness.

Borrowed views cannot escape into globals, containers, returned closures or unmodeled callbacks within the initial checked subset. Mutable borrowing requires exclusive access to the tracked region. Precise lifetime parameters are deferred until region inference is viable.

## Result and Option

Proposed runtime values: `Ok(value)`, `Err(error)`, `Some(value)`, and `Nothing()`. Public types are typed unions of ordinary variant classes, with generic variance selected after mutation rules are settled. Payloads are exposed consistently for pattern matching. Classes may be immutable while their payloads remain mutable.

~~~python
from typepact.rust import Result, Ok, Err

def parse_count(text: str) -> Result[int, ValueError]:
    try:
        return Ok(int(text))
    except ValueError as error:
        return Err(error)
~~~

Result does not prevent exceptions outside the declared conversion. Exception adapters catch only explicitly named exception classes; cancellation and process-control exceptions must propagate by default. Option conversion from nullable values is explicit and lossy for distinguishing Some(None) from absence; document this in adapter signatures.

Result-to-exception conversion requires an explicit error mapping. No automatic exception swallowing, implicit unwrapping, or Rust question-mark syntax is proposed.

## Runtime contract

Metadata decorators must preserve object identity, signatures, inheritance behavior and descriptors where possible, without wrapping every method. Runtime enforcement is a separate opt-in capability with its own overhead and limitations. Metadata use must not load the native analyzer.

Conflicting metadata is a declaration error; frontend order must not silently choose a winner. Imported aliases and re-exports resolve by symbol identity, not spelling alone. A user-defined function named resource must not receive Typepact semantics.

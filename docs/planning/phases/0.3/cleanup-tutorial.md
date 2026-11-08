# Preview 0.3 cleanup checking

This tutorial exercises the unreleased phase 0.3 candidate from a source
checkout. It is not a claim that 0.3 is release-qualified. The analyzer reads
these files as source and never imports or executes the example program.

The supported declaration says which method represents a successful close:

```python
from codepact import resource

@resource(closed_by="close")
class Connection:
    def close(self):
        pass
```

The declaration gives the analyzer a normal close transition with
`declared-unverified` trust. It does not prove the method releases a runtime
resource.

## Synchronous exits

Run the sync example with the checkout's Rust CLI:

```sh
cargo run --locked -- check --python-version 3.11 \
  --root docs/planning/phases/0.3/examples/sync
```

[`examples/sync/cleanup.py`](examples/sync/cleanup.py) contains one leaking
function and one `finally`-cleaned function. The expected finding is one
definite TP1002 on the early return, related to its acquisition. The command
exits `1` because a definite violation exists. Removing `leak` makes this
fixture exit `0` with complete coverage.

## Async cleanup and cancellation

Run the async example with the same target:

```sh
cargo run --locked -- check --python-version 3.11 \
  --root docs/planning/phases/0.3/examples/async
```

[`examples/async/cleanup.py`](examples/async/cleanup.py) compares direct
`await resource.aclose()` with the reviewed `contextlib.aclosing` model. Both
attempt the same checked cleanup. Each retains a possible TP1002 because
cancellation may interrupt the awaited close; coverage explains that boundary.
Possible findings and incomplete coverage alone exit `0`. The model is enabled
only for source targets 3.11–3.13; the current catalog deliberately leaves
3.14 unsupported.

## Interpreting the result

Read certainty, completion kind, trust, primary and related locations, and
coverage together. A clean supported-subset result is not proof of leak
freedom. Unknown managers, iterators, escaped resources, repeated close, task
scheduling, and exhausted budgets do not establish cleanup. The
[specification](specification.md) and [validation matrix](validation.md) define
the current boundaries and the evidence still needed for release qualification.

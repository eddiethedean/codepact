# Experimental ownership

CodePact 0.5 adds explicit moves, lexical borrows, and a bounded model for
`asyncio.TaskGroup`. [Install matching helpers and CLI](install.md)

## Enable ownership checks

The rules are opt-in. Pass `--experimental-ownership` for a single check, or
add the following project settings.

```toml
[tool.codepact]
config-version = 1
experimental-ownership = true
```

`--no-experimental-ownership` overrides a project opt-in. Both options also
apply to `baseline update`. Strict mode fails when the selected analysis reports
incomplete coverage. Choose the experimental scope separately.

## Transfer a resource and its cleanup duty

Create a `demo/` directory in the project you want to check and save this
example as `demo/session.py`. Run the matching CLI from that project.

```sh
codepact check --root demo --experimental-ownership
```

For a uv workspace, prefix the command with `uv run --locked`.

```python
from codepact import move, resource
from codepact.rust import Owned


@resource(closed_by="close")
class Session:
    def read(self) -> str:
        return "ready"

    def close(self) -> None:
        pass


def read_and_close(session: Owned[Session]) -> None:
    try:
        session.read()
    finally:
        session.close()


def run() -> None:
    source = Session()
    read_and_close(move(source))
    source.read()
```

The last read through `source` reports TP2002 with a related location at the
move. Remove that read and rerun the same command. The cleanup duty passed to
`read_and_close`, where `finally` closes the resource. A raise after evaluation
of the move leaves the source binding moved. If later argument evaluation
fails before the callee takes the resource, check the reported outstanding duty.

`receiver = move(source)` transfers the same resource to another local handle.
`alias = source` creates a regular Python alias and can block a subsequent move.
The helper preserves object identity at runtime. Its type signature returns the
same type it accepts.

## Borrow a resource during a task group

A checked `Borrowed[Session]` parameter can read through a shared view. Keep the
view inside the verified call or capture region and let the owner close the
resource. Returning it or storing it in a container reports TP2004.

To use the TaskGroup model, set both options in the same
`[tool.codepact]` table.

```toml
[tool.codepact]
config-version = 1
experimental-ownership = true
assume-default-task-factory = true
```

Append the following code to the repaired `session.py`.

```python
from asyncio import TaskGroup

from codepact.rust import Borrowed


async def read(session: Borrowed[Session]) -> None:
    session.read()


async def read_in_group() -> None:
    session = Session()
    try:
        async with TaskGroup() as group:
            group.create_task(read(session))
    finally:
        session.close()
```

Add a premature `session.close()` after `create_task` inside the group body
to reproduce TP2006. Remove that close and keep the `finally` cleanup to repair
the example. The model tracks
completion on the group exit, including paths that propagate an exception or
cancellation. A call to `task.cancel()` requests cancellation. Keep the
resource open until the group joins the task.

An owning move into a task commits during argument evaluation. A task cancelled
before it starts cannot run the worker's cleanup code. Task completion and
resource cleanup are reported as separate effects.

## Advanced contract preview (0.6)

Advanced contracts are a separate opt-in layered on experimental ownership.
Enable both settings (or pass both command-line switches):

```toml
[tool.codepact]
config-version = 1
experimental-ownership = true
experimental-advanced-contracts = true
```

```sh
codepact check --experimental-ownership --experimental-advanced-contracts
```

The preview enables TP2101 for known exclusive-access conflicts and TP2102
for known protocol precondition violations. The initial supported exclusive
region is a resolved synchronous local call with a `BorrowedMut[T]` parameter.
It checks argument alias overlap, modeled accesses through competing aliases,
known captures, and suspension at `await`, async context entry/exit, or `yield`.
An unsupported alias or effect weakens only the affected facts and adds a
coverage boundary. Nested exclusive regions and general lifetime relations are
not modeled.

Finite protocol declarations use exact CodePact imports and literal state
names. A successful local construction starts in the declared initial state.
Transitions can declare normal, exceptional, and cancelled outcomes; a
receiver-only `State[T, Literal[...]]` annotation (or equivalent
`Annotated[T, StateAccess[Literal[...]]]`) supplies a precondition without a
transition.

```python
from typing import Literal

from codepact import state_machine, transition
from codepact.rust import State


@state_machine(states=("ready", "closed"), initial="ready")
class Channel:
    @transition(from_state="ready", to_state="closed", on_raise="ready")
    def close(self) -> None:
        pass

    def send(self: State["Channel", Literal["ready"]]) -> None:
        pass


channel = Channel()
channel.close()
channel.send()  # TP2102: close established the closed state
```

These declarations are trusted assertions in this preview: CodePact checks
their names and state sets, but does not prove a method body implements its
declared transition. An omitted exceptional or cancelled outcome becomes
unknown on that exit. Unknown calls, unresolved imports, and unsupported
effects keep protocol state unknown. A protocol state named `closed` does not
declare resource cleanup.

The experimental TaskGroup extension accepts a local binding such as
`group = asyncio.TaskGroup()` followed by `async with group`, in addition to
the existing inline form. It reports known submissions before entry, after
exit, and repeated entry as TP2102. Both ownership and
`assume-default-task-factory = true` are required for its completion model.

The first ThreadPoolExecutor subset is narrower: exact
`concurrent.futures.ThreadPoolExecutor`, a local no-argument construction,
resolved `submit` workers with no tracked resource arguments or captures, and
`shutdown` with default or literal arguments. It tracks `Accepting` and
`Shutdown`, and reports submit after shutdown. Its model is pinned to CPython
3.13.16 source; other targets produce a boundary. Worker execution, futures,
worker-side effects, and cleanup are not inferred. The report lists the
experimental model and pin.

Run examples with `--mode strict` to fail on incomplete coverage.

## Scope and report migration

- The checker tracks local handles and bounded, resolved source calls. Native
  calls, dynamic capture and task handoffs can leave incomplete coverage.
- The TaskGroup model covers direct calls to a tracked group's `create_task`
  with one resolved coroutine function. Its source pins are CPython 3.11.15,
  3.12.13 and 3.13.16. The project assumption covers the running default
  CPython loop and its non-eager task factory. With the 0.8 runtime profile
  selected, the checker can also model scoped task creation, direct task
  awaits, shielding, and timeouts. Custom factories, detached tasks, and
  escaping group or task handles can leave coverage incomplete.
  [Reviewed asyncio profile](reviewed-contracts.md#select-the-runtime-profile)
- The report's `assurance` identifies the ownership scope. `available_models`
  gives each selected TaskGroup or advanced executor model pin and assumption.
- Baselines record the ownership scope, model catalog, and selected concurrent
  model revisions. Generate a new baseline under the desired scope after
  reviewing its findings.

| Rule | Meaning |
| --- | --- |
| TP2002 | Use of a moved handle, including a second move |
| TP2003 | Transfer without a valid unique owner or with a conflicting alias or borrow |
| TP2004 | Consumption or escape of a checked borrowed view |
| TP2005 | A tracked resource reached an `Owned` parameter without an explicit move |
| TP2006 | Cleanup while a tracked coroutine or task borrow remains outstanding |

Use `codepact explain RULE --json` for the rule's source-level guidance. Read
certainty and coverage alongside the finding before deciding how to repair it.

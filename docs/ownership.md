# Experimental ownership in the 0.5 candidate

The 0.5 development candidate adds explicit moves, lexical borrows, and a
bounded model for `asyncio.TaskGroup`. [Build matching 0.5 helpers and CLI](install.md#try-the-05-development-candidate)

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

## Scope and report migration

- The checker tracks local handles and bounded, resolved source calls. Native
  calls, dynamic capture and task handoffs can leave incomplete coverage.
- The TaskGroup model covers direct calls to a tracked group's `create_task`
  with one resolved coroutine function. Its source pins are CPython 3.11.15,
  3.12.13 and 3.13.16. The project assumption covers the running default
  CPython loop and its non-eager task factory. Custom factories, detached tasks,
  direct task joins, and escaping group or task handles remain boundaries.
- The report's `assurance` identifies the ownership scope. When the TaskGroup
  model is selected, `available_models` gives the pin and assumption.
- Baselines record the ownership scope and selected TaskGroup model revision.
  Generate a new baseline under the desired scope after reviewing its findings.

| Rule | Meaning |
| --- | --- |
| TP2002 | Use of a moved handle, including a second move |
| TP2003 | Transfer without a valid unique owner or with a conflicting alias or borrow |
| TP2004 | Consumption or escape of a checked borrowed view |
| TP2005 | A tracked resource reached an `Owned` parameter without an explicit move |
| TP2006 | Cleanup while a tracked coroutine or task borrow remains outstanding |

Use `codepact explain RULE --json` for the rule's source-level guidance. Read
certainty and coverage alongside the finding before deciding how to repair it.

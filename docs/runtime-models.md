<!-- release-recipes -->

# Configure reviewed runtime models

## Select the runtime profile

Set the source target and analyzed runtime in `pyproject.toml`. Apply
this profile when the application runs under the stated CPython version,
platform, and assumptions. The checker can run on a different host.
These examples use the published 0.10.0 CLI.

```{literalinclude} _examples/work/pyproject.toml
:language: toml
```

The file model checks ordinary local text paths opened in default mode,
`"r"`, or `"w"`, with supported literal arguments and the default opener.
Both `with open(...)` and `with handle` close the tracked file for all aliases
on successful context exit. Explicit close has the same alias effect.

With the profile above, save this example as `src/read_settings.py` and run
`codepact check --root src`.

```python
def read_settings():
    with open("settings.txt", "r") as handle:
        handle.read()
    handle.read()  # TP1001
```

CodePact reports TP1001 on the second read and relates it to the context
exit. Remove the second read to clear TP1001 and pass in the default advisory
mode. Strict mode can still flag incomplete coverage on the exceptional
cleanup path. The analyzer checks the Python source without opening
`settings.txt`.

### Asyncio and multiprocessing

Ownership mode adds local `asyncio.create_task`, direct task await,
`asyncio.shield(task)` and `asyncio.timeout` in the default event loop.
Advanced mode adds local Process and Pool lifecycle checks with an explicit
`multiprocessing.get_context("spawn")` selection. Join proofs require ordinary
child reaping and signal handling in the analyzed runtime, including the joins
performed by a Pool context exit.
An independent termination check can establish that a started child has
exited after a timed or unqualified wait.

Task cancellation can leave cleanup pending. A task's completion remains
observable after its TaskGroup exits. A timeout converts cancellation from
its own deadline, and a shielded timeout can leave its underlying task live.
Forced Pool shutdown can abandon admitted work.
Keep worker resources local and send plain values across the process
boundary. Alternate start methods, live-handle serialization, escaping
identities and unresolved runtime settings produce coverage boundaries.

Select dependency roots, project models, or a runtime profile to enable
foundations checks. Set `experimental-foundations = true` to request the
foundations report explicitly. These checks emit CLI JSON report schema 4
with model applicability, artifact digests, source scope, and exhausted limits.
Rust hosts can supply the equivalent inputs through `check_with_foundations`.
[Output schemas](schemas.md) · [Rust integration](rust-api.md#check-with-reviewed-models)

### Work result observation

With the runtime profile above, CodePact checks result or exception observation
for supported local asyncio tasks and futures, explicit-spawn Pool jobs, and
local thread-pool jobs. An await, `result()`, or `get()` can observe work in the
supported profile. Checking status or waiting for shutdown leaves the result
obligation pending. When `exception()` reports a successful worker with no
exception, the result still needs to be observed. A delivered `MustHandle`
outcome has a separate acknowledgment duty.
[Rule catalog](rules.md) · [Persistent outcomes](contracts.md#acknowledge-a-persistent-outcome)

### Try result observation and outcome handling

This example distinguishes an unobserved task result from a delivered
`MustHandle` outcome. If the task delivers an outcome, the caller still needs
to acknowledge it.

```{raw} html
<p><a href="_static/codepact-work.zip" download>Download the work and outcomes example</a></p>
```

Extract the project and run the published checker from its directory.

```sh
uv tool run --from codepact-cli==0.10.0 codepact check --json > report.json
```

```{literalinclude} _examples/work/src/main.py
:language: python
:linenos:
```

| Function | What to look for |
| --- | --- |
| `check_status` | `TP3003` at task creation. Calling `done()` reads status and leaves result observation outstanding. The live task can also leave coverage incomplete. |
| `observe_only` | The await establishes result observation. `TP5002` identifies the unacknowledged delivered outcome. |
| `acknowledge` | The await observes the task, and `discard_outcome` records the deliberate waiver as `TP5005`. Use a supported variant match when the payload is needed. |

Read the diagnostic's primary line and `foundations.model_uses` together.
`await-local-task` shows retrieval of the result. See
[acknowledging persistent outcomes](contracts.md#acknowledge-a-persistent-outcome)
for variant matches and forwarding. [Queue, IPC, and guard models](concurrency.md)

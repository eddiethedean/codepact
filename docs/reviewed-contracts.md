# Reviewed contracts and runtime models

CodePact 0.9.0 checks contracts from installed pure Python wheels. Package
authors can publish a manifest, and application teams can supply reviewed
contracts for an exact dependency version. The opt-in standard library
models target CPython 3.13.16 on Linux x86-64. Reports identify the selected
artifacts, runtime assumptions, and effects the checker could apply.

## Publish and verify a provider

Place an empty `py.codepact` file in a regular Python package. Stage the
package's source under its import root, including the implementation of any
explicit re-export. Export a manifest to that package, include it and the
marker as package data, and build the wheel with the package's build backend.
Verify the built archive before installing it.

```sh
codepact contracts export --root src --distribution acme-storage \
  --version 1.2.0 --import-scope acme_storage --python-version 3.13 \
  > src/acme_storage/codepact.contracts.json
python -m build --wheel
codepact contracts verify dist/acme_storage-1.2.0-py3-none-any.whl
```

The exporter and verifier read source, manifests and wheel metadata as
data. Verification checks distribution identity, public exports, source
digests and wheel `RECORD`. The selected manifest controls each public
symbol, including aliases that share a definition. A stale manifest leaves
its facts unavailable. A supported manifest can publish a must-use caller
requirement. Cleanup and ownership guarantees require matching checked
source or an applicable reviewed model.
The pure-Python provider profile rejects native extension files that could
change import resolution.

Install the wheel into the analysis environment. Pass the directory containing
the installed package and its `.dist-info` to
`codepact check --root src --dependency-root PATH`, or set ordered dependency
roots in project configuration. Local project modules resolve first, followed
by dependency roots in order. The first matching regular package or module
determines which facts can apply. A manifest listed in the
wheel's `RECORD` stays required after installation. Marked wheels built for
inline publication continue to publish from source.

## Supply a project model

Configure an exact installed distribution version and a synchronous factory
returning a fresh open resource. Include the reviewed origin. Use
`override_publication` to replace an existing factory fact at that exact
version. The ledger records the replaced origin. The first lockfile profile
checks a unique unconditional registry entry in `uv.lock` version 1, revision
3 against the selected installed version. Project models verify installed
source and distribution metadata against the wheel's `RECORD` hashes. The
configured close method must resolve to a synchronous instance method on the
selected resource class.

This example scans application source in `src` and resolves installed
dependencies from the separate `installed` directory.

```toml
[tool.codepact]
config-version = 1
root = "src"
python-version = "3.13"

[tool.codepact.dependencies]
roots = ["installed"]
project-contracts = ["acme-storage.contracts.json"]
uv-lock = "uv.lock"
```

```json
{
  "schema_version": 1,
  "models": [{
    "distribution": "acme-storage",
    "version": "1.2.0",
    "factory": "acme_storage.connect",
    "resource": "acme_storage.Connection",
    "close_method": "close",
    "provenance": "Acme storage review for 1.2.0"
  }]
}
```

A normal close discharges the local cleanup duty. Raised cleanup stays
unknown unless the model declares a reviewed `"exceptional_close": "closed"`
effect. Stale or ambiguous models cause incomplete coverage where callers
need them. Malformed or conflicting project model files exit with status 2.

## Select the runtime profile

Set the source target and analyzed runtime explicitly in `pyproject.toml`.
The runtime profile applies even when the CLI runs on a different host.

```toml
[tool.codepact]
config-version = 1
python-version = "3.13"
experimental-ownership = true
experimental-advanced-contracts = true
assume-default-task-factory = true

[tool.codepact.runtime-profile]
implementation = "cpython"
python-version = "3.13.16"
os = "linux"
architecture = "x86_64"
event-loop = "default"
start-method = "spawn"
assume-standard-process-wait = true
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

## Queues, IPC and guards

The 0.10 runtime models use the CPython 3.13.16 Linux x86_64 profile above.
Enable experimental ownership and the default task factory for asyncio.
Enable experimental advanced contracts for the multiprocessing models.
Declare the exact direct-entry script for new worker effects with
`spawn-main-script = "main.py"` in `[tool.codepact.runtime-profile]`. The
path is relative to the analysis root. Launch that file with Python and use
an explicit `multiprocessing.get_context("spawn")` with a main guard.

Asyncio queue checks distinguish `put` admission, `get` receipt and the
`task_done` duty. Use `finally` after a successful receive when accounting
must also run on a cancelled exit. A join covers the admissions up to the
accounting event that releases its waiter. Immediate shutdown can abandon
buffered entries. Work result observation remains a separate check.

Multiprocessing checks distinguish feeder completion from delivery and
receipt. Queue and JoinableQueue have a feeder after the first accepted put.
SimpleQueue and Pipe serialize in the calling process. Shared Value and Array
compound updates require the exact recursive wrapper lock for the full
read, modify and write.

A project guard identifies two uniquely allocated local bindings in a
top-level coroutine and a resolved method on the resource. For example, if
`main.update` allocates `counter` and `lock`, this configuration requires
the exact `asyncio.Lock` at `counter.increment()` and its resolved aliases.
For a coroutine method, the guard check runs on a supported await.

```toml
[[tool.codepact.async-guards]]
module = "main"
coroutine = "update"
resource = "counter"
lock = "lock"
method = "increment"
```

Reports retain incomplete coverage for unresolved identities, escaped
handles, unsupported launch modes, and effects outside the selected profile.
The checker can establish protocol effects at a resolved worker invocation.
Independent analysis of a worker declaration can still report unbound
parameters as a coverage boundary.
[Protocol diagnostics](rules.md#queue-ipc-and-synchronization-rules)

## Reviewed HTTP and SQL profiles

The profiles target CPython 3.13.16 on Linux x86-64 with Python 3.13 source.
Declare the runtime as shown above. For async profiles, enable experimental
ownership and set the default task-factory assumption for an event loop you
control.

To reproduce the reviewed SQL stack on Linux x86-64 with glibc 2.34 or later,
download {download}`phase09-provider-requirements.txt <../tests/conformance/phase09-provider-requirements.txt>`
into your project directory and install the pinned wheels in a fresh environment.
The requirements include the exact pure-Python SQLAlchemy wheel and its SHA-256
hash. A version-only SQLAlchemy pin can select a native wheel outside this profile.

```sh
uv venv --python 3.13.16 .codepact-providers
uv pip install --python .codepact-providers/bin/python --only-binary :all: \
  -r phase09-provider-requirements.txt
```

Add the environment's package directory to the project configuration alongside
the runtime profile above. Run `codepact check --root src --json` against your source.

```toml
[tool.codepact.dependencies]
roots = [".codepact-providers/lib/python3.13/site-packages"]
```

| Distribution | Reviewed operations |
| --- | --- |
| HTTPX 0.28.1 and HTTPX2 2.16.2 | Separate sync client and direct-await async client profiles. Use `trust_env=False` and literal `http://` requests. The sync profile also covers lexical streaming. |
| SQLAlchemy 2.0.58 | The reviewed pure Python wheel, with local scoped SQLite engines, connections and sessions. The async profile uses aiosqlite 0.22.1. |
| SQLModel 0.0.44 | Separate sync and direct-await `exec` behavior on the selected SQLAlchemy stack. |
| ETLantic SQLModel 0.59.0 | The installed `session_scope` contract for normal commit, modeled exception rollback, and close. |

Inspect `foundations.providers` and `foundations.model_uses` in the JSON report
for selected artifacts, applied effects and coverage boundaries. An async HTTP
client can serve two known requests while both tasks hold it open. SQL uses one
session per concurrent task. Awaiting cleanup and observing a task's result are
distinct obligations, and a database result has its own consumption behavior.
The SQLModel profile distinguishes scalar and row results for direct calls to
`sqlmodel.select`. Operations that require a row result, such as `mappings()`,
produce incomplete coverage when the result family is unresolved.

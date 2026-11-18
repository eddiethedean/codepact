# Check queues, IPC and guards

**Requires CodePact 0.10.** In a 0.10 candidate checkout, build and identify the
binary before using the examples. The [published install guide](install.md)
uses 0.9.0, which does not accept `spawn-main-script` or `async-guards`.

```sh
cargo build --locked -p codepact
./target/debug/codepact --version
```

Expect `codepact 0.10.0` for this candidate. In PowerShell, use
`target/debug/codepact.exe`. Set `CODEPACT` to the absolute path of that
executable or use the full path in each command.

```{raw} html
<p><a href="_static/codepact-concurrency.zip" download>Download the queue, guard, and spawn cases</a></p>
```

Extract the files and work from the directory containing `queue.toml`,
`guard.toml`, and `spawn.toml`. The examples declare CPython 3.13.16 on
Linux x86-64 as the **analyzed runtime**. The checker can run on another
host. Analyze the boundary and finding files as text. The empty-queue
worker in the spawn boundary has no producer and can block when run.

For each topic, copy just one variant to `src/main.py`. The file name
matters for the guard selector and direct-script entry. Run
the checker from the extracted root with the corresponding config.
In PowerShell use `New-Item -ItemType Directory -Force src`,
`Copy-Item queue/valid.py src/main.py -Force`, and
`& $env:CODEPACT check --config queue.toml --mode strict --json > report.json`.

```sh
mkdir -p src
cp queue/valid.py src/main.py
"$CODEPACT" check --config queue.toml --mode strict --json > report.json
```

## Account for a queue receipt

```{literalinclude} _examples/concurrency/queue/valid.py
:language: python
```

Admission from `put`, receipt from `get`, and acknowledgment from
`task_done` are separate effects. The valid example accounts for its receipt
before awaiting `join`. In your own consumer, place `task_done` in a
`finally` after successful receipt when cancellation must also account for
the item. Immediate shutdown can abandon buffered entries. Task result
observation is [a separate obligation](runtime-models.md#work-result-observation).

| Variant | Strict exit | Evidence in the report |
| --- | --- | --- |
| `queue/valid.py` | `0` | `asyncio.Queue.join` has applicable `counter-zero` evidence. |
| `queue/finding.py` | `1` | `TP7001` identifies the unacknowledged receipt from an applicable `get`. |
| `queue/boundary.py` | `1` | The queue model recognizes `get`. Coverage is incomplete because no peer admission is established. |

## Guard a resource method

Copy a `guard/` variant and check with `--config guard.toml`. This config
selects `main.update` and two uniquely allocated local bindings. The second
binding must be the exact `asyncio.Lock` held when the call executes. For
coroutine methods the guard check runs on a supported await.

```{literalinclude} _examples/concurrency/guard.toml
:language: toml
```

| Variant | Strict exit | Evidence in the report |
| --- | --- | --- |
| `guard/valid.py` | `0` | Applicable `guarded-access` evidence, with the lock held. |
| `guard/finding.py` | `1` | Applicable `guarded-access` evidence and `TP7002` for the missing acquisition. |
| `guard/boundary.py` | `1` | `guarded-access` is inapplicable and coverage is incomplete. The receiver can refer to another allocation. |

## Qualify a direct-script spawn

Copy a `spawn/` variant and check with `--config spawn.toml`. The config
enables experimental advanced contracts and selects an exact script path.
With `root = "src"` and source file `src/main.py`, set
`spawn-main-script = "main.py"`. This profiles a direct invocation such as
`python src/main.py`. Use an explicit spawn context and a main guard.

```{literalinclude} _examples/concurrency/spawn/valid.py
:language: python
```

| Variant | Strict exit | Evidence in the report |
| --- | --- | --- |
| `spawn/valid.py` | `0` | `qualified-worker-entry` is applicable, and the subsequent `join` is established. |
| `spawn/finding.py` | `1` | `TP7005` records rejection before launch when the target is a lambda. |
| `spawn/boundary.py` | `1` | Worker entry qualifies, but the IPC `receive` and untimed `join` are inapplicable. Completion and cleanup remain unproved. |

Feeder completion, message delivery, receipt, and process completion are
independent effects. `Queue` and `JoinableQueue` can use a feeder.
`SimpleQueue` and `Pipe` serialize in the calling process. Shared
`Value` and `Array` compound updates require the exact recursive wrapper
lock for the full read–modify–write.

Read `coverage.reasons` alongside each `foundations.model_uses` entry.
An applicable model identifies the semantics the checker used. A scope
can still be incomplete, for example if independently checking the
worker declaration leaves a parameter unresolved. [Protocol diagnostics](rules.md#queue-ipc-and-synchronization-rules)

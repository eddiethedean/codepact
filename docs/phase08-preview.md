# Reviewed contracts and concurrency preview

Phase 0.8 adds an experimental path for publishing and checking installed
contracts and for modeling selected standard-library operations. The
qualification target for the new runtime models is CPython 3.13.16 on
Linux x86_64. Reports identify the analyzed runtime and applicable effects.

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

Use `codepact check --dependency-root PATH` or set ordered dependency roots
in project configuration. The first regular package or module that wins
import resolution determines which facts can apply. A manifest listed in the
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

```toml
[tool.codepact]
config-version = 1
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

The file pack checks local text files opened with supported literal arguments.
Both `with open(...)` and `with handle` close the tracked file for all aliases
on successful context exit. Explicit close has the same alias effect.
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

Set `experimental-foundations = true` to request the 0.8 analysis envelope
without supplying a runtime. Selecting dependency roots or a runtime enables
the envelope automatically. Those checks emit CLI JSON report schema 4 with
a model-use ledger, selected artifact digests, named source scope and exhausted
limits. Rust hosts can
provide the equivalent inputs through `check_with_foundations` or an
`AnalysisSession`. [Output schemas](schemas.md)

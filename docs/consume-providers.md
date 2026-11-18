<!-- release-recipes -->

# Check with installed providers

CodePact reads exact files and metadata from explicitly selected dependency
roots. A root is the directory that contains the installed package and its
`.dist-info`, such as the environment's `purelib` path. Local modules resolve
first, followed by dependency roots in order. Include the root containing
your **application** separately from the dependency root.

## Try an installed provider

Follow [Publish a provider wheel](publish-provider.md) to build and verify the
sample. From the extracted provider project's root, install the verified
wheel into a separate directory. This creates a source tree for static
analysis without installing application dependencies.

<!-- smoke: install-provider -->
```sh
python -m pip install --no-deps --target installed dist/acme_storage-1.2.0-py3-none-any.whl
```

From `caller/`, run these commands. Use the published 0.9.0 CLI installed
on your PATH or prefix it with `uv tool run --from codepact-cli==0.9.0`.

```sh
codepact check --dependency-root ../installed --json > report.json
```

Expect `TP3001` at the discarded call in `src/main.py` and exit `1`. The
caller sets `fail-on = "warning"`. The report lists `acme-storage` version
`1.2.0` with `origin: "manifest"` in `foundations.providers`. The manifest
establishes a caller result-use requirement. The installed function's body
effects can remain unknown.

Replace `src/main.py` with `variants/repaired.py` and rerun the same command.
The repaired advisory report exits `0` with the must-use finding cleared.
Keep reading coverage before switching to strict mode.

To see the dependency boundary, omit `--dependency-root` and use
`--mode strict`. The caller's `required-providers` entry leaves incomplete
coverage and exit `1`. Check `coverage.reasons`, even if there is no
`foundations` object.

For an existing environment, discover the package directory with
`python -c "import sysconfig; print(sysconfig.get_path('purelib'))"` using
that environment's interpreter. Add it to `[tool.codepact.dependencies].roots`
or pass it to `--dependency-root`. Use
`codepact check --show-settings --json` to verify scope and precedence.
[Troubleshoot unresolved imports](troubleshooting.md#an-imported-class-cannot-be-resolved)

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

Add `uv-lock = "uv.lock"` under `[tool.codepact.dependencies]` if you
have a compatible lockfile and want CodePact to verify the selected
registry version.

A normal close discharges the local cleanup duty. Raised cleanup stays
unknown unless the model declares a reviewed `"exceptional_close": "closed"`
effect. Stale or ambiguous models cause incomplete coverage where callers
need them. Malformed or conflicting project model files exit with status 2.

## Reviewed HTTP and SQL profiles

The profiles target CPython 3.13.16 on Linux x86-64 with Python 3.13 source.
Use the [reviewed runtime profile](runtime-models.md#select-the-runtime-profile). For async profiles, enable experimental
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
the runtime profile. Run `codepact check --root src --json` against your source.

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

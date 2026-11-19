# 0.11 implementation evidence

Use a clean checkout of the exact candidate revision. The [validation
matrix](validation.md) defines the accepted observations. The comparison
baseline is [published CodePact 0.10.0](../../../releases/0.10.0-evidence.md).

| Validation | Reproduction |
| --- | --- |
| V11-01–05 | Run `cargo test --locked -p codepact --test phase11_async_resources`. The syntax corpus checks acquisition before and after first suspension, early break, close and retry, lexical async cleanup, stream closure, handler lifetime and child exit. |
| V11-06–08 | Run `tests/conformance/phase11_provider_probe.py` in the pinned provider environment. Set `CODEPACT_PHASE11_CHECKER` to the candidate CLI to replay the static cases beside the runtime probes. The Quality workflow retains the installed-provider logs. |
| V11-09–12 | Run `cargo test --locked -p codepact --test phase11_process` and the Phase 0.11 process probes. The process corpus checks partial startup, forced stop and reap, retained IPC, shared view and mapping closure, and name removal after attachment failure. |
| V11-13–16 | Run `cargo test --locked -p codepact --test phase11_bridges` and the Phase 0.11 bridge probes. The bridge corpus exercises dispatch, linked future identity, completion after cancellation, repeated pool shutdown and cross-thread submission. |
| V11-17–18 | Run `cargo test --locked -p codepact --test phase11_review`. The review corpus covers combined obligations, stable work correspondence, and runtime-profile invalidation and reuse. |
| V11-19 | Run Quality and the build-only Release workflow for the candidate revision. Verify the helper, both source archives, all eight native CLI wheel routes, and the Cargo archive. |
| V11-20 | Replay the paired performance capture against `v0.10.0`. Record raw samples and executable hashes with the result. |

[Quality workflow](../../../../.github/workflows/quality.yml) ·
[Release workflow](../../../../.github/workflows/release.yml) ·
[Runtime registry](../../../../tests/conformance/runtime-models.json)

## Runtime qualification

Run the registered stdlib probes with CPython 3.13.16 on Linux x86-64. The
suite fingerprints the loaded runtime sources and the interpreter artifact
for built-in modules. Use machine-readable receipts and retain the provider
logs for the same candidate revision.

```sh
uv run --no-project --python 3.13.16 python scripts/check_model_conformance.py \
  --python-version 3.13.16 --receipt target/phase11-runtime-linux.json
uv run --no-project --python 3.13.16 --no-build \
  --with-requirements tests/conformance/phase09-provider-requirements.txt \
  python tests/conformance/phase11_provider_probe.py
```

For the installed-provider static replay, build the candidate with
`cargo build --locked -p codepact`, then set
`CODEPACT_PHASE11_CHECKER="$PWD/target/debug/codepact"` for the provider command.
The runtime probes execute controlled fixtures. The CLI reads caller and
provider sources as input.

### Local candidate checks

Commit `6fa8f94` passed the 86 registered probes with zero skips on
CPython 3.13.16 in a Linux x86-64 container. The runtime receipt SHA-256
was `574723dedef21ee8ae924e598fa8338909da94d1573b3644db731f771fff25ad`.
Four installed-provider runtime probes and the candidate CLI's static replay
passed against the pinned provider artifacts. The macOS arm64 CLI wheel,
pure Python helper wheel and both source archives passed Twine metadata
checks. The extracted native wheel reported `codepact 0.11.0`.

## Performance replay

Build the same optimized binaries in both checkouts using
`cargo build --release --locked --example benchmark_phase09 --example benchmark_phase10`.
Capture the pair on the same host.

```sh
python scripts/capture_phase11_performance.py \
  --baseline /path/to/clean-v0.10.0 --output target/phase11-performance.json
```

The capture records inherited review and work costs, queue, IPC and guard
costs, and the new stream, subprocess, bridge and shared-memory workloads.
Record cold, unchanged and edited snapshots, profile invalidation,
comparison and peak RSS.

The local capture for `6fa8f94` used Rust 1.96 on macOS arm64. It alternated
the published and candidate binaries across ten process pairs at 64 callers.
The largest inherited median slowdown was 2.12% for an affected source
review edit (0.253 ms). The largest inherited peak RSS increase was 0.47 MiB
for the queue workload.

| New workload | Cold median | Unchanged median | Peak RSS |
| --- | ---: | ---: | ---: |
| Async stream | 16.33 ms | 9.02 ms | 16.84 MiB |
| Async subprocess | 22.73 ms | 13.23 ms | 18.44 MiB |
| Executor bridge | 11.62 ms | 6.92 ms | 15.86 MiB |
| Shared memory | 13.37 ms | 6.81 ms | 15.17 MiB |

The raw local receipt at `target/phase11-performance.json` has SHA-256
`91856191a4c0a6b25d1af222327b7107b1aa5f90aab8f05d989b069b0f946506`.

## Release decision

Use the [common release gates](../../release-gates.md) and
[release evidence template](../../release-evidence-template.md) to record
approval and publication. Candidate source and local receipts qualify only
the profiles and artifacts named in those receipts.

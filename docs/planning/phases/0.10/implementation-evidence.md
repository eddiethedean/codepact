# 0.10 implementation evidence

The [validation matrix](validation.md) defines the required cases. The
comparison baseline is [published 0.9.0](../../../releases/0.9.0-evidence.md).

| Evidence | Reproduction |
| --- | --- |
| V10-01–07 | Run `cargo test --locked -p codepact --test phase10_async` for the asyncio source-analysis corpus. The exact runtime suite exercises accounting, getters and putters, shutdown, semaphore reservations, condition reacquisition and event signaling. |
| V10-08–15 | Run `cargo test --locked -p codepact --test phase10_process` for the multiprocessing corpus. The runtime suite exercises direct-script and package startup, feeder serialization, reconstructed values, receiver copies and EOF, partial acquisition and compound shared updates. |
| V10-16 | Run `cargo test --locked -p codepact --test phase10_review` for guarded accesses, source review and CLI/Rust evidence. |
| V10-17 | Run the Quality workflow and the build-only Release workflow for the candidate. The latter builds and smokes the published distribution targets as branch artifacts. [Release workflow](../../../../.github/workflows/release.yml) |
| V10-18 | Build the optimized `benchmark_phase06`, `benchmark_phase08`, and `benchmark_phase09` examples in the candidate and exact `v0.9.0` baseline. Build `benchmark_phase10` in the candidate and run the capture script below. |

## Runtime conformance

The pinned CPython 3.13.16 Linux x86_64 run passed 66 registered probes with
zero skips, including all 26 Phase 0.10 probes. The local run used the Linux
amd64 `python:3.13.16-slim-bookworm` image via emulation, with manifest digest
`sha256:73aa5faee130c60da030e105ec971ccf57473fbd22ce73bd6fc9ba99a40b838d`.
The receipt at `target/phase10-runtime-linux.json` has SHA-256
`6762949cd84c6b8d9d0ee2f1c5a67f7743d2a7ad3b16110e20025f365e660758`.
The Quality workflow runs the same registered probes and uploads their receipt.
When a native module is built into CPython, the receipt identifies the module
and fingerprints the interpreter and its available shared runtime library.
The uv-managed CPython 3.13.16 Linux x86_64 replay also passed all 66 probes
with zero skips. Its receipt at `target/phase10-runtime-uv-linux.json`
identifies built-in `_multiprocessing` and has SHA-256
`0942903b07257822dc27be3bde74279b6ed09cc4185e368d0f4aa0a141b11e91`.
[Runtime registry](../../../../tests/conformance/runtime-models.json)

```sh
uv run --no-project --python 3.13.16 python scripts/check_model_conformance.py \
  --python-version 3.13.16 --receipt target/phase10-runtime-linux.json
```

## Performance replay

Build each named example with `cargo build --release --locked --example NAME`.
Run the capture from the candidate checkout.

```sh
python scripts/capture_phase10_performance.py \
  --baseline /path/to/codepact-v0.9.0 --output target/phase10-performance.json
```

The capture records absolute costs for queue, IPC and guard workloads. It also
records paired differences for the inherited workloads, executable digests
and raw samples. [Capture script](../../../../scripts/capture_phase10_performance.py)

The working-tree capture used Rust 1.96.0 on macOS 26.5.2 arm64. It ran ten
alternating process pairs with seven trials per process for inherited
workloads. The largest inherited median slowdown was 7.29% on the unchanged
executor check, a difference of 0.2 µs. The largest peak RSS increase was
1.77 MiB.

| Workload, 64 callers | Cold | Unchanged | Affected edit | Unrelated edit | Profile edit | Comparison | Peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Queue | 25.624 ms | 18.184 ms | 12.592 ms | 7.428 ms | 58.755 ms | 0.154 ms | 23.70 MiB |
| IPC | 14.384 ms | 7.416 ms | 7.976 ms | 1.138 ms | 32.350 ms | 0.032 ms | 18.11 MiB |
| Guard | 16.975 ms | 8.235 ms | 12.494 ms | 2.853 ms | 25.210 ms | 0.054 ms | 16.00 MiB |

The local receipt at `target/phase10-performance.json` has SHA-256
`963c9fce09a9a84403e9e4a6a52f17176039ded74fe2ba9a3f3763e9a0dfa44c`.

The [0.10.0 release record](../../../releases/0.10.0-evidence.md) contains the
signed gates, tagged revision, and registry verification.

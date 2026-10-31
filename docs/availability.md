# Release and capability availability

This guide records what users can install today and what is still available only
from source. It is maintained with the release documents; the exact 0.1.0
qualification and artifact evidence is in [the release record](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.1.0-evidence.md).

## Current release status

| Version | Status | Available behavior |
| --- | --- | --- |
| 0.1.0 | Published on PyPI and crates.io | Contract declaration inspection with `inspect-contracts` |
| 0.2.0 | Upcoming; qualification pending | Lifecycle checking is available in the source preview through `typepact check` |

This source snapshot still identifies itself as version `0.1.0`; that may change
when a release candidate is prepared. A version string alone does not tell
whether the executable came from the published package or a source build
containing the preview. Record the source commit when sharing preview results.
The [0.2.0 phase plan](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.2-lifecycle.md)
lists the gates that remain before release acceptance.

## Capability by installation

| Install | `inspect-contracts` | `check` | Version identifier |
| --- | --- | --- | --- |
| PyPI `typepact==0.1.0` and matching `typepact-cli==0.1.0` | Yes | No | `0.1.0` |
| crates.io `typepact==0.1.0` | Rust inspection API and CLI | No | `0.1.0` |
| Source checkout at the lifecycle preview | Yes | Yes, for the documented subset | Read the manifest version, and use the commit SHA to identify preview behavior |

Install the current published Python packages with:

```sh
uv add typepact==0.1.0
```

After 0.2.0 is published, install that release with `uv add typepact==0.2.0`.
Until then, that version is not available from the registries.

## Platform and runtime evidence

The 0.1.0 helper wheel is pure Python and requires Python 3.11 or newer. The CLI
release built eight native wheels:

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Wheel built for Windows 2022 | Wheel built for Windows 11 ARM |

The release CI built and inventoried all eight wheels. Local runtime checks
covered macOS ARM64, the helper/CLI source builds, and extracted Cargo archives;
runtime execution on the other wheel platforms was not part of the 0.1.0
qualification scope. See the [release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.1.0-evidence.md) for
exact results.

The Python helpers support Python 3.11 and later; CI checked CPython 3.11–3.14.
The analyzer grammar supports source targets Python 3.11, 3.12, and 3.13. The
selected source target is independent of the interpreter running the command.
Building the CLI from source requires Rust 1.96 or newer and a linker.

## Try the 0.2.0 source preview

Use the pinned source commit below for a reproducible preview run:

```sh
git clone https://github.com/eddiethedean/typepact.git
cd typepact
git checkout cf2fb90989d3ed01b7ca9edb56cc5ff8bd7122c4
cargo run --locked -- check --root examples
```

The example intentionally reports a definite use after close and exits `1`.
Follow the [lifecycle tutorial](lifecycle.md) for the expected result and a valid
counterpart. The source build needs Rust 1.96+ and the locked dependencies. For a
newer checkout, record `git rev-parse HEAD` with any report.

The preview does not establish cleanup on every exit, general ownership safety,
or runtime behavior of declared close methods. Its limits are described in
[supported syntax](supported-syntax.md) and [limitations](limitations.md).

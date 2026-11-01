# Release and capability availability

TypePact 0.2.0 is published on PyPI and crates.io. Its release and artifact
evidence is recorded in the [0.2.0 release record](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md).

## Current releases

| Version | Status | Available behavior |
| --- | --- | --- |
| 0.2.0 | Published on PyPI and crates.io | Declaration inspection and bounded lifecycle checking for the documented synchronous subset |
| 0.1.0 | Published on PyPI and crates.io | Contract declaration inspection with `inspect-contracts` |

The published Python packages and Rust crate report version `0.2.0`. Use a
source commit when you need to identify unreleased development changes.

## Capability by installation

| Install | `inspect-contracts` | `check` | Version identifier |
| --- | --- | --- | --- |
| PyPI `typepact==0.2.0` and matching `typepact-cli==0.2.0` | Yes | Yes, for the documented subset | `0.2.0` |
| crates.io `typepact==0.2.0` | Rust inspection API and CLI | Rust API and CLI | `0.2.0` |
| PyPI `typepact==0.1.0` and matching `typepact-cli==0.1.0` | Yes | No | `0.1.0` |
| crates.io `typepact==0.1.0` | Rust inspection API and CLI | No | `0.1.0` |
| Source checkout | Yes | Yes, for the implemented subset | Record the Git commit |

Install the current Python packages with:

```sh
uv add typepact==0.2.0
```

The helper pins the matching CLI package. To install only the command, use
`uv tool install typepact-cli==0.2.0`. For Rust applications, add
`typepact = "0.2.0"` or run `cargo install typepact --version 0.2.0 --locked`.

## Platform and runtime evidence

The 0.2.0 helper wheel is pure Python and requires Python 3.11 or newer. The CLI
release built and smoke-tested eight native wheels:

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Native x86-64 wheel | Native ARM64 wheel |

The exact release workflow built and ran platform install/smokes for every CLI
wheel. See the [0.2.0 release evidence](https://github.com/eddiethedean/typepact/blob/main/docs/releases/0.2.0-evidence.md) for the
artifact hashes and qualification results.

The Python helpers support Python 3.11 and later; CI checked CPython 3.11–3.14.
The analyzer grammar supports source targets Python 3.11, 3.12, and 3.13. The
selected source target is independent of the interpreter running the command.
Building the CLI from source requires Rust 1.96 or newer and a linker.

## Build from the release source

To reproduce the published source build:

```sh
git clone https://github.com/eddiethedean/typepact.git
cd typepact
git checkout v0.2.0
cargo run --locked -- check --root examples
```

The example intentionally reports a definite use after close and exits `1`.
Follow the [lifecycle tutorial](lifecycle.md) for the expected result and a valid
counterpart. The checker does not establish cleanup on every exit, general
ownership safety, or runtime behavior of declared close methods. See
[supported syntax](supported-syntax.md) and [limitations](limitations.md).

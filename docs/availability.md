# Release and capability availability

CodePact 0.4.0 was published on November 10, 2026 as a synthetic preview.
Tag `v0.4.0` points to `817dc2c0a85857ad7af0745e3933f875c403ab67`.
All 25 release jobs passed. The 11 PyPI files and the crates.io archive were
downloaded and verified, and clean installs passed smoke checks.
[Tag](https://github.com/eddiethedean/codepact/tree/v0.4.0) · [Release workflow](https://github.com/eddiethedean/codepact/actions/runs/42916282756) · [Release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.4.0-evidence.md)

CodePact 0.3.0 was the first release under the CodePact name. Versions 0.1.0
and 0.2.0 used the former TypePact name.
[0.3.0 evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md) ·
[0.2.0 evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.2.0-evidence.md) ·
[0.1.0 evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.1.0-evidence.md)

Read the guides in the [repository documentation index](https://github.com/eddiethedean/codepact/blob/main/docs/README.md)
or [build a local HTML preview](documentation.md).

## Current releases

| Version | Status | Package and crate names | Available behavior |
| --- | --- | --- | --- |
| CodePact 0.5.0 | Development candidate | [Matching helper and CLI from source](install.md#try-the-05-development-candidate) | Opt-in moves, lexical borrows, and a bounded TaskGroup model |
| CodePact 0.4.0 | Published synthetic preview | `codepact`, `codepact-cli`, Rust crate `codepact` | Adoption configuration/policy, JSON report schema 2, baselines, suppressions, and synchronous must-use preview; see the 0.4 phase plan |
| CodePact 0.3.0 | Published | `codepact`, `codepact-cli`, Rust crate `codepact` | Declaration inspection, lifecycle checking, cleanup obligations, and bounded intratask cancellation/cleanup; see the specification |
| TypePact 0.2.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Declaration inspection and bounded synchronous lifecycle checking |
| TypePact 0.1.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Contract declaration inspection |

Real-world usefulness remains unqualified for 0.4.0. The release record
documents the synthetic corpus, supported boundaries, and measured performance.
[Scope and qualification](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.4.0-evidence.md)

## Install CodePact 0.4.0

Install the helper and matching CLI with `uv add codepact==0.4.0`, or install
the command alone with `uv tool install codepact-cli==0.4.0`. Rust applications
can use `cargo add codepact@0.4.0`.
[PyPI helper](https://pypi.org/project/codepact/0.4.0/) · [PyPI CLI](https://pypi.org/project/codepact-cli/0.4.0/) · [Crates.io](https://crates.io/crates/codepact/0.4.0)

## Build from source

Build the selected repository checkout with Python 3.11+ and Rust 1.96+:

```sh
git clone --branch v0.4.0 https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
uv run --locked codepact check --root examples
```

Record the full Git revision when sharing reports from a source build.

## Platform and runtime evidence

The 0.4.0 release includes a universal pure Python helper wheel and eight
native CLI wheel targets:

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Supported | Supported |

The tag-triggered artifact build, registry digest checks, and clean-install
results are recorded in the [0.4.0 evidence record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.4.0-evidence.md).
The Python helpers target Python 3.11 and later; the analyzer source grammar
targets Python 3.11–3.13. Building the CLI from source requires Rust 1.96+ and a
linker.

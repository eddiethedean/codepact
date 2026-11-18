# Release and capability availability

CodePact 0.9.0 is available as the Python helper, a matching native CLI,
and a Rust crate. [PyPI helper](https://pypi.org/project/codepact/0.9.0/) ·
[PyPI CLI](https://pypi.org/project/codepact-cli/0.9.0/) ·
[crates.io](https://crates.io/crates/codepact/0.9.0) ·
[Source tag](https://github.com/eddiethedean/codepact/tree/v0.9.0)

Read the guides in the [hosted documentation](https://codepact.readthedocs.io/en/latest/index.html)
or open the [stable documentation](https://codepact.readthedocs.io/en/stable/index.html) for the current release tag.
For a permanent link to 0.9.0 instructions, use the
[release's immutable documentation index](https://github.com/eddiethedean/codepact/blob/7b17a494f7d98cfa7f9dc27db5dc7d88b1214e78/docs/README.md).
[Verify release artifacts and hosted docs](release-verification.md)

## Releases

| Version | Status | Package and crate names | Available behavior |
| --- | --- | --- | --- |
| CodePact 0.10.0 | Candidate checkout | Source build | [Queues, IPC, guards, and direct-script spawn](concurrency.md) |
| CodePact 0.9.0 | Current release | `codepact`, `codepact-cli`, Rust crate `codepact` | Persistent outcome and work duties, declaration policy, source and wheel comparison, and reviewed HTTP and SQL profiles |
| CodePact 0.8.0 | [Release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.8.0-evidence.md) | `codepact`, `codepact-cli`, Rust crate `codepact` | Reviewed installed contracts, wheel manifests, and opt-in file, asyncio, and multiprocessing models |
| CodePact 0.7.0 | [Release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.7.0-evidence.md) | `codepact`, `codepact-cli`, Rust crate `codepact` | Typed outcomes, opt-in finite matches, and marked installed-provider checking |
| CodePact 0.6.0 | [Published advanced-contracts preview](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.6.0-evidence.md) | `codepact`, `codepact-cli`, Rust crate `codepact` | Opt-in exclusive access, state protocols, and a bounded ThreadPoolExecutor model |
| CodePact 0.5.1 | Published corrective release | `codepact`, `codepact-cli`, Rust crate `codepact` | CLI, source-location, adoption-identity, and ownership-boundary fixes; 0.5.0 behavior retained |
| CodePact 0.5.0 | Published ownership preview | `codepact`, `codepact-cli`, Rust crate `codepact` | Opt-in moves, lexical borrows, and a bounded TaskGroup model |
| CodePact 0.4.0 | Published synthetic preview | `codepact`, `codepact-cli`, Rust crate `codepact` | Adoption configuration/policy, JSON report schema 2, baselines, suppressions, and synchronous must-use preview; see the 0.4 phase plan |
| CodePact 0.3.0 | Published | `codepact`, `codepact-cli`, Rust crate `codepact` | Declaration inspection, lifecycle checking, cleanup obligations, and bounded intratask cancellation/cleanup; see the specification |
| TypePact 0.2.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Declaration inspection and bounded synchronous lifecycle checking |
| TypePact 0.1.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Contract declaration inspection |

Real-world usefulness remains unqualified for the synthetic previews.

## Install CodePact 0.9.0

Install the helper and matching CLI with `uv add 'codepact[cli]==0.9.0'`. For
the helper alone, use `uv add codepact==0.9.0`. For the command alone, use
`uv tool install codepact-cli==0.9.0`. Rust applications can use
`cargo add codepact@=0.9.0`. [Install options](install.md) · [Migration guide](upgrading.md)

## Build from source

Build the selected repository checkout with Python 3.11+ and Rust 1.96+:

```sh
git clone --branch v0.9.0 https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked --extra cli
uv run --locked --extra cli codepact --version
uv run --locked --extra cli codepact check --root examples
```

The example check intentionally exits `1` with TP1001. Record the full Git
revision when sharing reports from a source build.

## Platform and runtime evidence

The 0.9.0 distribution matrix includes a pure Python helper wheel, both
source archives, and eight native CLI wheel targets.

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Supported | Supported |

The helpers run on Python 3.11 and later. The analyzer accepts source targets
3.11–3.13. Building the CLI from source requires Rust 1.96+ and a linker.

The reviewed runtime models target CPython 3.13.16 on Linux x86-64. This profile
describes the analyzed program and can be selected from any supported CLI
host. [Model profiles and assumptions](reviewed-contracts.md) ·
[Implementation evidence](https://github.com/eddiethedean/codepact/blob/v0.9.0/docs/planning/phases/0.9/implementation-evidence.md)

## Earlier release evidence

CodePact 0.5.1 was published on November 13, 2026. The corrective release
passed all 18 tag-triggered jobs. All 11 PyPI files and the crates.io archive
were downloaded and verified, and clean install smoke checks passed.
[Tag](https://github.com/eddiethedean/codepact/tree/v0.5.1) ·
[Release workflow](https://github.com/eddiethedean/codepact/actions/runs/43529732348) ·
[Release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.5.1-evidence.md)

CodePact 0.5.0 was published on November 10, 2026 and introduced experimental
ownership. Its [release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.5.0-evidence.md)
records the original qualification.

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

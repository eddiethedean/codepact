# Release and capability availability

CodePact 0.3.0 is the first release under the CodePact name and the new
`codepact` Python packages, CLI, and Rust crate. Its publication and verification
are recorded in the [release evidence](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md).

The published 0.1.0 and 0.2.0 releases used the former TypePact name. Their
historical records remain available in the [0.1.0](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.1.0-evidence.md)
and [0.2.0](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.2.0-evidence.md) release evidence.

The documentation site still uses the existing `typepact.readthedocs.io`
address. Its visible title and repository content use CodePact; the hosting
project slug has not been renamed.

## Current releases

| Version | Status | Package and crate names | Available behavior |
| --- | --- | --- | --- |
| CodePact 0.4.0 | Source implementation candidate; not published or pilot-qualified | `codepact`, `codepact-cli`, Rust crate `codepact` | Adoption configuration/policy, JSON report schema 2, baselines, suppressions, and synchronous must-use preview; see the 0.4 phase plan |
| CodePact 0.3.0 | Published | `codepact`, `codepact-cli`, Rust crate `codepact` | Declaration inspection, lifecycle checking, cleanup obligations, and bounded intratask cancellation/cleanup; see the specification |
| TypePact 0.2.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Declaration inspection and bounded synchronous lifecycle checking |
| TypePact 0.1.0 | Published | `typepact`, `typepact-cli`, Rust crate `typepact` | Contract declaration inspection |

The latest published packages and crate are 0.3.0. To reproduce that release,
check out tag `v0.3.0` and record the full Git commit with results. The current
repository metadata describes the unreleased 0.4.0 candidate; those packages
are available only by building this checkout and are not a distribution or
platform-support claim.

## Install CodePact 0.3.0

Install the Python helper and matching CLI with `uv add codepact==0.3.0`, or
install the command alone with `uv tool install codepact-cli==0.3.0`. Rust
applications can depend on `codepact` 0.3.0 from crates.io.

## Build from source

Build the selected repository checkout with Python 3.11+ and Rust 1.96+:

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
uv run --locked codepact check --root examples
```

For the 0.4 candidate, record the checkout's full Git revision with every
report. Do not describe it as the published 0.3 package. The 0.4 phase plan
lists the implementation evidence and the still-pending pilots, performance
review, maintainer acceptance and release gates.

## Platform and runtime evidence

The published release includes a universal pure Python helper wheel and eight
native CLI wheel targets:

| Platform | x86-64 | ARM64 |
| --- | --- | --- |
| Linux glibc | glibc 2.17+ | glibc 2.28+ |
| Linux musl | musl 1.2+ | musl 1.2+ |
| macOS | 10.13+ | 11.0+ |
| Windows | Supported | Supported |

The tag-triggered artifact build, registry digest checks, and clean-install
results are recorded in the [0.3.0 evidence record](https://github.com/eddiethedean/codepact/blob/main/docs/releases/0.3.0-evidence.md).
The Python helpers target Python 3.11 and later; the analyzer source grammar
targets Python 3.11–3.13. Building the CLI from source requires Rust 1.96+ and a
linker.

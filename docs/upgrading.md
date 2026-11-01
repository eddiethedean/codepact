# Upgrade from 0.1.0 to 0.2.0

Version 0.2.0 is published on PyPI and crates.io. It adds bounded lifecycle
checking while retaining declaration inspection. The Python helper pins the
matching `typepact-cli` version.

## Upgrade the Python packages

Update your project dependency and lockfile:

```sh
uv add typepact==0.2.0
uv sync --locked
uv run typepact --version
uv run typepact inspect-contracts app.py
uv run typepact check --root src
```

The 0.2.0 release supports both `inspect-contracts` and `check`. Checking reads
all supported `.py` files below its root and reports use-after-close and
close-through-`Borrowed` findings. Review the [release notes](release-notes.md),
[availability table](availability.md), and [limitations](limitations.md) before
enabling it in a required build gate.

## Upgrade the Rust dependency

Update the crate and rebuild:

```sh
cargo add typepact@0.2.0
cargo build --locked
```

The `check` and session-checking APIs are available in this published version.
See the [Rust API reference](rust-api.md) for signatures and result handling.

## Review behavior before changing CI

Inspection exits `1` on declaration errors or incomplete coverage. Lifecycle
checking exits `1` for a definite lifecycle violation or declaration error;
possible findings and coverage boundaries alone can still exit `0`. If your CI
policy requires complete coverage, parse the JSON report and enforce that policy
explicitly as shown in [report handling](ci-reporting.md).

The 0.x API, diagnostic set, and schema remain provisional. Check the release
notes and output schema before upgrading any consumer that parses diagnostics.

## Planned change in 0.3.0

Version 0.3.0 is not released. Its candidate adds TP1002 for definite cleanup
obligations left outstanding on modeled exits, so an existing check can begin
returning status `1` when a function acquires a resource and neither closes nor
returns it through a resolved `Owned[T]` boundary. Review the [unreleased
candidate notes](https://github.com/eddiethedean/typepact/blob/main/docs/release-notes.md#030--candidate-not-released) and the [cleanup
limitations](https://github.com/eddiethedean/typepact/blob/main/docs/planning/phases/0.3/specification.md) before trying the candidate in an optional CI job. Keep the check advisory until the maintainer accepts the
phase evidence and publishes a qualified release. Install the matching
`typepact` and `typepact-cli` versions together when 0.3 is released; no 0.3
package version is available yet.

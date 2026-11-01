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

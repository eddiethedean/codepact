# Upgrade from 0.1.0 to 0.2.0

Version 0.1.0 is the published contract-inspection release. Version 0.2.0 is
being prepared and adds lifecycle checking. The `check` command is currently
available only in the source preview; install the 0.2.0 packages only after the
release is published.

## Before 0.2.0 is published

Keep using the released packages for inspection:

```sh
uv add typepact==0.1.0
uv run typepact inspect-contracts app.py
```

To evaluate lifecycle checking, build the [pinned source preview](availability.md#try-the-020-source-preview).
The checkout currently reports version `0.1.0`, so record its Git commit rather
than relying on `typepact --version` to identify preview behavior.

## After 0.2.0 is published

Update the helper and CLI together; `typepact` pins `typepact-cli` to the same
version:

```sh
uv add typepact==0.2.0
uv run typepact --version
uv run typepact inspect-contracts app.py
uv run typepact check --root src
```

Keep `inspect-contracts` for declaration inspection. `check` adds bounded
lifecycle analysis, reads all supported `.py` files under its root, and reports
use after close and close-through-`Borrowed` findings. Review the
[0.2.0 release notes](release-notes.md), [availability table](availability.md),
and [limitations](limitations.md) before enabling it in a required build gate.

## Review behavior before changing CI

Inspection exits `1` on declaration errors or incomplete coverage. Lifecycle
checking exits `1` for a definite lifecycle violation or declaration error;
possible findings and coverage boundaries alone can still exit `0`. If your CI
policy requires complete coverage, parse the JSON report and enforce that policy
explicitly as shown in [report handling](ci-reporting.md).

The 0.x API, diagnostic set, and schema remain provisional. Check the release
notes and output schema before upgrading any consumer that parses diagnostics.

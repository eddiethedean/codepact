# Consume reports in CI

Save the JSON report alongside the original check exit status. Use the
[workflow example](automation.md) for a GitHub Actions job that preserves
both.

## Preserve the selected policy

For a custom POSIX-shell job with Python 3.11 or later, download
{download}`check-report.sh <_examples/check-report.sh>`. Pass the complete
command that runs your check. The script adds `--json`, saves the output,
validates the completed schema and status, and returns the check status.

<!-- smoke: ci-report -->
```sh
sh check-report.sh uv run --locked codepact check --root src --mode strict
```

```{literalinclude} _examples/check-report.sh
:language: sh
```

## Interpret each result

| Case | Check exit | What to inspect |
| --- | --- | --- |
| Passing check | `0` | Verify the selected rules, coverage, and source scope before relying on the result. |
| Active finding at the failure threshold | `1` | Read actionable diagnostics, certainty, and related spans. |
| Baseline or inline suppression | Policy-dependent | The suppressed entry remains in `diagnostics`. Its `disposition` records the exception. |
| Disabled rule | Policy-dependent | The report records disabled rules and retains matching diagnostics as audit entries. |
| Incomplete coverage | `0` or `1` | Read `coverage.reasons`. Strict mode fails on incomplete coverage. Advisory mode may pass. |
| Truncated detail | Policy-dependent | The report sets `details_truncated`. Rerun without a detail cap and check whether per-finding limits still apply before using the omitted detail. |
| Invocation or execution failure | `2` | Check stderr and any failure envelope. A tool runner or shell can also fail with its own status. |

To require full report detail for a downstream consumer, make truncation a
separate validation failure. To gate warnings, configure `fail-on = "warning"`.
Use `actionable` and `disposition` to identify findings under the selected
policy. [Exit-policy matrix](cli.md#policy-and-exits) · [JSON fields](output.md)

## Preserve reports safely

Adoption reports can include absolute checkout and configuration paths
(`project_root` and `config_file`), logical paths, symbol names, source spans,
diagnostic text, and SHA-256 source hashes in `snapshot_manifest`. Set
artifact access and retention for the repository before uploading reports.
Share a minimized reproduction or a reviewed excerpt outside that audience.
Keep the original report private when consumers need its complete identity
and audit metadata. [Security and data handling](security.md)

## Older releases

CodePact 0.3.0 lifecycle checks emit checker schema 1.
`inspect-contracts` emits inspection schema 0. Select a consumer based on
the command and report version. [Schema reference](schemas.md) ·
[Upgrade guide](upgrading.md)

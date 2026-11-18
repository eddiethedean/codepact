<!-- release-recipes -->

# Consume reports in CI

Save the JSON report alongside the original check exit status. Use the
[workflow example](automation.md) for a GitHub Actions job that preserves
both.

## Preserve the selected policy

For a custom POSIX-shell job with Python 3.11 or later, download
{download}`check-report.sh <_examples/check-report.sh>`. Pass the complete
command that runs your check. The script adds `--json`, saves the output,
checks the completed report's schema version and status, and returns the check
status. It accepts schema versions 2, 3, and 4, including foundations reports.

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
Foundations reports also carry dependency identities, artifact digests, and
model provenance.
Share a minimized reproduction or a reviewed excerpt outside that audience.
Keep the original report private when consumers need its complete identity
and audit metadata. [Security and data handling](security.md)

## Review a change

The trusted host workflow selects the base revision, the checker artifact and
the policy. Check out the base and candidate in separate directories. Retain
each revision's dependency artifacts and capture the full selected root in
both checkouts. Use the same pinned binary and analyzed Python target.

Set `CODEPACT` to the absolute path to the verified 0.9.0 CLI. Set
`CODEPACT_BASE_POLICY` and `CODEPACT_CANDIDATE_POLICY` to absolute paths to
host-controlled configurations that apply the same policy. If the configurations
select lockfiles or project model files, set their paths for the corresponding
revision. The checker resolves relative configuration paths from the policy
file's directory.

Set `CODEPACT_BASE_DEPENDENCY_ROOT` and `CODEPACT_CANDIDATE_DEPENDENCY_ROOT`
to the directories holding each revision's installed packages and their
`.dist-info` directories. The recipe passes each root explicitly. A relative
`--dependency-root` path resolves from the selected checkout. Keep the captures
in the host job's working directory alongside the two checkouts.

```sh
capture() {
    checkout=$1
    policy=$2
    dependency_root=$3
    snapshot=$4
    status=0
    (cd "$checkout" && "$CODEPACT" review capture \
        --root . --config "$policy" --python-version 3.13 \
        --dependency-root "$dependency_root") > "$snapshot" || status=$?
    case "$status" in
        0|1) return 0 ;;
        *) return "$status" ;;
    esac
}

capture base-checkout "$CODEPACT_BASE_POLICY" "$CODEPACT_BASE_DEPENDENCY_ROOT" before.json || exit $?
capture candidate-checkout "$CODEPACT_CANDIDATE_POLICY" "$CODEPACT_CANDIDATE_DEPENDENCY_ROOT" after.json || exit $?
"$CODEPACT" review before.json after.json --json > review.json
```

Compare the distribution names, versions, and `artifact_sha256` values in each
capture's `foundations.providers` against the CodePact provider identities you
expect for that revision. The digest covers the provider inputs that CodePact
reads. For source-only checks, omit `--dependency-root` and use policies with no
configured dependency roots.

An exit of `1` from capture can contain findings relevant to preservation
review. Keep the comparator's exit status for the CI gate. A hash binds a
capture to its content. Authority for the reference and policy comes from the
host job. [Review options and exits](cli.md#compare-captured-source-analyses)

## Older releases

CodePact 0.3.0 lifecycle checks emit checker schema 1.
`inspect-contracts` emits inspection schema 0. Select a consumer based on
the command and report version. [Schema reference](schemas.md) ·
[Upgrade guide](upgrading.md)

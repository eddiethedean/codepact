<!-- release-recipes -->

# Pilot a package

Use the standalone 0.10.0 checker to compare three outcomes with the same
source-only policy. The example has two local modules. `ExportSession` gives
the analyzer a declared cleanup contract, and `export_receipt` borrows the
session. The sample uses fixed data in place of a real storage service.

```{raw} html
<p><a href="_static/codepact-pilot.zip" download>Download the complete pilot ZIP</a></p>
```

Extract the archive and open the directory containing `pyproject.toml`.
From a repository checkout, the same project is in `docs/_examples/pilot`.

```{literalinclude} _examples/pilot/pyproject.toml
:language: toml
```

## Confirm the finding

<!-- smoke: pilot-settings -->
```sh
uv tool run --from codepact-cli==0.10.0 codepact check --show-settings --json
```

<!-- smoke: pilot-advisory -->
```sh
uv tool run --from codepact-cli==0.10.0 codepact check --json > report.json
```

Settings show a `src` root, target `3.13`, and advisory mode. The check
exits `1` with a definite `TP1001` pointing to the borrow in
`job.py` and a related location for the close.

```{literalinclude} _examples/pilot/src/job.py
:language: python
:linenos:
```

## Repair before tightening policy

Replace `src/job.py` with `variants/repaired.py`. The repair moves the
borrow before cleanup and uses `finally` to keep cleanup on modeled exits.
Use `cp variants/repaired.py src/job.py` in a POSIX shell or
`Copy-Item variants/repaired.py src/job.py -Force` in PowerShell.

<!-- smoke: pilot-strict -->
```sh
uv tool run --from codepact-cli==0.10.0 codepact check --mode strict --json > report.json
```

Expect exit `0`, no diagnostics, and coverage
`complete-for-supported-subset`.

## Compare an incomplete result

Replace `src/job.py` with `variants/boundary.py`. It uses postponed
annotations on the borrowed parameter. Run the same check in advisory and
strict modes.

| Mode | Exit | What to read |
| --- | --- | --- |
| Advisory | `0` | `coverage.status` is `incomplete`. Read `coverage.reasons` for the unsupported form. |
| Strict | `1` | Strict policy fails on the same coverage boundary. |

For your own package, record the CLI version, selected root, time spent
annotating, coverage reasons, and confirmed findings. Keep the same inputs
while repairing findings. Enable a strict gate when the team accepts the
selected scope, warning threshold, and exception policy.
[Continue with project adoption](adopting.md) · [Run the policy in CI](automation.md)

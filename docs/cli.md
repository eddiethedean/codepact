# Command-line reference

This reference covers the CodePact 0.8.0 analyzer and its opt-in check modes.
The analyzer reads Python source without importing or executing the project.
[Release availability](availability.md)

| Task | Command |
| --- | --- |
| Check a source tree | `codepact check --root src` |
| Inspect declarations in supplied files | `codepact inspect-contracts --root src src/models.py src/client.py` |
| Preview effective project policy | `codepact check --show-settings --json` |
| Emit a report for CI | `codepact check --root src --mode strict --json` |
| Verify a provider wheel | `codepact contracts verify WHEEL` |
| Compare two provider wheels | `codepact contracts diff BEFORE_WHEEL AFTER_WHEEL` |
| Compare captured source analyses | `codepact review BEFORE AFTER` |
| Look up the installed rules and limits | `codepact capabilities --json` |

## Commands

```text
codepact --version
codepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] [--json] FILE...
codepact check [--root DIR] [--dependency-root DIR ...] [--config PYPROJECT|--no-config]
               [--python-version 3.11|3.12|3.13]
               [--experimental-ownership|--no-experimental-ownership]
               [--experimental-advanced-contracts|--no-experimental-advanced-contracts]
               [--experimental-finite-matches|--no-experimental-finite-matches]
               [--mode advisory|strict]
               [--fail-on error|warning] [--rules CODE[,CODE...]]
               [--severity CODE=LEVEL] [--baseline FILE]
               [--json|--output-format text|json|concise] [--max-diagnostics N]
               [FILE...]
codepact check [project/policy options] --show-settings --json
codepact baseline update [project/policy options]
                        --output FILE --reason TEXT [--replace] [--json] [FILE...]
codepact capabilities --json
codepact explain RULE --json
codepact contracts export --root DIR --distribution NAME --version VERSION
                          --import-scope PACKAGE [--python-version 3.11|3.12|3.13]
codepact contracts verify WHEEL
```

`inspect-contracts` keeps the inspection interface: it requires explicit files
and returns inspection schema 0. `check --json` uses adoption report schema 3
for source-only checks and schema 4 when foundations are enabled.
[JSON output reference](output.md)

## Compare captured source analyses

Run `codepact review capture` in each project revision. It accepts the check
options and writes a versioned JSON capture of the full selected root. Save a
capture even when its standalone check exits `1`. A capture retains policy,
source and provider identities, checked contracts, coverage and raw findings.
The comparison uses the same checker and analyzed Python target for both sides.
If the captures use an earlier comparison algorithm, recapture both revisions
with the same checker version before comparing them.

```sh
codepact review capture --root src > before.json
# Apply the source edit.
codepact review capture --root src > after.json
codepact review before.json after.json --json
```

Supply `--repair FINGERPRINT` for each finding the change is expected to
resolve. A preserved repair requires supported correspondence to the same
obligation and complete compatible evidence. Passing a review can leave an
inherited violation in the standalone after check. The report includes the
after check's exit status.

To verify the after capture against the current workspace, supply
`--current DIR` and repeat the after capture options after `--`. Paths in those
options resolve from `DIR`. The checker rebuilds the capture from current
inputs before claiming a workspace result.

```sh
codepact review before.json after.json --current . --json -- --root src
```

An after capture can also be reviewed as historical evidence with the two
capture files. Completed review exits are `0` for `passed` and `1` for
`failed`, `review-required` or `inconclusive`. Invalid input and execution
failures exit `2`. [Trusted CI capture](ci-reporting.md#review-a-change)

## Compare published wheel contracts

Run `codepact contracts diff BEFORE_WHEEL AFTER_WHEEL` on two local pure-Python
wheels with CodePact manifests. Read the result by public import path. The
comparison reports new caller obligations, weakened guarantees and unavailable
evidence. A retained public contract can keep its compatibility when its
implementation moves behind an explicit re-export. Add `--python-version` to
require a particular analyzed target or `--json` for the structured result.

The command exits `0` for a compatible comparison, `1` for a breaking change
or insufficient evidence, and `2` for invalid input or execution failure. It
reads wheel contents without installing or executing the provider.

## Project configuration and scope

`check` discovers Python files under the effective root. File operands add
inputs to discovery. To check a smaller source tree, select a narrower root
or include/exclude patterns.

| Setting | Resolution |
| --- | --- |
| Configuration file | `--config FILE` selects a file relative to the working directory. `--no-config` skips lookup. The options conflict. |
| Automatic lookup | Search upward from the command-line `--root` or, when absent, the working directory. Stop at the nearest `pyproject.toml`, even when it has no `[tool.codepact]` table. Parent tables are never merged. |
| Effective source root | Use command-line `--root` relative to the working directory, then the configured `root` relative to its config file. Without either, use the config directory if a CodePact table was loaded or the working directory otherwise. |
| File operands and baseline | File operands and `--baseline` paths are relative to the working directory. A `baseline` in TOML is relative to its config file. |
| Policy and rules | CLI scalars override the config, and `--rules` replaces the configured rule list. CLI severity settings override matching configured entries. |

```toml
[tool.codepact]
config-version = 1
root = "src"
python-version = "3.12"
include = ["**/*.py"]
exclude = ["generated/**"]
enabled-rules = ["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"]
mode = "advisory"
fail-on = "error"
baseline = "codepact-baseline.json"
experimental-finite-matches = true

[tool.codepact.dependencies]
roots = [".venv/lib/python3.12/site-packages"]
required-providers = ["billing-sdk"]

[tool.codepact.severity]
TP3001 = "warning"
```

Create the baseline with `baseline update` before adding its path.
Unknown keys, invalid patterns or rule codes, unsupported targets, and
severity overrides for mandatory or disabled rules fail configuration.
`--show-settings --json` reports each setting's origin without reading
source. It accepts no file operands or detail limit. It emits settings schema 3
when a runtime profile, project contracts, `uv-lock`, or
`experimental-foundations = true` is set. Other settings reports use schema 2,
including reports with only dependency roots configured.

## Experimental finite matches

`TP5001` is selected only when `experimental-finite-matches` is enabled with
the config setting or `--experimental-finite-matches`. The preview is off by
default. It checks supported finite `Result` and `Option` matches; guards leave
a fallthrough path, and unresolved identities or unsupported payload tests
remain coverage boundaries. The setting participates in report and baseline
identity.

## Installed contract providers

Add explicit installed environment roots under `[tool.codepact.dependencies]`
or repeat `--dependency-root DIR`. CLI roots replace configured roots. The
analyzer checks supported declarations and re-exports from installed regular
pure Python wheels with a `py.codepact` marker and matching `RECORD` entries.
A verified manifest can select public exports and publish supported contracts.
Project models can supply a reviewed factory and cleanup contract for an exact
installed version, including for an unmarked wheel. Local project modules and
earlier dependency roots shadow later ones.

Use `required-providers` to require a selected distribution's inline or
manifest publication. If the publication cannot be resolved, coverage is
incomplete. A project model can supply applicable effects for an unmarked
wheel while the publication requirement remains unsatisfied.
[Publish a manifest or configure a project model](reviewed-contracts.md)

For a nested project, select the intended policy explicitly. For example,
from the repository root, if `services/billing/pyproject.toml` contains the
table above and `services/billing/src` exists, resolve the project policy.

<!-- smoke: nested-settings -->
```sh
uv run --locked codepact check --config services/billing/pyproject.toml --show-settings --json
```

Expect `root` to end in `services/billing/src` and
`settings_origins.root` to be `pyproject.toml`. Use the same `--config` when you run
the actual check. With a command-line `--root`, the reported origin changes
to `command-line`.

Include/exclude patterns match root-relative paths using `/`; `*` and `?` match
within one path component and `**` crosses components. Excludes win. Discovery
skips symlinks and common build/environment directories. Explicit inputs must
remain under the root, pass the same include/exclude policy, and map to distinct
Python module names. The 10,000-file input limit counts selected Python files.
Discovery visits at most 100,000 filesystem entries, buffers at most 20,000
entries from any one directory to keep deterministic sorting bounded, and
descends at most 256 directories below the root. Exceeding a discovery limit
fails with status 2 and does not produce a completed check. Installed
packages are considered only through the explicit provider roots described
above.

## Reviewed runtime models

Configure `[tool.codepact.runtime-profile]` in `pyproject.toml` to select the
analyzed program's runtime. The 0.8 models target CPython 3.13.16 on Linux
x86-64 with source target `3.13`. The asyncio and multiprocessing models
also need the applicable ownership settings and runtime assumptions.
[Runtime profile and supported operations](reviewed-contracts.md#select-the-runtime-profile)

## Output modes

The default `text` format prints active findings and a full-count summary.
`--json` aliases `--output-format json` and writes one report to stdout; runtime
or configuration failures produce a structured failure envelope and status 2.
`concise` emits seven-field, tab-separated primary/related locations for
actionable findings and sends counts and coverage status to stderr. See the
[format and field definitions](output.md#location-per-line-format).

`--max-diagnostics N` must be a positive integer. It caps returned finding
details, not analysis, full counts, coverage, or exit calculation. The report
sets `details_truncated` when details were omitted. Rerun without the cap and
compare `snapshot_id` before using the full details.

## Policy and exits

| Exit | Meaning |
| --- | --- |
| `0` | Completed and passed the selected policy. |
| `1` | Completed and failed the selected policy. Read the report for the failing conditions. |
| `2` | Invocation, configuration, input, baseline, serialization, or other execution failure. |

| Condition in a completed report | Advisory | Strict |
| --- | --- | --- |
| Mandatory source error | Fails | Fails |
| Active definite finding at or above `fail-on` | Fails | Fails |
| Active definite finding below `fail-on` | Passes by itself | Passes by itself |
| Active possible finding | Fails if it meets `fail-on` | Fails |
| Incomplete coverage | Passes by itself | Fails |
| Eligible suppressed or disabled finding | Audit entry | Audit entry |

The default policy is advisory with `fail-on = "error"`. A definite
TP3001 has warning severity by default, so `--mode strict` can still exit
`0` for that finding. Add `--fail-on warning` to gate active warnings.
Severity overrides preserve certainty. Baselines and rule selection preserve
coverage boundaries. [CI report handling](ci-reporting.md)

## Baselines and inline suppressions

Use the explicit command to write a baseline:

```sh
codepact baseline update --output codepact-baseline.json \
  --reason "record findings before adoption"
```

An existing destination requires `--replace`. The update validates directives
and any previous baseline before atomically replacing the destination. Ambiguous
duplicate identities are omitted and counted; incomplete coverage is recorded.
Add `--json` to emit the versioned baseline-update summary. Normal `check`
commands never write a baseline. A stale or incompatible baseline is reported
but its entries do not suppress current findings.

A line-level suppression requires exact eligible rule codes and a reason:

```python
fetch()  # codepact: ignore[TP3001] -- the query is intentionally discarded
```

Only the finding whose primary span starts on that line is suppressed. The
finding remains in JSON audit output. Malformed/unused suppressions and
incomplete coverage remain visible; mandatory and boundary rules cannot be
suppressed.

## Must-use results

The initial result-use rule recognizes these synchronous declarations:

```python
from typing import Annotated
from codepact import discard, must_use
from codepact.contracts import MustUseResult
from codepact.rust import MustUse

@must_use
def fetch() -> int: ...

def fetch_typed() -> MustUse[int]: ...
def fetch_verbose() -> Annotated[int, MustUseResult()]: ...

fetch()                 # TP3001
value = fetch()         # immediate handling only
discard(fetch())        # explicit intentional discard
```

These forms carry no cleanup or ownership semantics. Assigned values are not
tracked after the assignment. Async/dynamic consumption and unsupported nested
expressions remain coverage boundaries. See
[rules](rules.md#tp3001-discarded-must-use-result) and the
[adoption guide](adopting.md).

## Read-only metadata commands

`codepact capabilities --json` lists schema versions, supported targets,
available rules, formats, hard limits and unavailable services. `codepact
explain RULE --json` returns a static explanation with valid/invalid/unknown
examples and advisory repair guidance. These commands do not inspect a project
or apply edits; no automatic fixes are advertised.

## Experimental ownership options

CodePact accepts `--experimental-ownership` and
`--no-experimental-ownership` on `check` and `baseline update`. The 0.6
advanced-contract preview adds `--experimental-advanced-contracts` and
`--no-experimental-advanced-contracts`; it requires ownership to be enabled.
The explicit flag overrides `experimental-advanced-contracts` in the project
configuration. Advanced mode adds TP2101 and TP2102 by default and is included
in report assurance and baseline scope.

```toml
[tool.codepact]
experimental-ownership = true
experimental-advanced-contracts = true
```

The TaskGroup model still requires `assume-default-task-factory = true` and a
compatible target. The experimental ThreadPoolExecutor model is listed only
for Python target 3.13 and uses the CPython 3.13.16 source pin. Strict mode
controls coverage policy independently of either opt-in.

Use the [ownership guide](ownership.md) for runnable examples and baseline
migration. The installed `capabilities --json` output gives the rules and
limits for that binary.

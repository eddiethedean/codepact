# Command-line reference

This reference covers the published CodePact 0.4.0 synthetic preview.
The analyzer reads Python source without importing or executing the project.
[Release availability](availability.md)

| Task | Command |
| --- | --- |
| Check a source tree | `codepact check --root src` |
| Inspect declarations in supplied files | `codepact inspect-contracts --root src src/models.py src/client.py` |
| Preview effective project policy | `codepact check --show-settings --json` |
| Emit a report for CI | `codepact check --root src --mode strict --json` |
| Look up the installed rules and limits | `codepact capabilities --json` |

## Commands

```text
codepact --version
codepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] --json FILE...
codepact check [--root DIR] [--config PYPROJECT|--no-config]
               [--python-version 3.11|3.12|3.13] [--mode advisory|strict]
               [--fail-on error|warning] [--rules CODE[,CODE...]]
               [--severity CODE=LEVEL] [--baseline FILE]
               [--output-format text|json|concise] [--max-diagnostics N]
               [FILE...]
codepact check [project/policy options] --show-settings --json
codepact baseline update [project/policy options]
                        --output FILE --reason TEXT [--replace] [FILE...]
codepact capabilities --json
codepact explain RULE --json
```

`inspect-contracts` keeps the inspection interface: it requires explicit files
and returns inspection schema 0. `check --json` uses the 0.4 CLI adoption
report, schema 2. The Rust `check` API returns `Inspection` (schema 0), and
`check_report` returns `CheckResult` (schema 1).
[JSON output reference](output.md)

## Project configuration and scope

`check` discovers sorted Python files below its effective root. File operands
are added to discovery; they do not narrow discovery. The default root is the
current directory. A `[tool.codepact]` table in the nearest `pyproject.toml`
can configure the root and policy. Use `--config FILE` to select one file, or
`--no-config` to ignore configuration discovery.

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

[tool.codepact.severity]
TP3001 = "warning"
```

The baseline path is relative to the configuration directory, not `src`; it
must be explicitly created with `baseline update`. Unknown keys, unsupported
targets, invalid patterns or rule codes, and severity changes to mandatory
rules are configuration errors. `--show-settings --json` resolves policy and
reports each setting's origin without reading source files. It accepts no file
operands or detail limit.

Include/exclude patterns match root-relative paths using `/`; `*` and `?` match
within one path component and `**` crosses components. Excludes win. Discovery
skips symlinks and common build/environment directories. Explicit inputs must
remain under the root, pass the same include/exclude policy, and map to distinct
Python module names. The 10,000-file input limit counts selected Python files.
Discovery visits at most 100,000 filesystem entries, buffers at most 20,000
entries from any one directory to keep deterministic sorting bounded, and
descends at most 256 directories below the root. Exceeding a discovery limit
fails with status 2 and does not produce a completed check.
CodePact does not search installed packages.

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

| Status | Meaning |
| --- | --- |
| `0` | Analysis completed and no finding or policy condition failed the selected policy. Advisory mode can return 0 with incomplete coverage. |
| `1` | A mandatory source error, an actionable finding, or a strict-mode possible finding/incomplete-coverage condition failed the policy. The completed report is preserved. |
| `2` | Invocation, configuration, baseline, input, serialization, or other execution failed; do not treat the result as a completed check. |

Advisory mode fails on findings at or above `--fail-on` (`error` by default).
Strict mode also fails on unsuppressed possible violations and incomplete
coverage. `TP3001` defaults to warning, so it does not fail the default
error-only threshold. Mandatory syntax/declaration errors cannot be disabled
or demoted. Severity and thresholds never change certainty.

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

The 0.5 development candidate accepts `--experimental-ownership` and
`--no-experimental-ownership` on `check` and `baseline update`. An explicit
flag overrides `experimental-ownership` in the project configuration.
The TaskGroup model additionally requires
`assume-default-task-factory = true` and a compatible runtime. Strict mode
controls the selected report policy independently of the ownership opt-in.

Use the [ownership guide](ownership.md) for runnable examples and baseline
migration. The installed `capabilities --json` output gives the rules and
limits for that binary.

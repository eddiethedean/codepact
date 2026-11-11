# Synthetic project controls

This corpus is a repeatable defect-discovery and regression set for the
CodePact analyzer. Every project is synthetic. The test suite reads these files
as source; it does not import or execute them.

## Projects

The eight paired projects have separate `buggy/` and `fixed/` trees. Each pair
uses the same configuration and provider modules; repairs change only
`src/main.py`. `manifest.json` records the independently specified expected
rule, function, certainty, rationale, and repair for each planted case.

| Project | Shape | Planted smells |
| --- | --- | --- |
| `csv_importer` | CSV command line tool | Abandoned and closed streams, invalid owned return, discarded validation result |
| `warehouse_loader` | Data loading job | Overwritten cursor, use after finish, invalid handoff, discarded query |
| `web_handler` | Request handler | Closing a borrowed session, abandoned session, use after release, ignored authorization result |
| `artifact_packager` | Artifact builder | Early-return leak, alias use after close, borrowed close, ignored result through a callable alias |
| `sdk_client` | SDK result handling | Ignored instance, static, class, and aliased callable results |
| `async_worker` | Async worker before its first suspension | Abandoned and closed links, invalid owned return, ignored decoder result |
| `plugin_runner` | Plugin host with named re-exports | Borrowed cleanup, abandoned token, invalid ownership return, ignored configuration, use after close through an imported cleanup helper |
| `report_exporter` | Report writer | Exceptional-path leak, use after finish, invalid handoff, ignored checksum |

The four boundary projects cover detached cleanup tasks, cancellation during
async close, dynamic plugin loading, and repeated close without an idempotency
model. The dynamic plugin fixture includes a file-write canary; a test fails if
analysis executes the source.

## Run the controls

Run all synthetic project controls on the three supported analysis targets:

```sh
cargo test -p codepact --test synthetic_projects --locked
```

The suite checks 33 independently specified definite findings across the
buggy project trees and confirms that the source repairs remove those findings
without changing the policy, model, selected files, configuration, or provider
contracts. It also compares fresh and reused library sessions, reversed input
order, repeated CLI output, advisory/strict boundary reports, and every
analyzed source file before and after the CLI invocation.

`make check` also runs Ruff and strict Pyright over every buggy, repaired, and
boundary source variant. This keeps planted semantic mistakes separate from
ordinary lint and type errors; it does not claim that specialized analyzers or
project-specific rules could not find similar issues.

To inspect one project manually, run:

```sh
cargo run --locked -- check \
  --config crates/codepact/tests/projects/csv_importer/buggy/pyproject.toml \
  --json

cargo run --locked -- check \
  --config crates/codepact/tests/projects/csv_importer/fixed/pyproject.toml \
  --json
```

The first report contains four expected findings; the repaired report has no
stable findings. The two projects are separate directories for manual use.
The test suite also installs the buggy and repaired source into the same
temporary workspace so it can verify that settings and policy fingerprints
remain identical.

Some repaired projects intentionally retain incomplete coverage. In
`artifact_packager`, lifecycle effects through a callable alias remain
unknown. In `sdk_client`, some class-method and callable-alias effects remain
unknown. Their advisory reports are not proof of a fully clean project. The
boundary controls verify that strict mode refuses a clean status when coverage
is incomplete.

## Catalog-inspired extension

The [code smells corpus](code_smells/README.md) adds a separate oracle for
0.5 candidate ownership checks and catalog-inspired lifecycle and must-use
examples. Run it with
`cargo test -p codepact --locked --test synthetic_projects code_smell_corpus`.

## Scope

These small programs exercise known contracts and controlled source repairs.
They do not estimate real-world precision, adoption effort, project diversity,
or the rate of actionable findings in existing repositories. They do not
qualify release gate A05 or replace its preregistered real-project pilot.
Every project and case is marked `synthetic` in the manifest, with
`real_project_pilot_credit` set to `false`.

The associated methodology and retained run evidence are documented in the
[synthetic project evaluation](../../../../docs/planning/phases/0.4/synthetic-projects-2026-11-09.md).

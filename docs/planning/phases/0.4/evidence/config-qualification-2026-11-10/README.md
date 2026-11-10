# V04-02 configuration controls — November 10, 2026

Technical result: **Pass, 23 independently expected controls.** Technical review:
Codex, 2026-11-10. Accountable maintainer release-gate signoff remains separate.

Source base: `84c099f` plus the resolved-contract identity fix. The [results](results.json)
retain all Rust source hashes, binary SHA-256 `be2dd8078aa8f2c3edd2e8e889f91024c4d08eaf4498b729631f865dc9cf5ffa`, runner hash,
host/interpreter, every TOML input, command, expected message fragment, exit,
stdout and stderr. The [runner](reproduce.py) asserts exit 2, a failure envelope
with configuration-or-execution category, an actionable key/value or parse
explanation and no completed diagnostics for invalid inputs.

Controls cover unknown keys, wrong scalar/list/table types, missing/unsupported
versions, invalid target/rule/severity/mode/threshold/path patterns, duplicate
keys/rules and conflicting config flags. Unrelated tool tables are accepted;
no CodePact table selects defaults. The source-side-effect marker remains absent.
The existing `nearest_config_wins_and_invalid_project_settings_fail_before_analysis`
and `project_config_wrong_types_and_invalid_values_fail_with_actionable_errors`
tests additionally cover these settings. This qualifies V04-02 only.

Reproduce from the repository root:

```sh
cargo build --locked
python docs/planning/phases/0.4/evidence/config-qualification-2026-11-10/reproduce.py target/debug/codepact /tmp/config-results.json
```

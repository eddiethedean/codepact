# JSON schemas and fixtures

CodePact publishes versioned schemas for each machine-readable interface. The
CLI emits a schema version in every completed report or failure envelope;
consumers should reject versions they do not support.

| Interface | Schema |
| --- | --- |
| Declaration inspection | [inspection schema 0](schemas/inspection-v0.schema.json) |
| Rust `CheckResult` | [checker result schema 1](schemas/check-result-v1.schema.json) |
| CLI check report | [adoption report schema 2](schemas/check-report-v2.schema.json) |
| CLI execution failure | [failure envelope schema 2](schemas/failure-v2.schema.json) |
| Baseline file | [baseline schema 1](schemas/baseline-v1.schema.json) |
| Baseline update summary | [baseline-update schema 1](schemas/baseline-update-v1.schema.json) |
| Capability list | [capability schema 1](schemas/capabilities-v1.schema.json) |
| Rule explanation | [explanation schema 1](schemas/explanation-v1.schema.json) |
| Resolved settings | [settings schema 1](schemas/settings-v1.schema.json) |

Valid and invalid JSON fixtures are maintained in the
[repository fixture directory](https://github.com/eddiethedean/codepact/tree/main/docs/schemas/fixtures).
The fixtures illustrate version boundaries and rejection behavior; they do not
qualify release or pilot gates.

# JSON schemas and fixtures

CodePact publishes versioned schemas for each machine-readable interface. The
CLI emits a schema version in every completed report or failure envelope;
consumers should reject versions they do not support.

| Interface | Schema |
| --- | --- |
| Declaration inspection | [inspection schema 0](schemas/inspection-v0.schema.json) |
| Rust `CheckResult` | [checker result schema 1](schemas/check-result-v1.schema.json) |
| CLI check report | [adoption report schema 3](schemas/check-report-v3.schema.json) |
| CLI execution failure | [failure envelope for report schema 3](schemas/failure-v3.schema.json) |
| Baseline file | [baseline schema 1](schemas/baseline-v1.schema.json) |
| Baseline update summary | [baseline-update schema 1](schemas/baseline-update-v1.schema.json) |
| Capability list | [capability schema 1](schemas/capabilities-v1.schema.json) |
| Rule explanation | [explanation schema 1](schemas/explanation-v1.schema.json) |
| Resolved settings | [settings schema 2](schemas/settings-v2.schema.json) |

Valid and invalid JSON fixtures are maintained in the
[repository fixture directory](https://github.com/eddiethedean/codepact/tree/main/docs/schemas/fixtures).
The fixtures illustrate version boundaries and rejection behavior; they do not
qualify release or pilot gates. Schema 2 reports and failures remain available
for older consumers. Schema 3 adds explicit provider selection and the
finite-match setting; the Rust `CheckResult` remains schema 1.


## Experimental ownership scope

Schema 2 accepts `experimental-ownership-checking-for-supported-exit-flow` in
`assurance`. Experimental rule selection requires that assurance value. The
settings response exposes `experimental_ownership` and
`assume_default_task_factory`. Schema 1 capabilities add the experimental
rule tier, and schema 1 baselines can record `ownership` and
`ownership_model_revision`. Update strict consumers before enabling ownership.
Earlier baselines remain readable and cannot match across ownership scopes.

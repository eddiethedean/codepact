# JSON schemas and fixtures

CodePact publishes versioned schemas for each machine-readable interface. The
CLI emits a schema version in every completed report or failure envelope;
consumers should reject versions they do not support.

```{include} _generated/schema-table.inc
```

Valid and invalid JSON fixtures are maintained in the
[repository fixture directory](https://github.com/eddiethedean/codepact/tree/main/docs/schemas/fixtures).
The fixtures illustrate version boundaries and rejection behavior; they do not
qualify release or pilot gates. Schema 2 reports and failures remain available
for older consumers. Schema 3 adds explicit provider selection and the
finite-match setting. CodePact 0.9.0 emits CLI report schema 4 and Rust result
schema 2 for foundations checks. These schemas carry the resolved scope,
provider and model-use evidence, and versioned comparison descriptor.
Execution failures still use the schema 3 failure envelope.
[Configure foundations checks](reviewed-contracts.md)


## Experimental ownership scope

Schema 2 accepts `experimental-ownership-checking-for-supported-exit-flow` in
`assurance`. Experimental rule selection requires that assurance value. The
settings response exposes `experimental_ownership` and
`assume_default_task_factory`. Schema 1 capabilities add the experimental
rule tier, and schema 1 baselines can record `ownership` and
`ownership_model_revision`. Update strict consumers before enabling ownership.
Earlier baselines remain readable and cannot match across ownership scopes.

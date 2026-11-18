# Choose an integration

| Goal | Start here | What you own |
| --- | --- | --- |
| Annotate resources, moves, and result handling in application code | Install the [Python helpers](install.md#helper-only-installation). See the [Python API](python-api.rst). | Accurate declarations and runtime cleanup. |
| Check a package from a shell or CI | Use the [CLI](cli.md) on a selected root. | Source target, scope, and failure policy. |
| Read a check from another tool | Consume [versioned CLI JSON](output.md). | Schema validation, exit status, coverage, and artifact handling. |
| Embed checking in a Rust application | Use the [Rust API](rust-api.md#choose-an-entry-point). | Source and module inputs, dependency verification, cancellation, and policy. |
| Publish a contract for downstream callers | [Build and verify a provider wheel](publish-provider.md). | Package markers, manifest bytes, and the versioned caller contract. |

Check the [compatibility matrix](compatibility.md) before choosing a runtime
profile. For a first package, the [pilot](pilot.md) demonstrates the same
finding, repair, and coverage reasoning used by all check integrations.

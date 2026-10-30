# Typepact

**Stronger types. Safer Python.**

Typepact is a planned Python typing and contract analysis project with two first-class interfaces: a Pythonic API and an explicit Rust-inspired API. Both feed one shared semantic analyzer implemented in Rust.

**Status: design and planning.** No package, analyzer, or CLI is implemented in this repository yet. All APIs and commands below are proposals.

Start with the [planning index](docs/planning/README.md), [product vision](docs/planning/vision.md), and [implementation phases](docs/planning/implementation.md).

The first milestone will define a resource with `@resource`, pass it to a function using `Borrowed[T]`, and diagnose use after close across both interfaces. Stronger ownership and lifetime checks follow only when their analysis boundaries are demonstrated.

Proposed identities: Python distribution `typepact`, explicit namespace `typepact.rust`, Rust engine crate `typepact`, and command `typepact check`. Registry availability and publishing ownership must be confirmed before release.

See [contributing](CONTRIBUTING.md) for the development plan and [limitations](docs/planning/limitations.md) for what the analyzer can and cannot establish.

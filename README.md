# TypePact

**Stronger types. Safer Python.**

TypePact provides Pythonic and explicit Rust-inspired contract declarations with a shared Rust inspector.

**Status: 0.1 analyzer implementation complete; candidate qualification remains open.** The inspector uses Ruff AST declarations with source ranges, target-version checks, captured contract views, and direct class or alias imports from explicitly supplied modules. Salsa tracks per-file parsing and per-module semantic summaries, with cancellation, revision-tagged session results, and bounded input/work/cache budgets. The inspector reports declarations only; it does not check resource flow or runtime safety.

Start with the [planning index](docs/planning/README.md), [product vision](docs/planning/vision.md), and [implementation phases](docs/planning/implementation.md).

The Python package supports Python 3.11 and newer. Install the lightweight helpers with `python -m pip install .`. Build and run the inspector with `cargo run -p typepact -- inspect-contracts --root . examples/contracts.py`. The parser reads source as text and never imports the inspected program.

The product name is **TypePact**; the Python package name remains `typepact`.

Implemented identities: Python distribution `typepact`, explicit namespace `typepact.rust`, Rust crate and command `typepact`. The inspector command is `typepact inspect-contracts`.

See [contributing](CONTRIBUTING.md) for the development plan and [limitations](docs/planning/limitations.md) for what the analyzer can and cannot establish.

The supported syntax and current boundaries are documented in the [0.1 inspector guide](docs/inspector.md). Flow-sensitive use-after-close checking is planned for phase 0.2.

Follow the [0.x release roadmap](ROADMAP.md) for versioned phases and required release gates.

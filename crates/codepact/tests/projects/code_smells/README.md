# Catalog-inspired CodePact corpus

This corpus adapts ideas from the [Code Smells Catalog](https://www.codesmells.org/)
into original Python projects. The catalog names the design context. The CLI
checks the concrete contract violation listed for each example.

## Bug and repair pairs

Each pair has `buggy/` and `fixed/` project directories. Both use the same
configuration and provider source. The repair changes `src/main.py`.
[The manifest](manifest.json) records the expected rule, source site, certainty,
completion kind, and any analysis boundaries.

| Project | What the annotation makes checkable | Catalog entry |
| --- | --- | --- |
| `batch_teardown` | A spool is leaked on early return or raise, used after its context exits, or returned closed under `Owned`. A conditional close adds possible findings. | [Required Setup or Teardown Code](https://www.codesmells.org/smells/required-setup-or-teardown-code) |
| `delivery_status` | Three boolean outcomes are dropped despite `@must_use`, `MustUse[bool]`, or `Annotated[bool, MustUseResult()]`. The repair branches on or forwards the result. | [Afraid To Fail](https://www.codesmells.org/smells/afraid-to-fail) |
| `preview_side_effects` | A preview closes a `Borrowed` decoder through an alias. Another helper closes a resource the caller still uses. | [Side Effects](https://www.codesmells.org/smells/side-effects) |
| `global_receipt` | A global cache retains a receipt borrowed for one call. The repaired caller keeps access inside the borrow region. | [Global Data](https://www.codesmells.org/smells/global-data) |
| `exposed_cursor` | A returned list or callable exposes a borrowed cursor. The repair returns rows read during the borrow. | [Indecent Exposure](https://www.codesmells.org/smells/indecent-exposure) |
| `aliased_upload` | An attempted `move()` conflicts with a live alias, and a separate path uses its original handle after a move. | [Mutable Data](https://www.codesmells.org/smells/mutable-data) |
| `forwarded_buffer` | A relay passes a buffer to an owning callee without an explicit move. The original cleanup duty remains outstanding. | [Tramp Data](https://www.codesmells.org/smells/tramp-data) |
| `async_index` | Cleanup starts after a cancellation request or inner group exit while another task borrow remains outstanding. The repair closes the index after group unwinding. | [Required Setup or Teardown Code](https://www.codesmells.org/smells/required-setup-or-teardown-code) |

## Boundary and comparison projects

- `opaque_factory` leaves a resource constructor and its cleanup effects
  unresolved. `detached_index` passes a borrow to detached work. Both report
  incomplete coverage and fail in strict mode. [Hidden Dependencies](https://www.codesmells.org/smells/hidden-dependencies) · [Required Setup or Teardown Code](https://www.codesmells.org/smells/required-setup-or-teardown-code)
- `unmarked_status` has byte-identical caller source to the buggy
  `delivery_status` project. Remove the three provider annotations and the
  three TP3001 findings disappear. The comparison isolates the annotation as
  the evidence for the findings. [Afraid To Fail](https://www.codesmells.org/smells/afraid-to-fail)
- `design_controls` contains duplicate timeout logic, a flag argument and
  unnamed constants. These are design review examples. The CLI reports a
  clean result for its supported subset. [Duplicated Code](https://www.codesmells.org/smells/duplicated-code) · [Flag Argument](https://www.codesmells.org/smells/flag-argument) · [Magic Number](https://www.codesmells.org/smells/magic-number)

## Run from the repository root

Use the 0.5 candidate CLI for the full corpus. Project settings enable
experimental ownership and the default TaskGroup runtime assumption where
those checks apply.

```sh
cargo test -p codepact --locked --test synthetic_projects code_smell_corpus

codepact check --config crates/codepact/tests/projects/code_smells/delivery_status/buggy/pyproject.toml --mode strict --json
codepact check --config crates/codepact/tests/projects/code_smells/delivery_status/fixed/pyproject.toml --mode strict --json
```

The first CLI invocation reports three TP3001 findings and exits 1. The
repaired source exits 0 with complete coverage for the supported subset.
For a source build, substitute `cargo run --locked --` for `codepact`.

To add a smell, choose the observable behavior first. Put the contract in
shared provider source where practical, write the buggy call and a source
repair, then add the expected rule and location to the manifest. Use a
boundary example when the relevant effect is unresolved, and a comparison
project when the purpose is to show the analyzer's scope. The tests run all
variants against source targets 3.11, 3.12, and 3.13. These are synthetic
regression examples.

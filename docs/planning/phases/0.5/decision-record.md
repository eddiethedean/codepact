# 0.5 ownership decisions

These are the implementation choices for the 0.5.0 development candidate.
The [release evidence](../../../releases/0.5.0-evidence.md) records local
qualification and the remaining release review.

| Topic | Candidate choice |
| --- | --- |
| Activation | `experimental-ownership = true` in config schema 1, explicit on/off CLI overrides, and the additive Rust `Ownership` option. Strict mode applies policy to the selected analysis. |
| Public helper | `codepact.move(value: T) -> T` preserves Python identity and ordinary typing. The analyzer resolves the declaration and evaluates the transfer at the argument expression. |
| Transfer | Preserve resource identity and lifecycle, invalidate the source binding, carry the original cleanup duty and related sites, and apply callee effects on their matching completion edges. |
| Borrow | Preserve ordinary Python aliases. Shared views remain local to verified calls or tracked coroutine/task captures. Unknown escaping effects retain incomplete coverage. |
| Recursion and limits | Iterate resolved, invocation-specific effect summaries from unreachable exits. Limit an active fixed point to 16 passes under the per-file flow-work budget. Bound ownership tracking at 4,096 handles, 32 active task groups, and 128 retained coroutine records and scheduled captures. |
| Runtime assumption | Select the TaskGroup model explicitly for the running default CPython loop and non-eager task factory. Use source pins v3.11.15, v3.12.13 and v3.13.16. Known task-factory mutation and unresolved membership retain boundaries. |
| Reports and baselines | Add TP2002–TP2006 to the experimental tier, retain inspection schema 0 and checker report schema 2, and include the semantic scope and applicable model revision in matching. |
| Qualification | Use a synthetic ownership preview with independent bug/repair and boundary oracles. Keep inherited 0.4 controls and report field usefulness as unqualified. |

The checker and Python helpers remain source-only. The runtime probes execute
controlled TaskGroup fixtures separately from target-project analysis.
[Specification](specification.md) · [V05 matrix](validation.md)

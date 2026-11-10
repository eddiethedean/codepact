# Baseline contract counterexample — November 10, 2026

The [retained results](results.json) reproduce a stale baseline suppression on
the exact frozen candidate wheel from Release run 42821539177 and demonstrate
the correction on the optimized development binary. Each record retains the
binary hash, baseline document, source, commands, exits and full reports.

The acquisition and exit in `main.py` stay unchanged. The resource declaration
in `provider.py` changes from `closed_by='close'` to `closed_by='release'`.

| Build | Original finding | Changed contract | Fingerprint |
| --- | --- | --- | --- |
| Frozen `317ef90`, algorithm 3 | Suppressed; exit 0 | Incorrectly suppressed; exit 0 | Unchanged |
| Corrected development source, algorithm 4 | Suppressed; exit 0 | Active TP1002; exit 1 | Changed |

The [regression tests](../../../../../../crates/codepact/tests/phase04.rs)
also change the resolved import while retaining both resource definitions,
preserve a harmless moved declaration, reject algorithm 3, compare fresh/reused
sessions, and reject baseline creation when internal evidence has been lost by
deserializing public JSON. Public inspection and checker schemas retain their
existing fields.

Reproduce from the repository root with the original artifact and a freshly
built optimized corrected binary:

```sh
python docs/planning/phases/0.4/evidence/baseline-contract-2026-11-10/reproduce.py /path/to/original/codepact target/release/codepact /tmp/baseline-counterexample.json
```

The [runner](reproduce.py) expects the old defect and the corrected result;
this is an analyzer regression control, not a real-project A05 finding. Source
identity for the corrected binary is also retained in the adjacent
[operational measurements](../adoption-cli-2026-11-10/README.md).

Required check commands, exits and source hashes are in [checks.json](checks.json).
The [non-docs required-check transcript](required-checks.log.gz) retains all
passed targets and the sandbox DNS failure at the final docs step. The separate
[successful documentation transcript](docs-checks.log.gz) completes the checks
after pinned dependencies were cached and the new evidence link was corrected.

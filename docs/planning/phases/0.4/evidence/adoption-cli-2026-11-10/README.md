# Adoption CLI measurements — November 10, 2026

Status: **14 covered CLI workloads pass their independent oracles. G4 remains pending.**
Seven optimized samples per workload extend the [earlier operational evidence](../../performance-2026-11-09.md).
The [results](results.json.gz) retain every duration, source/config/baseline hash,
output digest and size, binary identity and Rust source-file hash. The source
base is `84c099f` plus the resolved-contract identity correction. Those hashes
identify the exact measured development source; this is not a new release freeze.

- Binary SHA-256: `f6727edbc1857617a726be1ef37901ad42ee37869fdf6fe4f41db1a3b29d4dc4`.
- Host: macOS-26.5.2-arm64-arm-64bit; Python 3.11.14.
- [Environment](environment.json): rustc 1.96.0 (ac68faa20 2026-05-25); cargo 1.96.0 (30a34c682 2026-05-25); arm64; model unavailable because sandbox denied sysctl.
- Peak child RSS across all children: 72.09 MiB.
  This includes preparation and all cases; it is not per-case or retained-session memory.

| Workload | Median ms | Range ms | Output bytes | Exit |
| --- | ---: | ---: | ---: | ---: |
| mustuse-json | 68.314 | 61.326–72.678 | 289921 | 1 |
| mustuse-limited | 68.514 | 63.230–77.188 | 19107 | 1 |
| mustuse-text | 61.420 | 60.169–68.016 | 65026 | 1 |
| mustuse-concise | 60.566 | 57.985–68.067 | 41148 | 1 |
| baseline-dense | 62.708 | 60.362–65.510 | 289207 | 0 |
| baseline-empty | 63.928 | 60.261–71.042 | 289942 | 1 |
| baseline-changed | 65.921 | 60.411–83.407 | 290370 | 1 |
| baseline-ambiguous | 61.285 | 59.851–69.151 | 290436 | 1 |
| config-33 | 3.602 | 3.386–5.074 | 4549 | 0 |
| config-65536 | 4.314 | 3.858–5.057 | 4555 | 0 |
| discovery-10 | 4.527 | 4.230–4.807 | 5775 | 0 |
| discovery-10000 | 2377.535 | 2296.879–3340.088 | 1484301 | 0 |
| source-bytes-1000000 | 5.824 | 5.680–6.712 | 4452 | 0 |
| source-bytes-1000001 | 2.479 | 2.366–2.808 | 376 | 2 |

Each sample launches a fresh CLI process, including launch/output-capture cost.
OS caches are not flushed. The [runner](reproduce.py) asserts independent expected
exits, full/returned finding counts, matching counts and supported coverage.
Limited output still fails on all 250 underlying findings. Changed-site matching
leaves one new finding active; duplicate insertion leaves both ambiguous sites
active. Exact config/source byte limits and 10,000 selected files succeed;
one excess source byte returns 2 without a completed report. All seven output
hashes agree per workload, and target file hashes remain unchanged.

Reproduce from the repository root:

```sh
cargo build --release --locked
python docs/planning/phases/0.4/evidence/adoption-cli-2026-11-10/reproduce.py target/release/codepact /tmp/adoption-results.json
```

These synthetic operational controls do not count toward A05. Warm-session
policy/report costs with query-event evidence, final-candidate inherited 0.3
comparisons, per-case peak/retained memory and pilot workloads remain required.

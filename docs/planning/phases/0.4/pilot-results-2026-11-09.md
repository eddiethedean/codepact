# 0.4 pilot scan results — candidate `620656b`

Status: **Partial scans recorded; A05 remains pending.** No findings have been
labeled and no actionability percentage is claimed. The six frozen revisions
were replayed on exact implementation candidate `83ae561` on 2026-11-10; see
the [current-candidate replay](evidence/2026-11-10-candidate-83ae/pilot/README.md).

## Run identity

- Candidate source revision: `620656b8a8a28ac57a29e589164dc9b58c8141d8`.
- CLI version: `codepact 0.4.0`; executable SHA-256:
  `1aaf255de45636a3b3c779b8cc8d0348d286b163a1ff095b35f63f3b4e29dd5d`.
- Host: Darwin 25.5.0 arm64; Python 3.11.14; rustc 1.96.0.
- Project revisions, trees, reasons for selection, source inventory and frozen
  settings: [pilot preregistration](pilot-inventory.md). The three expansion
  candidates were added before their scans in the amendment recorded there.
- The same command was used for every project:

  ```text
  target/debug/codepact check --root PROJECT --no-config --python-version 3.11 --mode advisory --fail-on error --rules TP1001,TP1002,TP1003,TP2001,TP3001 --output-format json
  ```

- No project dependencies were installed. No project source was imported or
  executed, and no project tests or hooks were run. Full default Python-file
  discovery was used as preregistered.

## Results

| Project | Shell exit / report status | Analysis result | Eligible definite findings |
| --- | --- | --- | ---: |
| Requests | `0` / completed | 37 files; 5,006 diagnostics, all TP9001 with unknown certainty; coverage incomplete, 407,127 source bytes and 5,006 coverage reasons. Snapshot `d2b9b08e53d4d7784f909f97e9e6ee3832335dff2bff7a64ec0535c37a95cd60`. | 0 |
| pip | `2` / failed before analysis | Discovery reached `build-project/build-project.py`; module derivation rejected its hyphenated path component. No findings or source files were analyzed. | Not available |
| Uvicorn | `0` / completed | 84 files; 10,782 diagnostics, all TP9001 with unknown certainty; coverage incomplete, 629,571 source bytes and 8,785 coverage reasons. Snapshot `5ad8449ed81bfae13cc0e1376a1818a08508e420d3464bd89e4d3d0b9e74bd03`. | 0 |
| Flask | `0` / completed | 83 files; 7,400 diagnostics, all TP9001 with unknown certainty; coverage incomplete, 591,255 source bytes and 7,400 coverage reasons. Snapshot `da3cb4b5da7e1a9d9c574e431eff15f0910d95be475e561379560402d32b0b25`. | 0 |
| Click | `2` / failed before analysis | Full-discovery Python source exceeds the candidate's documented 1,000,000-byte request budget. The report identifies `tests/test_utils/test_style.py` as the next file that could not be read. | Not available |
| FastAPI | `2` / failed before analysis | Full-discovery Python source exceeds the candidate's documented 1,000,000-byte request budget. The report identifies `fastapi/security/oauth2.py` as the next file that could not be read. | Not available |

All diagnostics in the three completed reports are TP9001 boundaries, not
eligible TP1001/TP1002/TP1003/TP2001/TP3001 definite violations. The eligible
population across all six repositories is unknown because three analyses did
not complete. No sample was drawn and no finding was labeled. Therefore the
minimum-30 denominator, three-project reviewed-project condition and 90%
actionability threshold cannot be evaluated; A05 does not pass.

The pip path failure and Click/FastAPI byte-budget failures are retained as
candidate behavior under the frozen default-discovery scope. This result does
not silently exclude files, narrow scope, change limits or reinterpret a
failure as a zero-finding result. Any amended scope or new candidate requires
its own dated preregistration and consistently rerun evidence before labeling.

## Retained reports

Each compressed artifact is a lossless gzip of the analyzer's JSON stdout.
SHA-256 values in the final column refer to the uncompressed JSON, so the
decompressed content can be checked independently. The failure reports contain
the schema's failed-execution envelope; the shell exit is captured separately.

| Project | Compressed report | Gzip SHA-256 | Uncompressed bytes / JSON SHA-256 |
| --- | --- | --- | --- |
| Requests | [JSON.gz](evidence/2026-11-09-candidate-620656/requests-report.json.gz) | `f4f6dfcad248a6e575e21438ea1b678f63fd6c8d03c5d54e00484ee0cb723bbc` | 5,353,964 / `193c520a7df6e2aeddf2cf0441201b15dfeb2b004e28f35f6b7d17de92d4afc4` |
| pip | [JSON.gz](evidence/2026-11-09-candidate-620656/pip-report.json.gz) | `ec88c52d789f1b4e8f2cc76cf5016cc284f38091de9d1108ae31b5edd6a94e0d` | 288 / `fe4e5e910895258bd1fb25224c3b8738016bbbd4fa2084f6022615ee826bd00b` |
| Uvicorn | [JSON.gz](evidence/2026-11-09-candidate-620656/uvicorn-report.json.gz) | `970c63b1b55114ec54135860a5ce96875a1595eba546046a29232f2e8edfedbc` | 11,541,215 / `16c3187299c410d6d6ea2a052ebdba7445d55d3a16805b19192e346a8e0588e1` |
| Flask | [JSON.gz](evidence/2026-11-09-candidate-620656/flask-report.json.gz) | `68e2deef64ffd47bde31dc2b75969f26acfd22e4cc12d3fb5d2351813218b664` | 7,902,127 / `e6101d03d2e15b897edb11f67d181cc8b9ca25da4cd79568cea7c341ecaf3cb8` |
| Click | [JSON.gz](evidence/2026-11-09-candidate-620656/click-report.json.gz) | `98658c91d154da01f3e308c1cc05946c159eeb1e25a874f8b212a8cbedca6130` | 322 / `4315d1b86553ea08aae47dd2c828fe9515b32694ff5cb7328bcad234c9b9037e` |
| FastAPI | [JSON.gz](evidence/2026-11-09-candidate-620656/fastapi-report.json.gz) | `c49975fa8002754a5d5d6aab2a219c2f3ddb8a26d0321af6e10ccce409f20f76` | 320 / `6534c521b83c8b803e2020614fe9529978818443137d40ff1ca91a0d5fbb27c5` |

## Exact implementation-candidate replay — `c0ea48a`

The same six pinned, clean checkouts were replayed with the exact macOS arm64
CLI wheel produced by [build-only Release run 96](https://github.com/eddiethedean/codepact/actions/runs/42715489947)
for implementation candidate `c0ea48a2fae771512e7c0010ba235ea9f7136e8d`.
The frozen command, Python 3.11 target, rules, advisory/error policy and full
default discovery from the preregistration were unchanged. The wheel SHA-256
was `426978c5b1a5a177cd917bcd4da5e143f3fb76233c4c7312ec91253daf5bb584`.
Each checkout's commit and tree matched the pinned inventory and was clean
before replay. The exact reports and individual details are retained in the
[candidate replay record](evidence/2026-11-09-candidate-c0ea48/pilot/README.md).

| Project | Result | JSON bytes / SHA-256 | Retained report |
| --- | --- | --- | --- |
| Requests | Completed; 0 eligible definite findings; 5,006 TP9001 boundaries | 5,354,054 / `df7b809b939f784dab836cc86d39bd17b8b27159afffbb5673453b411b710c17` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/requests-report.json.gz) |
| pip | Failed before analysis: invalid module component in `build-project/build-project.py` | 288 / `fe4e5e910895258bd1fb25224c3b8738016bbbd4fa952f57e2fe9b41d2988016` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/pip-report.json.gz) |
| Uvicorn | Completed; 0 eligible definite findings; 10,782 TP9001 boundaries | 11,541,305 / `4d420d7f95d8f77d2c1198ed3f8bc5c807e2aa7cbe877c253fb1d6aa19a46bda` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/uvicorn-report.json.gz) |
| Flask | Completed; 0 eligible definite findings; 7,400 TP9001 boundaries | 7,902,217 / `af21aa17889cff4e2cc85b8f7ef810ff26e95be75f4e14db8f0b9ad7a939c3b5` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/flask-report.json.gz) |
| Click | Failed before analysis: 1,000,000-byte total source budget exceeded before `tests/test_utils/test_style.py` | 322 / `4315d1b86553ea08aae47dd2c828fe9515b32694ff5cb7328bcad234c9b9037e` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/click-report.json.gz) |
| FastAPI | Failed before analysis: 1,000,000-byte total source budget exceeded before `fastapi/security/oauth2.py` | 320 / `6534c521b83c8b803e2020614fe9529978818443137d40ff1ca91a0d5fbb27c5` | [JSON.gz](evidence/2026-11-09-candidate-c0ea48/pilot/fastapi-report.json.gz) |

The exact-candidate replay confirms the earlier scan pattern. The full eligible
population remains unknown because three projects did not complete and the
completed projects yielded no eligible definite findings. No sample was drawn
or labeled; the minimum-30 and 90%-actionable criteria remain unevaluated.
A05 remains pending.

## Exact implementation-candidate replay — `83ae561`

The same six pinned project revisions, command, full default discovery, Python
3.11 target, stable rule set and advisory/error policy were replayed using the
macOS arm64 CLI wheel from [build-only Release run
42729735141](https://github.com/eddiethedean/codepact/actions/runs/42729735141)
on exact candidate `83ae561cfce8600159f53d3db4cc94892cad9db8`. The wheel SHA-256
is `ce26dad93f6f8c410920454460b8b27158da7303f03ed25b9e3156100cda2cb0`.
Checkout commits and trees matched the preregistration and were clean. Each
current-run report's JSON and gzip hashes match its retained `c0ea48a` report
byte for byte; the current-run summaries are recorded in the
[replay evidence folder](evidence/2026-11-10-candidate-83ae/pilot/README.md).

The result is unchanged: Requests, Uvicorn and Flask completed with zero
eligible definite findings; pip failed module-name derivation, and Click and
FastAPI exceeded the 1,000,000-byte source budget before analysis. The
eligible population remains unknown, no sample was drawn or labeled, and A05
remains pending.


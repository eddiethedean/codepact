# Exact-candidate pilot replay on `83ae561`

Status: **The frozen six-project replay completed or failed exactly as
reported below; A05 remains pending.** The three completed scans returned no
eligible definite findings. Three projects failed before analysis. The
eligible population is therefore unknown, and no sample, labels or
actionability percentage is claimed.

## Candidate, project selection and method

- CodePact source candidate: `83ae561cfce8600159f53d3db4cc94892cad9db8`.
- Executed package: macOS arm64 CLI wheel from build-only [Release run
  42729735141](https://github.com/eddiethedean/codepact/actions/runs/42729735141),
  SHA-256 `ce26dad93f6f8c410920454460b8b27158da7303f03ed25b9e3156100cda2cb0`.
- `codepact --version`: `codepact 0.4.0`; host: Darwin arm64; Python 3.11.14.
- The three initial pinned revisions remain Requests
  (`3c7baaa05ab4aa49cc819ce3a819d260de19c157`), pip
  (`4b71f36fb4dfbc87ce65ac4710eeccf81d379f55`) and Uvicorn
  (`a99c1b76ee9caea67ff98e2ccc457bd372db193f`), selected before analysis for
  the synchronous-library, CLI and asynchronous-server roles. Per the
  preregistered expansion rule, Flask (`49943b5a94413a530761350f04e896f76e8d620e`),
  Click (`8abb77a8432e03ed14a95f972b44f905279415a7`) and FastAPI
  (`534961812bef29ad80d2f7c399e52c688e8a0d57`) were also retained. Their
  original checkout revisions, trees and selection reasons are in the
  [preregistration](../../../pilot-inventory.md).
- Every checkout was clean at its pinned revision before analysis. The command,
  target, selected stable rules, advisory/error policy and full default
  discovery were unchanged from the preregistration:

  ```text
  codepact check --root PROJECT --no-config --python-version 3.11 --mode advisory --fail-on error --rules TP1001,TP1002,TP1003,TP2001,TP3001 --output-format json
  ```

- No project dependency was installed, and no project source, hook or test was
  imported or executed. The analyzer read source as data.

## Results

| Project | Pinned tree | Result | Selected Python files / bytes | Diagnostics | Eligible definite findings |
| --- | --- | --- | ---: | --- | ---: |
| Requests | `0c8b989` | Completed; coverage incomplete | 37 / 407,127 | 5,006 TP9001 boundaries | 0 |
| pip | `9eb70d4` | Failed before analysis: module name cannot be derived for `build-project/build-project.py` | 0 / 0 | Failure envelope only | Unknown |
| Uvicorn | `8fbdd8d` | Completed; coverage incomplete | 84 / 629,571 | 10,782 TP9001 boundaries | 0 |
| Flask | `6751ab7` | Completed; coverage incomplete | 83 / 591,255 | 7,400 TP9001 boundaries | 0 |
| Click | `84808be` | Failed before analysis: full-discovery scope exceeded the 1,000,000-byte source budget | 0 / 0 | Failure envelope only | Unknown |
| FastAPI | `3cec183` | Failed before analysis: full-discovery scope exceeded the 1,000,000-byte source budget | 0 / 0 | Failure envelope only | Unknown |

The scan does not establish a usable 30-finding population. Three full reports
contain only TP9001 unknown-boundary diagnostics and zero definite TP1001,
TP1002, TP1003, TP2001 or TP3001 findings; three scopes did not complete. No
sample was drawn or labeled. The accepted 30-finding, three-reviewed-project
and 90%-actionable criteria cannot be evaluated. No files or projects were
removed from this replay to produce a completed run.

## Retained artifacts

Each compressed report is a lossless gzip of the exact analyzer stdout. Its
JSON digest is over the decompressed bytes; its gzip digest identifies the
retained report. The adjacent summary JSON records project tree, exit/status,
counts, coverage and both hashes.
All six retained JSON documents validate against the published checker schema
v2 (completed reports) or failure schema v2 (failed runs).

| Project | Report | Summary | JSON bytes / SHA-256 | Gzip SHA-256 |
| --- | --- | --- | --- | --- |
| Requests | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/requests-report.json.gz) | [summary](requests-summary.json) | 5,354,054 / `df7b809b939f784dab836cc86d39bd17b8b27159afffbb5673453b411b710c17` | `27377144c6896420b9defa5095579aa5d982429d2ce5c3ad3a20fb895ac180fb` |
| pip | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/pip-report.json.gz) | [summary](pip-summary.json) | 288 / `fe4e5e910895258bd1fb25224c3b8738016bbbd4fa952f57e2fe9b41d2988016` | `ec88c52d789f1b4e8f2cc76cf5016cc284f38091de9d1108ae31b5edd6a94e0d` |
| Uvicorn | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/uvicorn-report.json.gz) | [summary](uvicorn-summary.json) | 11,541,305 / `4d420d7f95d8f77d2c1198ed3f8bc5c807e2aa7cbe877c253fb1d6aa19a46bda` | `ce6e95987090f602ee0ee7570502ad4c69a2477555d681730b485d42d2e92798` |
| Flask | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/flask-report.json.gz) | [summary](flask-summary.json) | 7,902,217 / `af21aa17889cff4e2cc85b8f7ef810ff26e95be75f4e14db8f0b9ad7a939c3b5` | `3d641d901e0b1da97a556a790f905c23e874f23ba2b1e353d66688f0a4216518` |
| Click | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/click-report.json.gz) | [summary](click-summary.json) | 322 / `4315d1b86553ea08aae47dd2c828fe9515b32694ff5cb7328bcad234c9b9037e` | `98658c91d154da01f3e308c1cc05946c159eeb1e25a874f8b212a8cbedca6130` |
| FastAPI | [byte-identical JSON.gz](../../2026-11-09-candidate-c0ea48/pilot/fastapi-report.json.gz) | [summary](fastapi-summary.json) | 320 / `6534c521b83c8b803e2020614fe9529978818443137d40ff1ca91a0d5fbb27c5` | `c49975fa8002754a5d5d6aab2a219c2f3ddb8a26d0321af6e10ccce409f20f76` |

The JSON and gzip SHA-256 values match the retained `c0ea48a` reports byte for
byte; the summaries capture the current executable's rerun. The changed
candidate behavior is not exercised by these project inputs. This replay
qualifies the reports for the current executable; it does not complete A05 or
replace the need for a justified, preregistered contract-preparation
amendment, viable project population, labels and exact-candidate rerun.

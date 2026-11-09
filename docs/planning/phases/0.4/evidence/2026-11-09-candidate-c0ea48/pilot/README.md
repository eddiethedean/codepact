# Frozen pilot replay on exact candidate `c0ea48a`

Status: **Replay reproduced; A05 remains pending.** These reports are the
exact-candidate rerun of the preregistered six-project pilot. Three checkouts
completed with only unknown-boundary diagnostics; three failed before
analysis. The eligible definite-finding population remains unknown, so no
labels, sample, actionability rate or A05 pass is claimed.

## Run identity and method

- Candidate source: `c0ea48a2fae771512e7c0010ba235ea9f7136e8d`.
- Executed artifact: macOS arm64 CLI wheel from build-only [Release run
  96](https://github.com/eddiethedean/codepact/actions/runs/42715489947),
  SHA-256 `426978c5b1a5a177cd917bcd4da5e143f3fb76233c4c7312ec91253daf5bb584`.
- `codepact --version`: `codepact 0.4.0`; Python target: 3.11.
- All six checkouts were verified clean at the preregistered commit and tree
  before replay. The same full-default-discovery command, rule set and policy
  from the [pilot preregistration](../../../pilot-inventory.md) were used.
- No project dependencies, source imports, hooks or tests were run.
- Full JSON stdout was retained losslessly as gzip. The JSON SHA-256 is over
  the decompressed bytes; gzip SHA-256 is over the committed evidence file.

## Results

| Project | Pinned commit | Status / eligible definite findings | Output detail | JSON bytes / SHA-256 | Gzip SHA-256 |
| --- | --- | --- | --- | --- | --- |
| Requests | `3c7baaa` | Completed, 0; 5,006 TP9001 unknown boundaries | [JSON.gz](requests-report.json.gz); 407,127 source bytes | 5,354,054 / `df7b809b939f784dab836cc86d39bd17b8b27159afffbb5673453b411b710c17` | `27377144c6896420b9defa5095579aa5d982429d2ce5c3ad3a20fb895ac180fb` |
| pip | `4b71f36` | Failed before analysis, unavailable | [JSON.gz](pip-report.json.gz); invalid module component in `build-project/build-project.py` | 288 / `fe4e5e910895258bd1fb25224c3b8738016bbbd4fa952f57e2fe9b41d2988016` | `ec88c52d789f1b4e8f2cc76cf5016cc284f38091de9d1108ae31b5edd6a94e0d` |
| Uvicorn | `a99c1b7` | Completed, 0; 10,782 TP9001 unknown boundaries | [JSON.gz](uvicorn-report.json.gz); 629,571 source bytes | 11,541,305 / `4d420d7f95d8f77d2c1198ed3f8bc5c807e2aa7cbe877c253fb1d6aa19a46bda` | `ce6e95987090f602ee0ee7570502ad4c69a2477555d681730b485d42d2e92798` |
| Flask | `49943b5` | Completed, 0; 7,400 TP9001 unknown boundaries | [JSON.gz](flask-report.json.gz); 591,255 source bytes | 7,902,217 / `af21aa17889cff4e2cc85b8f7ef810ff26e95be75f4e14db8f0b9ad7a939c3b5` | `3d641d901e0b1da97a556a790f905c23e874f23ba2b1e353d66688f0a4216518` |
| Click | `8abb77a` | Failed before analysis, unavailable | [JSON.gz](click-report.json.gz); total-source limit exceeded before `tests/test_utils/test_style.py` | 322 / `4315d1b86553ea08aae47dd2c828fe9515b32694ff5cb7328bcad234c9b9037e` | `98658c91d154da01f3e308c1cc05946c159eeb1e25a874f8b212a8cbedca6130` |
| FastAPI | `5349618` | Failed before analysis, unavailable | [JSON.gz](fastapi-report.json.gz); total-source limit exceeded before `fastapi/security/oauth2.py` | 320 / `6534c521b83c8b803e2020614fe9529978818443137d40ff1ca91a0d5fbb27c5` | `c49975fa8002754a5d5d6aab2a219c2f3ddb8a26d0321af6e10ccce409f20f76` |

The pip path-mapping failure and Click/FastAPI 1,000,000-byte source-budget
failures match the earlier candidate's observed failures. The completed
projects still have no eligible definite findings. As the preregistered
protocol requires, no files or projects were excluded to force a completed
scan, and no synthetic findings were counted. A05 remains pending.

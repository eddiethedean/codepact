# Exact-candidate pilot replay on `eb093c9`

Status: **A05 remains pending.** This replay is tied to CodePact source
candidate `eb093c9e7e5159953071ea486a0104ad77e0bbed` and the macOS arm64 CLI
wheel from [build-only Release run 42814622714](https://github.com/eddiethedean/codepact/actions/runs/42814622714).
The wheel SHA-256 is
`d4c3ad29992374626e2ea9a7022a99c89e9f51f0f1373f8c5b972d0d19c0d894`.
It reported `codepact 0.4.0` on macOS arm64 with CPython 3.11.15.

The six project checkouts were clean at their preregistered commits before and
after analysis. The analyzer read source as data; it did not install project
dependencies, import project code, or run project hooks or tests. All scans
used the frozen command, full default discovery, and rules:

```text
codepact check --root PROJECT --no-config --python-version 3.11 --mode advisory --fail-on error --rules TP1001,TP1002,TP1003,TP2001,TP3001 --output-format json
```

## Results

| Project | Pinned commit / tree | Result | Selected Python files / bytes | Diagnostics | Eligible definite findings |
| --- | --- | --- | ---: | --- | ---: |
| Requests | `3c7baaa` / `0c8b989` | Completed; coverage incomplete | 37 / 407,127 | 5,006 TP9001 unknown boundaries | 0 |
| pip | `4b71f36` / `9eb70d4` | Failed before analysis: `build-project/build-project.py` does not map to valid Python module components | 0 / 0 | Failure envelope | Unknown |
| Uvicorn | `a99c1b7` / `8fbdd8d` | Completed; coverage incomplete | 84 / 629,571 | 10,842 TP9001 unknown boundaries | 0 |
| Flask | `49943b5` / `6751ab7` | Completed; coverage incomplete | 83 / 591,255 | 7,400 TP9001 unknown boundaries | 0 |
| Click | `8abb77a` / `84808be` | Failed before analysis: full-discovery source budget of 1,000,000 bytes exceeded | 0 / 0 | Failure envelope | Unknown |
| FastAPI | `5349618` / `3cec183` | Failed before analysis: full-discovery source budget of 1,000,000 bytes exceeded | 0 / 0 | Failure envelope | Unknown |

The exact failure messages, revisions, trees, diagnostic totals, coverage,
byte counts, exit statuses and hashes are in the linked summaries. All six
reports validate against published checker schema v2 or failure schema v2.
The current Uvicorn replay reports 10,842 unknown-boundary diagnostics; the
earlier candidate reported 10,782, so the older report is not reused as current
evidence.

| Project | Report | Summary | JSON bytes / SHA-256 | Gzip SHA-256 |
| --- | --- | --- | --- | --- |
| Requests | [lossless report](requests-report.json.gz) | [summary](requests-summary.json) | 5,354,054 / `df7b809b939f784dab836cc86d39bd17b8b27159afffbb5673453b411b710c17` | `27377144c6896420b9defa5095579aa5d982429d2ce5c3ad3a20fb895ac180fb` |
| pip | [lossless report](pip-report.json.gz) | [summary](pip-summary.json) | 288 / `fe4e5e910895258bd1fb25224c3b8738016bbbd4fa952f57e2fe9b41d2988016` | `ec88c52d789f1b4e8f2cc76cf5016cc284f38091de9d1108ae31b5edd6a94e0d` |
| Uvicorn | [lossless report](uvicorn-report.json.gz) | [summary](uvicorn-summary.json) | 11,598,692 / `3071b307543552b4f4d8216533eb546a2e434c541f0fd329f176e1838be826ee` | `143abbe6586a065f2a3351d10667132fdd8f9e72262bb330dd41168bdde358bd` |
| Flask | [lossless report](flask-report.json.gz) | [summary](flask-summary.json) | 7,902,217 / `af21aa17889cff4e2cc85b8f7ef810ff26e95be75f4e14db8f0b9ad7a939c3b5` | `3d641d901e0b1da97a556a790f905c23e874f23ba2b1e353d66688f0a4216518` |
| Click | [lossless report](click-report.json.gz) | [summary](click-summary.json) | 322 / `4315d1b86553ea08aae47dd2c828fe9515b32694ff5cb7328bcad234c9b9037e` | `98658c91d154da01f3e308c1cc05946c159eeb1e25a874f8b212a8cbedca6130` |
| FastAPI | [lossless report](fastapi-report.json.gz) | [summary](fastapi-summary.json) | 320 / `6534c521b83c8b803e2020614fe9529978818443137d40ff1ca91a0d5fbb27c5` | `c49975fa8002754a5d5d6aab2a219c2f3ddb8a26d0321af6e10ccce409f20f76` |

Three projects completed, and none produced an eligible definite violation.
Three failed before analysis. The eligible population is unknown; no findings
were labeled, so the 30-finding and 90%-actionable thresholds cannot be
evaluated. This replay does not pass A05. See the [pilot protocol](../../../pilots.md)
and [release-readiness plan](../../../release-readiness.md) for the required
annotation and expansion process.

# CodePact documentation

CodePact finds conflicting resource declarations and supported lifecycle errors in Python without importing or running your application. It reports each finding with source locations, certainty, and coverage information.

**CodePact 0.3.0 is published.** It provides contract inspection, bounded lifecycle checking, and cleanup obligations across supported exits. The previous 0.1.0 and 0.2.0 releases used the TypePact package names. APIs, diagnostics, and JSON schema remain provisional during 0.x.

```sh
git clone https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run codepact inspect-contracts connection.py
uv run codepact check --root src
```

Create `connection.py` with the [quick start](quickstart.md), then read the result using the [JSON reference](output.md).

Follow the [lifecycle tutorial](lifecycle.md) to use `check`. See the
[availability guide](availability.md) for exact release and platform support.

## Find your next step

| I want to… | Start here |
| --- | --- |
| Declare a resource and inspect it | [Quick start](quickstart.md) |
| Find a supported use after close | [Lifecycle checking](lifecycle.md) |
| Decide whether my project fits the supported subset | [Adopting CodePact](adopting.md) |
| Resolve imports across my own modules | [Multi-file tutorial](project-guide.md) |
| Use the inspector in a build pipeline | [Automation and CI](automation.md) and [report handling](ci-reporting.md) |
| Understand an error or incomplete result | [Troubleshooting](troubleshooting.md) |
| Look up a diagnostic or example | [Rule catalog](rules.md) and [examples](examples.md) |
| Embed the analyzer in a Rust application | [Rust API](rust-api.md) |

```{note}
Python helpers run on Python 3.11+. The analyzer accepts source targets 3.11–3.13. Every emitted contract is `declared-unverified`; a successful inspection describes declarations within the supported subset.
```

```{toctree}
:maxdepth: 1
:caption: Get started

install
quickstart
lifecycle
availability
adopting
upgrading
contracts
project-guide
automation
ci-reporting
examples
faq
```

```{toctree}
:maxdepth: 1
:caption: Analyzer reference

cli
output
supported-syntax
limitations
troubleshooting
inspector
rules
security
performance
```

```{toctree}
:maxdepth: 1
:caption: API and project

python-api
rust-api
architecture
roadmap
release-notes
support
development
```

## Project links

- [Source code](https://github.com/eddiethedean/codepact)
- [Issues and feature requests](https://github.com/eddiethedean/codepact/issues)
- [PyPI package](https://pypi.org/project/codepact/)
- [Native CLI package](https://pypi.org/project/codepact-cli/)
- [MIT license](https://github.com/eddiethedean/codepact/blob/main/LICENSE)
- [Security reporting instructions](https://github.com/eddiethedean/codepact/blob/main/SECURITY.md)

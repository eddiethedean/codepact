# TypePact documentation

**Python resource contracts, inspected by Rust.** TypePact records declared resource lifecycle and access intent from Python source and reports normalized contract facts, source locations, provenance, and diagnostics.

Version **0.1.0** provides contract inspection. The analyzer reads source as text and does not import or execute the inspected project. It does not yet check resource use through control flow or enforce ownership at runtime.

```sh
uv add typepact==0.1.0
uv run typepact inspect-contracts connection.py
```

Create `connection.py` with the [quick start](quickstart.md), then read the result using the [JSON reference](output.md).

## Find your next step

| I want to… | Start here |
| --- | --- |
| Declare a resource and inspect it | [Quick start](quickstart.md) |
| Resolve imports across my own modules | [Multi-file tutorial](project-guide.md) |
| Use the inspector in a build pipeline | [Automation and CI](automation.md) |
| Understand an error or incomplete result | [Troubleshooting](troubleshooting.md) |
| Embed the analyzer in a Rust application | [Rust API](rust-api.md) |

```{note}
Python helpers run on Python 3.11+. The analyzer accepts source targets 3.11–3.13. Every emitted contract is `declared-unverified`; a successful inspection describes declarations within the supported subset.
```

```{toctree}
:maxdepth: 1
:caption: Get started

install
quickstart
contracts
project-guide
automation
```

```{toctree}
:maxdepth: 1
:caption: Inspector reference

cli
output
supported-syntax
limitations
troubleshooting
inspector
```

```{toctree}
:maxdepth: 1
:caption: API and project

python-api
rust-api
architecture
roadmap
release-notes
development
```

## Project links

- [Source code](https://github.com/eddiethedean/typepact)
- [Issues and feature requests](https://github.com/eddiethedean/typepact/issues)
- [PyPI package](https://pypi.org/project/typepact/)
- [Native CLI package](https://pypi.org/project/typepact-cli/)
- [MIT license](https://github.com/eddiethedean/typepact/blob/main/LICENSE)

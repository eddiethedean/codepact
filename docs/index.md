# TypePact documentation

**Python resource contracts, inspected by Rust.** TypePact records declared resource lifecycle and access intent from Python source and reports normalized contract facts, source locations, provenance, and diagnostics.

Version **0.1.0** provides contract inspection. The analyzer reads source as text and does not import or execute the inspected project. It does not yet check resource use through control flow or enforce ownership at runtime.

```{toctree}
:maxdepth: 2
:caption: Get started

install
quickstart
contracts
```

```{toctree}
:maxdepth: 2
:caption: Inspector reference

cli
output
supported-syntax
limitations
inspector
```

```{toctree}
:maxdepth: 2
:caption: API and project

python-api
rust-api
architecture
roadmap
development
```

## Project links

- [Source code](https://github.com/eddiethedean/typepact)
- [Issues and feature requests](https://github.com/eddiethedean/typepact/issues)
- [PyPI package](https://pypi.org/project/typepact/)
- [Native CLI package](https://pypi.org/project/typepact-cli/)
- [MIT license](https://github.com/eddiethedean/typepact/blob/main/LICENSE)

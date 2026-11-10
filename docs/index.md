# CodePact documentation

**Find lifecycle mistakes in Python before running your application.**

CodePact reads source and checks declared resource contracts for supported
use after close, outstanding cleanup duties, and invalid ownership handoffs.
Reports connect each finding to source locations and explain its certainty
and the parts of the program that remain outside the supported analysis.

```{important}
**Choose your version.** CodePact 0.3.0 is the published release. This site’s
0.4.0 candidate documentation also describes project configuration, baselines,
line suppressions, and must-use results available from the source checkout.
The candidate is not published or pilot-qualified. See [release availability](availability.md).
```

## Start with a working example

Install the published release in your uv project:

```sh
uv add codepact==0.3.0
uv run --locked codepact --version
```

The [quick start](quickstart.md) provides a complete source file and a first
successful check. Continue with [lifecycle checking](lifecycle.md) to reproduce
a use after close, understand the related close location, and repair it.

## Find your next step

| Goal | Guide |
| --- | --- |
| Install the helpers, standalone CLI, or Rust library | [Installation](install.md) |
| Declare owned or borrowed access | [Contract declarations](contracts.md) |
| Check imports across modules | [Multi-file tutorial](project-guide.md) |
| Evaluate the 0.4 candidate in a project | [Adoption workflow](adopting.md) |
| Run checks and preserve reports in CI | [Automation](automation.md) and [report handling](ci-reporting.md) |
| Understand a finding or incomplete coverage | [Rules](rules.md), [limitations](limitations.md), and [troubleshooting](troubleshooting.md) |
| Integrate the analyzer with another tool | [Rust API](rust-api.md) and [report schemas](schemas.md) |
| Run analyzer development controls | [Synthetic project controls](synthetic-projects.md) |
| Build or publish this documentation | [Documentation deployment](documentation.md) |

## Know what a result establishes

The Python helpers run on Python 3.11+; the analyzer accepts source targets
3.11–3.13. Declarations carry `declared-unverified` provenance. CodePact does
not execute the project or verify that a close method releases a real resource.
Read certainty and coverage alongside findings: an advisory check can pass
with unsupported regions. The [output reference](output.md) explains how to
interpret reports; [supported syntax](supported-syntax.md) defines the scope.

```{toctree}
:maxdepth: 1
:caption: Get started

install
quickstart
lifecycle
contracts
project-guide
examples
```

```{toctree}
:maxdepth: 1
:caption: Adopt and automate

adopting
automation
ci-reporting
upgrading
```

```{toctree}
:maxdepth: 1
:caption: Reference

cli
rules
output
schemas
python-api
rust-api
supported-syntax
inspector
limitations
performance
troubleshooting
faq
```

```{toctree}
:maxdepth: 1
:caption: Project and maintenance

availability
architecture
roadmap
release-notes
security
support
development
documentation
synthetic-projects
```

## Project links

[Source](https://github.com/eddiethedean/codepact) ·
[Issues](https://github.com/eddiethedean/codepact/issues) ·
[Python package](https://pypi.org/project/codepact/) ·
[Standalone CLI](https://pypi.org/project/codepact-cli/) ·
[MIT license](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

The historical 0.1 and 0.2 releases used the TypePact name. The documentation
continues to use `typepact.readthedocs.io`; the current project is CodePact.

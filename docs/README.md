# CodePact documentation

[Browse the hosted documentation](https://codepact.readthedocs.io/en/latest/index.html)

Start with the [quick start](quickstart.md) to run a check, read a finding,
and repair a resource lifecycle bug. [Installation](install.md) covers the
0.5.0 packages and source builds.

| I want to | Read |
| --- | --- |
| Learn the declaration syntax | [Contracts](contracts.md) and [Python API](python-api.rst) |
| Understand a finding | [Lifecycle tutorial](lifecycle.md), [rule catalog](rules.md), and [report fields](output.md) |
| Analyze a project with local imports | [Multi-file tutorial](project-guide.md) |
| Add CodePact to CI | [Adoption](adopting.md), [automation](automation.md), and [CI report handling](ci-reporting.md) |
| Try explicit moves or TaskGroup checks | [Experimental ownership](ownership.md) |
| Integrate the analyzer | [CLI](cli.md), [Rust API](rust-api.md), and [JSON schemas](schemas.md) |
| Review analysis boundaries | [Supported syntax](supported-syntax.md), [limitations](limitations.md), and [troubleshooting](troubleshooting.md) |
| Check release and platform support | [Availability](availability.md), [release notes](release-notes.md), and [roadmap](roadmap.md) |

The formatted site uses Sphinx. From a checkout, run `make check-docs` and
open `target/docs/html/index.html`. [Build and preview instructions](documentation.md)

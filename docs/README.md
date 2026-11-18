# CodePact documentation

[Browse the hosted documentation](https://codepact.readthedocs.io/en/latest/index.html)

Start with the [quick start](quickstart.md) to run a check, read a finding,
and repair a resource lifecycle bug. [Installation](install.md) covers the
0.9.0 packages and source builds.

| I want to | Read |
| --- | --- |
| Learn the declaration syntax | [Contracts](contracts.md) and [Python API](python-api.rst) |
| Check if the project fits my code | [Compatibility and fit](compatibility.md) |
| Understand a finding | [Lifecycle tutorial](lifecycle.md), [rule catalog](rules.md), and [report fields](output.md) |
| Analyze a project with local imports | [Multi-file tutorial](project-guide.md) |
| Try a real project and add checks to CI | [Pilot](pilot.md), [adoption](adopting.md), and [automation](automation.md) |
| Compare source revisions or wheel publications | [Change review and wheel comparison](cli.md#compare-captured-source-analyses) |
| Try explicit moves or TaskGroup checks | [Experimental ownership](ownership.md) |
| Publish wheel contracts or configure reviewed models | [Reviewed contracts and runtime models](reviewed-contracts.md) |
| Integrate the analyzer | [Choose an integration](integration.md) and [configuration reference](configuration.md) |
| Review analysis boundaries | [Supported syntax](supported-syntax.md), [limitations](limitations.md), and [troubleshooting](troubleshooting.md) |
| Evaluate for a team | [Evaluation guide](evaluation.md), [release verification](release-verification.md), and [release notes](release-notes.md) |

The formatted site uses Sphinx. From a checkout, run `make check-docs` and
open `target/docs/html/index.html`. [Build and preview instructions](documentation.md)

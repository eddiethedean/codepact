# CodePact

:::::{container} cp-hero

::::{container} cp-hero-copy

:::{container} cp-kicker
Static analysis for Python
:::

:::{container} cp-headline
Catch resource bugs before runtime.
:::

CodePact checks your Python resource contracts. Find supported use-after-close,
missed cleanup, and discarded results directly in source.

:::{container} cp-actions
[Run your first check](quickstart.md)
[Installation options](install.md)
:::

::::

::::{container} cp-terminal

```{raw} html
<div class="cp-terminal-bar">
  <span class="cp-terminal-dots" aria-hidden="true"><i></i><i></i><i></i></span>
  <span>connection.py</span>
</div>
<div class="cp-terminal-body">
  <p class="cp-terminal-command"><span aria-hidden="true">$ </span>codepact check --root demo</p>
  <div class="cp-terminal-diagnostic">
    <p class="cp-terminal-location">connection.py:23:1</p>
    <p><strong>TP1001</strong> Borrowed call requires an open resource</p>
  </div>
  <dl>
    <dt>Certainty</dt><dd>definite</dd>
    <dt>Related close</dt><dd>connection.py:22:1</dd>
  </dl>
  <p class="cp-terminal-status">Exit status <b>1</b></p>
</div>
```

::::

:::::

::::{container} cp-release-strip

:::{container} cp-release cp-release-published
**0.4.0 · Published preview**

[Install from PyPI or crates.io](availability.md)
:::

:::{container} cp-release cp-release-candidate
**0.5.0 · Development candidate**

[Try opt-in ownership checking](ownership.md)
:::

::::

## Find your guide

::::{container} cp-guide-grid

:::{container} cp-guide
[Declare contracts](contracts.md)

Mark a resource’s close method, owned or borrowed access, and results that require handling.
:::

:::{container} cp-guide
[Trace a finding](lifecycle.md)

Follow the source locations for a use-after-close finding, then repair it and recheck.
:::

:::{container} cp-guide
[Adopt in a project](adopting.md)

Set a project policy, review existing findings, and record intentional exceptions.
:::

:::{container} cp-guide
[Run in CI](automation.md)

Choose an exit policy and preserve structured reports for later review.
:::

:::{container} cp-guide
[Look up a rule](rules.md)

Find diagnostic codes, their meanings, and the first repair to consider.
:::

:::{container} cp-guide
[Integrate the checker](rust-api.md)

Embed checks in Rust or consume the versioned [JSON schemas](schemas.md).
:::

::::

## Read coverage alongside findings

CodePact reads source without executing your application. It follows supported
calls, aliases, branches, and exits. Declarations specify lifecycle behavior,
while application code is responsible for actual runtime cleanup. An advisory
check can pass with incomplete coverage. [Report fields and certainty](output.md) ·
[Supported syntax](supported-syntax.md)

The Python helpers run on Python 3.11 and later. The checker accepts Python
3.11–3.13 source targets. [Platform and release evidence](availability.md)

```{toctree}
:hidden:
:maxdepth: 2
:caption: Get started

quickstart
install
lifecycle
contracts
project-guide
examples
bugs-caught
```

```{toctree}
:hidden:
:maxdepth: 2
:caption: Adopt and automate

adopting
automation
ci-reporting
upgrading
ownership
```

```{toctree}
:hidden:
:maxdepth: 2
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
:hidden:
:maxdepth: 2
:caption: Project

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

[Source on GitHub](https://github.com/eddiethedean/codepact) ·
[Report an issue](https://github.com/eddiethedean/codepact/issues) ·
[MIT license](https://github.com/eddiethedean/codepact/blob/main/LICENSE)

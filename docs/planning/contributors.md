# Contributor and governance plan

## Onboarding

Provide a runnable development setup when manifests land, with pinned tools, documented native build prerequisites and one short command for the focused suite. First contributions should include documentation fixes, diagnostic explanations, fixture cases and external resource models with evidence.

Label issues by component and milestone. Include minimal reproductions and expected semantics. Assign an accountable maintainer to each milestone before scheduling it.

## Review requirements

A behavior change needs its canonical semantic rule, both API implications, positive and negative fixtures, unknown-boundary behavior and migration impact. Reviewers should challenge assumptions about aliases, exceptions and dynamic dispatch.

Public APIs, diagnostic codes, configuration keys and semantic changes require an RFC or decision record. Record alternatives, compatibility evidence, risks and acceptance criteria. Ordinary refactors need a description and relevant checks.

Require a maintainer review for release-bound changes. Broader ownership claims should receive an additional independent semantic review when contributor capacity permits. Small teams should explicitly record deferred review rather than silently claiming it occurred.

## Contribution workflow

Open an issue for substantive design changes, agree on scope, submit a focused branch/PR, run available checks, and update relevant documentation. Keep generated artifacts reproducible. A contribution must not add dependencies or copied code without provenance and license review.

## Governance

Initially, repository maintainers decide scope and releases, recording disputed semantic decisions publicly. Establish named module owners as the project grows. Adopt a code of conduct and private vulnerability reporting channel before building a contributor community; do not invent an email address or claim a channel exists before configuration.

## Maintenance expectations

Every shipped external model needs a responsible owner, upstream version bounds and fixtures. Every public feature needs a reference page, stability status and deprecation policy. Keep open research separate from supported guarantees.

## Analyzer change review

Use the [migration plan](analyzer-migration.md) as the architecture baseline. A query change must identify its tracked inputs, invalidation boundary, cycle/budget behavior and revision lifetime, with fresh/updated-session parity evidence. A binding change must account for previously captured aliases and all supported rebinding forms. Review Ruff/Salsa API upgrades behind the adapter and include MSRV, licensing and registry-build evidence; do not leak upstream or database IDs into public schemas.

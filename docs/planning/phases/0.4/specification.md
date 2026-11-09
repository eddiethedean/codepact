# 0.4 adoption specification

Status: **AD01 accepted all proposed defaults on 2026-11-09; technical qualification remains pending.** The [AD01 decision record](decision-record.md) records the choices, alternatives and reconsideration triggers. This specification defines the accepted contract for [AD02–AD08](../0.4-adoption.md); the [V04 matrix](validation.md) defines required evidence. Code and focused tests do not qualify a phase gate or authorize a release.

## Released starting point and compatibility

CodePact 0.3 has `check` and `inspect-contracts`, source targets 3.11/3.12/3.13, inspection schema 0, checker schema 1, bounded cleanup/cancellation analysis and lightweight resource/access helpers. It has no pyproject adoption settings, baselines, line suppressions or must-use helpers. Keep the source target range until separate parser/model evidence qualifies an extension; helper-runtime support is a separate matrix.

Extend `check`; do not introduce a duplicate lint command. Keep `inspect-contracts`' explicit-file behavior, schema 0 and existing exit policy. The new project/policy resolver applies to `check` and baseline operations; do not silently apply rule filters or baseline matching to inspection. AD01 accepts preservation of existing Rust one-shot/session APIs with no API amendment; new policy/report APIs wrap canonical results rather than changing the meaning of `check`/`check_report` behind existing callers.

AD01 accepts checker schema **2** for the expanded 0.4 `check --json` result. Publish both schemas and a field/consumer migration guide; retain inspection schema 0 independently. No runtime schema-1 output adapter is included. Preserve existing diagnostic codes, completion, certainty, trust and model metadata; no field is repurposed. New consumers reject an unsupported major schema version clearly. Baseline, capability and explanation documents have separate version fields. Package version and schema version are not interchangeable.

## Accepted command surface

```text
codepact check [--root DIR] [--config PYPROJECT | --no-config]
               [--python-version VERSION] [--mode advisory|strict]
               [--fail-on error|warning] [--output-format text|json|concise]
               [--json] [--baseline FILE] [--max-diagnostics N] [FILE...]
codepact check [same project/policy options] --show-settings --json
codepact baseline update [same project/policy options]
                        --output FILE --reason TEXT [--replace] [--json] [FILE...]
codepact capabilities --json
codepact explain RULE --json
```

`--json` aliases `--output-format json`; conflicting format selectors fail with 2. `--show-settings` resolves and validates settings without analyzing source, and reports each value's origin. It does not imply a clean check. Baseline updates always use full canonical results and accept text or JSON summary output; detail limits and `--show-settings` are not baseline-update options. `capabilities` and `explain` are read-only static metadata operations; rule explanations need no project execution or network access. Unknown options/commands/rules are invocation errors.

## Project configuration and discovery

Example 0.4 configuration, not supported by 0.3:

```toml
[tool.codepact]
config-version = 1
root = "src"
python-version = "3.12"
include = ["**/*.py"]
exclude = ["generated/**"]
enabled-rules = ["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"]
mode = "advisory"
fail-on = "error"
baseline = "codepact-baseline.json"

[tool.codepact.severity]
TP3001 = "warning"
```

The baseline path above is relative to the configuration directory, not `src`; do not use the example until that file has been explicitly created. The complete initial key set is shown above. Unknown keys, duplicate TOML keys, wrong types, invalid patterns, unsupported config versions, unsupported Python targets, unknown rule codes and invalid severities fail with 2 before checking. Validate `[tool.codepact]` only; unrelated project/tool tables remain legal. `config-version` is required when this table is present. No environment variables, project plugins, Python expressions, hooks, remote includes or per-file overrides affect settings in 0.4.

Resolution is deterministic:

1. `--config` selects exactly one pyproject file and conflicts with `--no-config`. Otherwise search upward from the explicit CLI root, or from the invocation directory when no root is given. Stop at the nearest `pyproject.toml` even if it lacks a CodePact table; never merge ancestor or nested configurations. `--no-config` skips searching.
2. CLI scalar options override file values, which override built-in defaults. CLI `--root` is resolved against the invocation directory. A configured root is resolved against the config directory. With a selected CodePact table but no root setting, default to the config directory; with no table/config, retain the invocation directory. The config file and effective root are reported separately.
3. Resolve configured paths, including `baseline`, against the config directory. Resolve CLI file/baseline/output paths against the invocation directory. Include/exclude patterns always match normalized root-relative paths. Arrays replace defaults; they are never implicitly appended. CLI file operands do not replace discovery.
4. Discover sorted `.py` files under the effective root as in 0.3. Default includes are `**/*.py`, including root-level files. Define `/` as the pattern separator, `*`/`?` within a component and `**` across zero or more components; no negation, brace expansion or absolute/parent-traversal patterns. Excludes win over includes. Retain 0.3's built-in directory skips and skip discovered symlinks. No implicit gitignore policy is added.
5. Explicit files remain within the canonical root and obey includes/excludes and built-in skips; an explicitly excluded/skipped input is an actionable input error, not a bypass. Deduplicate physical files; reject module-name collisions and an empty selected source set. Do not follow symlink components in explicit source paths. Never discover installed packages or execute target files. Imported-but-excluded/unavailable source remains an unresolved boundary when relevant.

Built-in defaults preserve 0.3 root/discovery behavior, Python 3.11, advisory mode and fail-on error. Stable released violation rules plus TP3001 are selected by default. Experimental rules require explicit selection. The registry lists exact codes and semantic revisions; no prefix wildcards. TP4xxx declaration/syntax diagnostics and TP9xxx coverage/execution diagnostics are mandatory and cannot be disabled or assigned lower severities. Severity overrides on selected violation rules allow `error`, `warning` or `info`, not `off`; selection is explicit in `enabled-rules`.

Report selected/disabled rules, include/exclude settings, selected files and unavailable dependencies. Rule disabling or file exclusion changes analyzed scope. It cannot change the underlying contract into a satisfied obligation or establish complete coverage for excluded code. Module identities remain root-relative under the 0.3 naming rules; changing root may change identity and invalidate baselines.

## Policy and exit calculation

First compute canonical findings, certainty, trust and coverage for the selected analysis scope. Then apply inline suppression, baseline matching, effective severity and threshold to derive presentation/actionable sets. Retain raw values and disposition separately. Disabling a violation rule must not remove semantic facts needed by another selected rule. Mode, severity and suppression never promote possible/unknown facts to definite facts.

AD01 accepts TP3001 default severity `warning`; definite lifecycle/declaration errors retain 0.3 severities. An eligible violation is actionable if unsuppressed and at or above `fail-on` (`error` or `warning`). Strict mode additionally fails for every unsuppressed possible violation, regardless of configured severity, and any incomplete analysis coverage. Definite below-threshold findings remain below threshold in either mode. Inline/baseline suppression may acknowledge eligible possible violations, but cannot remove incomplete coverage. Mandatory syntax/declaration errors always fail. A successful check means completed analysis under reported scope and policy, not general safety.

| Full analysis outcome | Advisory | Strict |
| --- | ---: | ---: |
| Complete; no actionable findings | 0 | 0 |
| Unsuppressed finding at/above severity threshold, or mandatory source error | 1 | 1 |
| Only unsuppressed possible violation below threshold | 0 | 1 |
| Only unsupported boundary or recoverable analysis-budget exhaustion | 0 | 1 |
| Only baseline/inline-suppressed eligible findings; complete coverage | 0 | 0 |
| Suppressed boundary display, but coverage remains incomplete | 0 | 1 |
| Invalid invocation/configuration/baseline/target, unreadable input, fatal input limit, internal failure or analyzer request cancellation | 2 | 2 |

The suppressed-boundary row is a presentation test; TP9xxx itself is not eligible for inline/baseline suppression. Caller display filtering must still preserve coverage. Fatal whole-request source/file limits use 2; recoverable flow-budget exhaustion produces a completed, incomplete report and the policy above. Cancellation of the analyzed Python program remains a modeled completion kind, distinct from cancellation of the analyzer request.

For machine output, completed analysis and execution failure have explicit status values; a failure envelope contains structured error category/message, available identities and no claim of completed analysis. Never emit a partial completed envelope after failure. Operational messages go to stderr; stdout holds exactly one JSON document in JSON mode. A broken output stream or serialization failure returns 2 even if no complete JSON error can be written. Wrappers retain the report on exit 1 and distinguish it from exit 2.

## Suppressions

AD01 accepts one line form only:

```python
fetch()  # codepact: ignore[TP3001] -- intentionally queried for its side effect
```

Require an explicit nonempty list of exact eligible TP1xxx/TP2xxx/TP3xxx violation codes and a nonempty reason. No blanket ignore, file-level ignore, next-line scope or implicit `noqa` alias. A trailing suppression targets findings whose primary span starts on that physical line, including a multiline call's starting line. It never suppresses a related-location finding on another line.

Malformed directives and unknown/noneligible codes emit an unsuppressible declaration error (TP4002). A well-formed unused suppression is reported as policy metadata for review without masking findings. Preserve suppressed findings and reasons in JSON audit data; human output includes suppression counts. Suppressions do not discharge resource duties, consume returned values, alter summaries or remove coverage reasons. Changes to comments update policy/snapshot data even when semantic facts can be reused.

## Baseline format and identity

A baseline is an explicitly generated local JSON document with `baseline_schema_version`, engine/rule semantic revisions, source target/model identity, root/module-mapping policy, entries and a creation/update reason. Each entry records exact rule, fingerprint algorithm version, logical path, semantic anchor/evidence hash, certainty, trust, completion and an explanation for audit. Validate the document before analysis; malformed, unreadable or unsupported-version files fail with 2. No automatic migration or silent baseline fallback.

The accepted fingerprint hashes canonical structured evidence: rule code and semantic revision, root-relative logical path/module, enclosing declaration identity, normalized diagnostic-site AST, relevant resolved contract identity, completion and related semantic evidence. Exclude prose, absolute checkout paths, byte offsets and ordinary whitespace/comments. Include changes to result-use/cleanup effects, certainty and trust. Full-file source hashes identify snapshots, not baseline finding identity. Do not persist Salsa allocation IDs.

Match an entry to at most one current finding, and a finding to at most one entry. If repeated identical sites, overload-like declarations or changed surrounding structure make correspondence ambiguous, leave all affected findings unsuppressed and report ambiguity. Do not use a line-number fallback or source-order ordinal that could hide a newly inserted identical violation. AD01 freezes this canonical matching contract; V04-11 requires insertion and collision counterexamples before qualification.

A changed line number or unrelated comment should preserve an unambiguous match; changed rule/evidence/certainty/trust, file rename, declaration identity or module mapping must surface the finding. Target/model/semantic-rule incompatibility leaves affected entries unmatched with a visible warning, not blanket suppression. A new rule has no entries and is visible. Mode/severity/output-format changes do not change finding identity. Report unmatched/stale entries and matched/suppressed counts; stale entries never cause new findings to disappear.

`baseline update --output FILE --reason TEXT` is the only writer. Refuse an existing destination without `--replace`, and refuse directory/symlink destinations. The update operation does not apply or require the configured baseline; validate/read an existing destination only for the update diff. This allows initial creation before the configured baseline exists. Collect from full current selected-rule results before baseline matching, excluding inline-suppressed findings. Omit ambiguous identities with explicit counts/reasons rather than writing duplicate entries that cannot safely match; duplicate entries in a loaded baseline are invalid input.

Retain unrelated config/source exactly; write the complete deterministic set of unambiguous eligible entries through an atomic replacement only after completed analysis and successful validation. Recoverable incomplete coverage is recorded and shown prominently; no entry may suppress its reasons. Failure/cancellation/truncation cannot write a partial baseline. Report added/removed/changed/skipped counts, source snapshot and policy identity. A normal check never creates, modifies or refreshes a baseline.

## Initial must-use contract

AD01 accepts these public spellings:

```python
from typing import Annotated
from codepact import discard, must_use
from codepact.contracts import MustUseResult
from codepact.rust import MustUse

@must_use
def fetch() -> int:
    return 1

def fetch_explicit() -> MustUse[int]:
    return 1

def fetch_verbose() -> Annotated[int, MustUseResult()]:
    return 1

fetch()                 # TP3001: discarded must-use result
value = fetch()         # Immediate handling; eventual use is not established.
_ = fetch_explicit()    # Assignment has the same immediate-handling rule.
discard(fetch())        # Explicit intentional discard; no cleanup effect.
```

`@must_use` preserves callable identity/signature through typed ParamSpec-compatible declarations, attaches metadata without a wrapper and does not import the analyzer. `MustUse[T]` is an Annotated-compatible alias of `T`; the metadata marker is distinct from the alias, as with existing access metadata. `discard(value) -> None` evaluates its argument normally, accepts any value and has no analyzer/runtime ownership, close or exception-suppression behavior. Importing helpers remains lightweight.

Only resolved synchronous function/method return contracts participate initially. A decorator, shorthand return annotation and verbose metadata lower to one canonical result-use bit; duplicate equivalent declarations do not double-report. Resolve imports, supported aliases/re-exports, captured bindings and shadowing by identity as in 0.3. A user-defined `must_use`, `MustUse` or `discard` has no special semantics. Parameters, locals or class-wide MustUse declarations are not supported return contracts; reject misplaced known metadata with TP4002. Unknown decorators, unresolved call targets and unsupported annotation shapes remain boundaries when they prevent determining a relevant obligation.

| Source action on a known marked synchronous call | Initial rule |
| --- | --- |
| Bare expression statement, with optional parentheses | One TP3001 at the call; declaration as related evidence |
| Assignment, including `_` and supported destructuring | Immediate obligation satisfied; existing unsupported destructuring/effects remain boundaries |
| Return, yield, argument or other containing expression | Counts as immediate handling only where that expression is supported; no automatic must-use propagation to the enclosing function |
| Resolved `discard(call())` | Explicit handling; preserve acquisition, mutation, raise and cleanup effects of argument evaluation |
| Known marked call inside an unsupported discarded container/comprehension/expression | Visible boundary for unqualified handling; do not claim eventual use or fabricate a definite bare-discard violation |
| Assigned result later abandoned/overwritten | No new must-use dataflow diagnostic in 0.4; resource cleanup duties still apply independently |
| Async call/await, generator consumption or dynamically selected callable | Result-use consumption unqualified; retain a boundary when a marked contract is involved; preserve 0.3 async/cancellation analysis |

A marked `None` return still expresses the declared result-use contract; ordinary type checking remains external. Pure result-use errors do not establish resource ownership or runtime safety. Discarding a must-use owning factory can emit TP3001 and the existing cleanup finding independently; `discard(factory())` removes only TP3001 and cannot discharge its cleanup duty. Passing/returning a result does not strengthen unknown cleanup/escape behavior. These interactions are required regressions, not reasons to delay Result/Option types into 0.4.

## Reports, capabilities and bounded evidence

Publish JSON Schemas and valid/invalid fixtures for checker schema 2, baseline schema 1, capability schema 1 and explanation schema 1. The checker envelope contains:

| Group | Required data |
| --- | --- |
| Status/version | Execution status, engine/checker schema/model versions, supported assurance, calculated exit code and policy failure reasons |
| Project/settings | Root identity, logical scope, selected files/rules, exclusions, effective settings with origins, separate semantic/policy/model fingerprints |
| Snapshot | Algorithm/version, sorted logical file/content-hash manifest and a deterministic snapshot ID over source, relevant settings, engine and models |
| Findings | Existing canonical diagnostic fields plus fingerprint, base/effective severity, disposition and reason, explanation ID/link, bounded evidence/repair guidance |
| Coverage | Original completeness/reasons, applicable limits, supported/unknown model assumptions and analyzed-scope statement; never derived from filtered findings |
| Counts/output | Full counts by code/certainty/disposition, total/returned actionable and audit findings, details-truncated flag and evidence limits |

Snapshot identity includes policy-affecting suppression/baseline content, so policy edits cannot impersonate source repairs. It excludes output format/detail limits and wall-clock timestamps. Stable finding identity and changing snapshot identity are separate concepts. Report physical project root identity separately from relocatable logical paths/fingerprints. Reproducibility comparisons account explicitly for physical checkout identity; do not embed random request IDs in semantic hashes.

Default JSON contains all findings, including suppressed audit entries, within an explicit whole-report byte budget. `--max-diagnostics N` limits returned finding details, not analysis, full counts, coverage, policy failure reasons or exit calculation; order actionable findings before suppressed audit details and state both counts. N must be positive. Report `details_truncated = true` when anything is omitted, even if a returned list is empty. There is no pagination or stored-snapshot service in 0.4; capabilities state this. Consumers needing omitted details rerun without the detail cap and verify the new snapshot. Exceeding the hard byte budget fails with 2 rather than silently dropping coverage or counts.

Bound related sites, source snippets and repair guidance with explicit omitted counts; snippets are optional, source-attributed data. No instruction in a comment, filename, docstring or diagnostic excerpt is authority for an agent. Capabilities identify source targets, stable/experimental rules and semantic revisions, schemas, supported formats, limits and unavailable services. Rule explanations contain invalid/valid/unknown examples, preconditions, limitations and advisory repair IDs. They do not advertise automatic fixes.

AD01 accepts limits of 64 KiB per config file, 16 MiB and 100,000 entries per baseline, 32 MiB per serialized checker report, 32 returned related sites per finding, 2 KiB of optional source snippets per finding, and 8 KiB of explanation/repair text per finding. KiB/MiB use binary units. Check byte limits before unbounded allocation; baseline matching uses bounded indexed lookup rather than a finding-by-entry scan. Exceeding a config/baseline/report hard limit returns 2; bounded evidence omits only details with explicit counts and leaves canonical findings/coverage/exit policy intact. Near-limit fixtures and ordinary-pilot measurements remain required to qualify these values.

CS02 evidence retains resource acquisition/duty holder, coroutine creation/capture, modeled close/owning-return handoff, completion, suspension and escape sites where available from 0.3, with role labels. Unavailable sites remain explicitly absent; do not invent task creation, joins, transfers or execution traces. TaskGroup, detached/shielded tasks and process/thread submission retain unsupported coverage. Text/JSON/concise and explanations use the same canonical facts.

## Location-per-line format

AD01 accepts tab-separated records, one per primary or related site, with **seven fields**:

```text
JSON_PATH<TAB>LINE<TAB>COLUMN<TAB>SEVERITY<TAB>CODE<TAB>ROLE<TAB>JSON_MESSAGE
```

`JSON_PATH` and `JSON_MESSAGE` are single JSON string literals, escaping tabs, newlines, quotes and control characters. Spaces/colons are ordinary path characters. Integers are one-based lines/UTF-8 byte columns; ends are exclusive in JSON. ROLE is `primary` or `related`; related records inherit their parent code/severity and immediately follow it with a role-specific message. Only actionable/displayed findings produce records; coverage and full counts remain visible in a stderr summary. Failed/incomplete execution must still be distinguishable by exit/status. The JSON report is the richer audit contract.

Sort findings by normalized logical path, primary start/end, rule, completion and fingerprint; sort related locations by role/path/span. Use the same order in text and JSON, except the documented actionable-first detail capping. Disable color/control output in concise/JSON. Qualify decoding with Unicode, emoji, CRLF, multiline spans and adversarial path characters, plus one reproducible external-tool adapter. This is LE01 CLI delivery; it does not claim LSP or editor-client qualification.

## Decision checkpoint AD01

All defaults below were accepted by the maintainer on 2026-11-09; see the [AD01 decision record](decision-record.md) for exact choices, alternatives and reconsideration triggers. The local candidate follows them. Qualification fixtures and measurements remain required. No independent reviewer is required.

| Decision | Accepted default | Required counterexample/evidence |
| --- | --- | --- |
| Configuration | Single nearest pyproject, explicit precedence/path bases, exact rule list | Nested cwd/config roots, exclusions, malformed settings and symlink escapes |
| Policy | Advisory fail-on-error compatibility; strict adds possible/incomplete failure; TP3001 warning | Full exit truth table, suppression/threshold and cancellation interactions |
| Schema | Checker 2, inspection 0 preserved; baseline/capability/explanation 1 | Old/new consumer fixtures, explicit incompatible-version failures and migration notes |
| Suppression | Reasoned same-line exact-code directive; mandatory diagnostics ineligible | Multiline primary/related sites, malformed/unused directives and retained coverage |
| Baselines | Versioned semantic-anchor/evidence match; ambiguity surfaces findings; explicit atomic update | Insert identical finding, rename/reorder, semantic change, stale entry and interrupted write |
| Must-use | Synchronous immediate handling; Annotated alias plus identity-preserving decorator; no assigned-value tracking | Mixed forms, descriptor ordering, shadowing, cleanup overlap, unsupported async consumption |
| Reporting limits | Inherit analyzer budgets; 64 KiB config, 16 MiB/100,000-entry baseline, 32 MiB report and bounded per-finding evidence as above | Measure ordinary/near-limit cases and failure recovery before qualification |
| Pilot evaluation | Three projects including async, at least 30 definite findings, at least 90% actionable | Execute the frozen selection and labeling protocol; retain counts, disagreements and insufficient-sample behavior |

Support methods only where exact binding/descriptor identity is available. Qualify `@must_use` directly on a function and inside known `@staticmethod`/`@classmethod` wrappers; reversed/unknown wrapper order requires a declaration error or explicit boundary, never guessed parity. Helper/type-checker evidence must precede a qualified support claim. Changing an accepted default updates V04 expectations and relevant shared plans before it becomes a support claim.

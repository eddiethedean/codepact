# 0.8 foundations specification

**Implemented preview.** The [phase plan](../0.8-reviewed-concurrency-foundations.md)
defines the required gates. The [validation matrix](validation.md) defines
the cases that qualify the implementation. Candidate acceptance is tracked
in the [implementation evidence](implementation-evidence.md).

## Provider selection and normalized facts

1. Reuse the 0.7 ordered project and dependency roots. Resolve the import
   before reading publication or project-model facts. A local package or
   earlier unmarked provider can win. For a regular package, subsequent
   submodules resolve within that package. Resolve aliases and explicit
   same-distribution re-exports to a canonical definition and retain the
   public name used by the caller. Track statically resolvable future and
   string annotations, qualified typing names and relative imports needed by
   the pilot. Dynamic exports, namespace merging, custom hooks and opaque
   editable or native layouts carry explicit coverage reasons.
2. Verify installed ownership from the selected distribution's metadata and
   file inventory. Retain the normalized distribution and version, import
   scope, canonical symbol, evidence kind and location and applicable file
   digests. Standard-library entries use the selected runtime profile in place
   of distribution identity. A different root order, overlapping ownership
   or changed file contents can change applicability at the same import.
3. Lower inline source, a manifest beside `py.codepact`, a reviewed catalog
   and project-model entries into the same semantic vocabulary. Within the
   selected provider and artifact, identify normalized facts by canonical
   symbol, contract kind and subject. Separate call-entry requirements from
   normal, exceptional and cancelled effects. Preserve each origin and its
   applicability. Carry 0.7's immediate must-use behavior,
   plus the bounded resource factory, receiver cleanup and supported
   ownership-transfer effects used by E10-A and E11.
4. Within a marked publication, a present manifest defines the published
   surface. Match the selected provider and the resolved public export to a
   manifest entry, then apply the facts published by that entry. A local
   import alias keeps the identity of the export it references. When two
   public names point to the same definition, an omitted name has no
   published contract through that name. An independently applicable model
   can still supply its own facts. If the manifest is stale, invalid or uses
   an unsupported schema, leave the affected publication unavailable with a
   reason. With a supported manifest and an unknown mandatory capability,
   leave the dependent facts unavailable and retain independently understood
   facts. Preserve the selected provider's released inline path when the
   manifest is absent.
5. A reviewed applicable model or source-verified effect can establish a
   supplier guarantee. Publisher declarations and artifact digests record
   intent and consistency. The engine needs behavioral evidence to rely on
   cleanup or transfer to discharge a duty. A successful owned factory
   return delivers one caller duty. A valid explicit `move(src)` invalidates
   the source when it evaluates. The evaluated argument carries the cleanup
   duty until the qualified callee accepts it at entry. If evaluation fails
   before the move, the source keeps the duty. If a later argument or
   signature binding fails after the move, the source stays moved and the
   unaccepted duty remains outstanding. Once the callee accepts the duty,
   apply its qualified effects on the corresponding exit. A project override
   names the selected distribution, bounded versions, symbol and contract
   kind and records the replaced origin. Contradictory applicable facts
   without an authorized override leave the affected dimension unknown.

The E11 pilot selects an installed regular wheel, an exact distribution
version, a resource and its close method, and a resolved synchronous factory
with a fresh open owned return. A successful close discharges the duty for
that instance. A raised close leaves cleanup unknown unless a qualified
exceptional effect applies. Validate a supplied `uv.lock` against the selected
installed version. Start with a unique, unconditional package entry in a
version 1, revision 3 lockfile, the format used by the repository's lockfile.
A missing, mismatching or ambiguous entry blocks the affected model and
reports the reason. Conditional or multi-source
entries that cannot be selected precisely have the same boundary.
Installed evidence supplies ownership and artifact applicability. Malformed
or conflicting project contract files are configuration errors and exit
`2`. [uv lockfile documentation](https://docs.astral.sh/uv/concepts/projects/layout/#the-lockfile)

## Author tooling and analysis reports

- Export deterministic manifest bytes from staged package inputs using the
  existing inspector and normalization. Include the import scope, distribution
  identity, target bounds, public and canonical symbols, evidence and digests
  of the packaged inputs. Version the artifact format independently of the
  report schema and engine release. The author builds the wheel as a separate
  step. Verify the built archive's marker, manifest, distribution identity,
  listed files and wheel `RECORD`. Validate paths and bound reads before decoding.
  Export, verification and consumer analysis read data only.
  [Published contracts design](../../published-contracts.md#author-workflow-release-requirements) ·
  [Wheel specification](https://packaging.python.org/en/latest/specifications/binary-distribution-format/)
- Use the existing CLI discovery as the filesystem host. Give Rust callers
  a way to supply equivalent resolved inputs and apply the same
  normalization, selection and flow rules. Keep existing source-only entry
  points and opt-in behavior. Use tracked inputs in the Salsa session for
  provider identity, applicable facts, metadata or model revisions, policy,
  target profile and scope. A semantic edit refreshes affected callers and
  coverage. A location edit refreshes evidence locations while preserving
  semantic finding identity.
- Report model use on clean results as well as findings. Record the selected
  public and canonical symbols, fact, origin, applicability, evidence location
  and rejection reason where relevant. Retain selected files, provider
  artifacts, caller closure within the named scope, unresolved siblings,
  exclusions and reached limits. A gap or exhausted budget produces incomplete
  coverage at affected operations and preserves independently established
  findings. Version report changes using the existing output contracts.
  [Report schemas](../../../output.md)

## Proposed model profiles

Use CPython **3.13.16 on Linux x86_64** as the first candidate for the new
models. Record the runtime build and upstream source hashes during
qualification. Preserve the previously qualified TaskGroup and contextlib
profiles and their existing controls.

The CLI reads the analyzed runtime profile from explicit project settings.
Rust callers supply the same inputs through the analysis request. Include
implementation, exact Python version, OS and architecture. Keep the Python
source target separate and require it to match the profile's major and minor
version. For the new asyncio effects, require declared use of the default
CPython event loop and non-eager task factory, using the existing
default-factory assumption where applicable. For multiprocessing, require
the resolved literal `get_context("spawn")`. Check any configured start
method against the resolved context. The MP01 untimed-join effect also
requires the wait assumptions below as part of the analyzed profile.

Record the selected profile, assumptions and applicability reason in
effective settings and model-use reports. Missing, unsupported or conflicting
inputs leave the dependent new effect unavailable and report incomplete
coverage where analysis needs it. A known contradiction in the analyzed code
invalidates the affected assumption. Use the analyzed runtime profile even
when the checker runs on a different host. Packaging platform coverage and
analyzed model applicability have separate evidence.

| Gate | Initial profile | Effects required |
| --- | --- | --- |
| LC04 | Resolved built-in `open` on ordinary local paths with default mode or literal `"r"` or `"w"`, with no custom opener. Cover `read` or `write` as appropriate, `close` and direct `with` using the tracked file. | A successful open creates one owned file. Entry aliases it. A successful close or modeled exit closes the file for every local alias. A failed open delivers no caller handle. Unsupported modes, openers and unqualified I/O failure effects retain their own boundaries. |
| AS01 | Source-resolved local coroutine calls, direct await, `asyncio.create_task` and existing bounded `TaskGroup.create_task`, with a default non-eager factory and local task identities. | Coroutine creation records captures and performs argument evaluation. Scheduling success creates a task. Directly awaiting the coroutine runs it in the current task. Awaiting the tracked task or completing the group exit releases the corresponding borrow only on established completion paths. Rejected scheduling accounts for the coroutine and any already evaluated move using the exact entry point's effects. |
| AS02 | Local task cancellation, `asyncio.timeout` around a direct task wait, and `asyncio.shield` of an already tracked task in the same bounded scope. Include a timeout around the shielded wait. | A request can precede execution, interrupt cleanup or be suppressed. Use the specific wait and exit edge for underlying completion. A cancelled or timed-out shield wrapper can leave the inner task live. When an await establishes termination, apply only cleanup effects established by the underlying path. Group exit can complete siblings' work even while propagating an error. |
| MP01 | Local handles from literal `multiprocessing.get_context("spawn").Process`, with a statically resolved importable fixture worker and plain serialized arguments. Cover `start`, `join`, `is_alive`, `exitcode` and `close`. | Track unstarted, started, terminated and closed handle states. A normal untimed join establishes termination only under the qualified wait assumptions. Timed joins and untimed joins with unqualified waits leave termination unproved unless independent evidence establishes it. A successful close settles the local handle, including an unstarted handle. Worker failure can still terminate the process. A recognized pre-launch failure has no successful start effect. Unclassified failures retain uncertainty. |
| MP02 | Local `get_context("spawn").Pool` with a fixed small worker count, `apply_async` of a resolved worker over plain values, explicit `close` or `terminate` and `join`, and `with` exit. | Successful submission records acceptance at the Pool. `close` stops new submissions and allows admitted work to drain before a successful join. Forced shutdown may abandon work. A successful context exit follows the reviewed forced termination and join path. Submission rejection before enqueue and downstream delivery failure have distinct effects. |

For MP01, qualify the untimed wait for a local started child with ordinary
POSIX child reaping and signal handling. The model requires that no competing
reaper or signal handler can prevent `waitpid` from delivering a child status.
POSIX spawn inherits a wait that can swallow `OSError`, so an interrupted
untimed join can return `None` while the child is live. With an unqualified or
interrupted wait, retain the prior handle state and report incomplete coverage
when the caller needs termination evidence. On a proven started path, a
subsequent `is_alive() is False` or `exitcode is not None` can establish
termination. [CPython POSIX spawn](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/popen_spawn_posix.py#L24) ·
[CPython wait implementation](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/popen_fork.py#L22-L41)

Treat unsupported factories, targets, callbacks, escaped identities, ambient
process defaults, alternate start methods and live handle serialization as
explicit boundaries. Keep capture, completion or termination, observation,
acknowledgment and cleanup independent. The worker creates and cleans its
own resource in the positive control. The parent protocol can establish the
parent's handle state while worker resource cleanup requires its own evidence.
Work observation and member accounting remain AS03/MP03 in 0.9.
[CPython asyncio tasks](https://github.com/python/cpython/blob/v3.13.16/Lib/asyncio/tasks.py) ·
[CPython Process](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/process.py) ·
[CPython Pool](https://github.com/python/cpython/blob/v3.13.16/Lib/multiprocessing/pool.py) ·
[Python file I/O](https://docs.python.org/3.13/library/io.html)

## Decisions for the 0.9 handoff

- CR01 accepts the comparison descriptor, normalized snapshot inputs and
  preservation prerequisites. Source edits are comparable under the same
  semantic context. Lost scope, lost applicability, non-comparable semantics,
  incomplete evidence or ambiguous required correspondence cannot establish
  preservation. The 0.8 provider and scope records supply inputs for the 0.9
  comparator. [Change review v1](../../change-review.md)
- OH01 accepts opt-in production for resolved exact CodePact Result or Option
  callable returns with ordinary payloads. Successful delivery forwards one
  pending duty or creates one outward duty. An exception before delivery
  creates no caller duty. Keep unsupported declaration positions explicit
  and preserve unmarked outcomes and released immediate MustUse behavior.
  Track the duty through calls and assignments in 0.9.
  [Persistent outcomes v1](../../persistent-outcomes.md)

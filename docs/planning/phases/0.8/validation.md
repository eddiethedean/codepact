# 0.8 validation matrix

The [phase plan](../0.8-reviewed-concurrency-foundations.md) defines release
gates. The [specification](specification.md) defines the bounded profiles.
The [implementation evidence](implementation-evidence.md) records local checks.
Compare inherited behavior with the published [0.7.0 evidence](../../../releases/0.7.0-evidence.md).

## Required cases

| Row | Gates | Fixtures and required observations |
| --- | --- | --- |
| V08-01 | LC01–LC02, PC01 | Resolve direct, aliased and explicit same-distribution re-exported declarations, including supported future and string annotations, qualified typing names and relative imports. Record the public and canonical identities and the selected distribution and version. A dynamic export, unresolved annotation or cross-distribution re-export with no applicable origin retains an explicit boundary. |
| V08-02 | LC02, LC06, PC06 | Run a marked inline provider alone, with an earlier unmarked wheel, and with a local package of the same import name. Verify submodule resolution remains in the winning regular package. Reorder the roots and add or remove the local shadow with the caller held fixed. Compare fresh and reused inline-provider selection and caller invalidation. Exercise ambiguous file ownership, unresolved siblings and file/edge/byte budget exhaustion. Report selected roots, actual caller scope and applicable or rejected facts even on clean runs. |
| V08-03 | LC01, LC03, E10-A | Build equivalent provider variants in inline and exported manifest forms. In independent environments, analyze the same downstream factory leak/repair, owned argument transfer and invalid use after transfer, and exceptional path with an unknown cleanup or transfer effect. Assert that failure before `move(src)` preserves the owning source. For both a later argument failure and a signature binding failure after the move, assert that the source stays moved and the unaccepted duty remains outstanding. Compare normalized effects, finding certainty and coverage. Assert the distinct publication origins, and use a publisher-only cleanup promise as the unqualified control. |
| V08-04 | LC08, E10-A, E04–E05 | Use the installed CLI to export twice from equivalent staged inputs and compare bytes. Explicitly build the wheel, verify the actual archive using the installed CLI, and install the provider and caller outside the checkout. Validate marker and manifest inclusion, identity, file digests and `RECORD`. Put execution sentinels in the controlled provider and caller and check that export, verify and check leave them untouched. Verify the catalog through the shipped CLI and source routes and replay the helper-only, exact CLI extra and standalone installs. |
| V08-05 | LC02, LC06, LC08, E10-A | For a wheel intended to publish a manifest, reject missing marker or manifest, wrong distribution or import scope, stale digests, malformed records or mismatched wheel files. Publish one of two public aliases for the same canonical callable. In a consumer with no independent model, only the manifest-listed alias receives its published effect. Exercise an installed file edit at the same version, unsupported manifest schema, and unknown mandatory capability alongside an independent supported fact. Assert the consumer's coverage reason, unavailable dependent effects and retained unrelated facts. |
| V08-06 | LC03, E11 | Install the exact unmarked provider matched by the project model. Pair a fresh-owned factory leak with close and repaired exit flow. Catch a raised close and verify cleanup remains unknown unless the applicable exceptional effect establishes it. Test an explicit bounded override and assert both effective and replaced origins. |
| V08-07 | LC02, LC06, E11 | Compare matching, mismatching and ambiguous `uv.lock` entries using the selected installation. Upgrade the provider with the old model, shadow it and exercise missing/ambiguous ownership. Assert affected incompleteness and absence of a stale cleanup effect. Malformed or conflicting project contract inputs exit `2`. |
| V08-08 | LC03–LC04 | Use an ordinary file with aliased reads/writes, explicit close and direct `with`. Pair leak and use after close with repaired controls. Cover failed open, exception inside `with`, and a cleanup path with no qualified exceptional effect. Keep a rebinding of `open` and an unsupported custom opener as boundary controls. Confirm lifecycle facts with real temporary files in the controlled runtime suite. |
| V08-09 | LC06, LC08, PC06, E10-A, E11, G4 | Exercise provider upgrade, root reorder, shadowing, manifest and model edits, lock mismatch and recovery, and declaration span movement while holding the caller bytes fixed. Compare CLI and Rust results and reused versus fresh sessions. Assert refresh of affected callers and provenance, unchanged semantic identity for a span-only edit and reuse for an unrelated edit. |
| V08-10 | AS01 | Pair completion before close with a known pending borrower at close. Exercise direct await, an aliased scheduled task and existing TaskGroup children. Creating a coroutine leaves a pending capture. An unawaited coroutine, an await outside the owner scope and an escaping capture each retain their specific outstanding state or boundary. |
| V08-11 | AS01–AS02 | Exercise rejected scheduling, including a move during argument evaluation, and a task cancelled before its first execution. Distinguish argument evaluation, coroutine disposal, task creation and execution of body/finally. Pin the released version-specific TaskGroup rejection controls. |
| V08-12 | AS02 | Pair cancellation request followed by close with cancellation followed by an await that establishes termination before close. Exercise propagated and suppressed cancellation, interrupted wait and interrupted awaited cleanup. Check the remaining cleanup duty independently of task state. |
| V08-13 | AS02 | Exercise `asyncio.timeout` around a direct tracked task wait and around `shield(task)`. Check genuine termination, timeout and interrupted wrapper paths separately. With a gated inner task, verify that shield cancellation or timeout can leave the underlying borrower live. Pair with a subsequent underlying await before close. |
| V08-14 | AS02, CS05 | Replay exceptional TaskGroup exit with a child failure and a sibling that still holds a borrow while unwinding. Accept owner close after established group completion. Retain child obligations on paths lacking that proof and verify the effect of cancelled or failed child cleanup independently. |
| V08-15 | MP01 | Qualify successful start, untimed join under the wait assumptions and close through aliases. Cover join before start, duplicate start, use after close, false `is_alive` and valid close on an unstarted handle, and failed start before launch. On Linux, use a gated spawn child for the timed join and interrupt a separate untimed join with a signal handler that raises `OSError`. Assert that each wait can return while the child is live. Require the static cases to retain uncertainty when the wait assumptions are missing or contradicted and to recognize a separate positive termination check on a started path. Diagnose premature close at the certainty supported by the feasible paths. |
| V08-16 | MP01–MP02 | Use an explicit-spawn worker that constructs and closes its file locally and uses plain values as input and, for the Pool worker, output. Exercise worker failure and parent-side join/close without inferring worker success, cleanup or returned-result observation. Use a live parent handle, an opaque worker and wrong or implicit start methods as boundaries. |
| V08-17 | MP02 | Qualify valid submission and `close` then `join`. Show failed submission, submission after close or terminate, and join on a running Pool. Keep an injected rejection before enqueue distinct from serialization or delivery failure after acceptance. Draining and joining leave result observation unset. |
| V08-18 | MP02 | Exercise explicit `terminate` then `join`, plus normal and exceptional Pool context exit. Use controlled unfinished work at forced exit and assert only the reviewed shutdown effects. Interrupted or failed shutdown keeps uncertainty about remaining work or cleanup. |
| V08-19 | CR01, OH01, PC06 | Review the written descriptor and preservation prerequisites against clean source edits, lost provider applicability, narrowed scope, incomparable settings and exhausted budgets. Review the opt-in delivery, forwarding and renewal rules against unmarked, released MustUse, unsupported positions and raising-before-return examples. Record the design decisions for 0.9. |
| V08-20 | LC04, AS01–AS02, MP01–MP02, G1–G8 | Pair a matching explicit model profile with a source target alone, a mismatched source target and runtime profile, a wrong patch version or platform, and conflicting loop, factory or start-method inputs. Check applicability and coverage when host and analyzed profiles differ. Change model or profile inputs while caller bytes stay fixed and include moved related locations and an exhausted budget. Check CLI/Rust and fresh/reused parity, including recovery after a limit. Re-run inherited regressions, installed CLI smoke tests and schema checks. Record cold, unchanged, affected and unrelated run costs against the 0.7 baseline. |

## Evidence rules

- **Static analysis.** Record expected finding codes, certainty and primary
  and related spans before running a fixture. Assert coverage and affected
  operation, pending duties and applicable evidence, including negative
  assertions for unsupported effects. A coverage-only check exits `0` under
  advisory defaults and `1` in strict. A baseline changes eligible finding
  disposition while retaining raw coverage. Compare semantic results
  across CLI and Rust, allowing each interface its own presentation.
- **Runtime conformance.** Register exact probe IDs and the accepted
  runtime profiles alongside the existing model suites. Verify platform,
  Python patch and applicable start method, loop, task factory or wait
  assumptions before counting a run. Retain receipts with actual runtime
  identity, model and probe source hashes and executed/pass counts. Use
  explicit synchronization or barriers for timing-sensitive tests, bounded
  waits and cleanup for every spawned worker. Execute only
  repository-controlled probes. A skip, wrong runtime or unsupported-boundary
  fixture supplies no positive model evidence.
  [Conformance workflow](../../testing.md#exact-runtime-model-qualification)
- **Candidate record.** Retain the exact candidate and artifact hashes,
  dependency roots, provider versions, lock and model identities, scope,
  limitations, fixture commands and time and peak memory measurements.
  Record E10-A and E11 separately. Composite E10 completes only after its
  later checkpoints. [Release evidence template](../../release-evidence-template.md)

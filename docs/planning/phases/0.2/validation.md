# 0.2 validation matrix

Status: **No row is accepted as qualified yet.** Existing source and tests cover part of this matrix; the candidate evidence record must link exact results before any row or gate is marked passed. The [specification](specification.md) is the oracle; the [phase plan](../0.2-lifecycle.md) defines L01–L06. IDs below are independent of 0.1 V01–V24.

## Required matrix

| ID | Fixture / action | Expected result | Gates |
| --- | --- | --- | --- |
| V02-01 | Exact Connection tutorial; remove final use for valid counterpart | Exactly one definite TP1001 at final call, related close span; valid counterpart has no violations/boundaries | L01 |
| V02-02 | Decorator/explicit lifecycle × shorthand/verbose access; conflicting metadata; undecorated base-only reference | Shared semantics agree; provenance retained; conflict is declaration error; alias metadata does not mutate base | L01, L04 |
| V02-03 | Alias then close; rebind alias before close; distinct constructions | Alias invalidation, rebinding detachment and independent objects | L02 |
| V02-04 | Close in one/both/neither branch then use | Possible/definite/absent finding; branch origins retained | L02 |
| V02-05 | Branch chooses a or b into x; close x, use originals; same-object counterpart | Weak updates for uncertain targets; strong only for same exact object | L02 |
| V02-06 | Branch-dependent alias/unbound local; alias-set budget | No open proof from missing paths or truncated sets; visible boundary as needed | L02, L05 |
| V02-07 | Close Borrowed directly, via alias/helper; non-consuming status counterpart | Forbidden close at operation with annotation provenance; supported read passes | L03 |
| V02-08 | Helper closes on some/all paths; same resource in two parameter positions; return before operation | Correct conditional effects, alias-aware order or explicit boundary; unreachable operation excluded | L03, L05 |
| V02-09 | Shadowing/rebinding, captured old aliases, function-local names, unrelated close methods | Semantics follow resolved identity; lookalikes gain none | L04 |
| V02-10 | Discover resource/helper through named re-exports; reorder entry files | Same contract/call identity, findings, ordering and coverage | L04 |
| V02-11 | Missing module, duplicate module, file/package collision, import cycle, symlink escape | Deterministic bounded error/boundary; no outside-root traversal | L04, L05 |
| V02-12 | Unknown helper/method receives resource; zero-argument unknown call after escape; resource storage/return | Reachable state unknown; escape persists; no guessed purity or alias relation | L03, L05 |
| V02-13 | Normal close vs try/except/finally; unsupported constructor/method; repeat close | Normal transition precise; exceptional/unsupported behavior conservative; declared trust retained; no invented repeat-close rule | L03, L05 |
| V02-14 | Change callee read to conditional/definite close; change annotation/model trust | Affected callers update; fresh and updated facts/findings/coverage agree | L06 |
| V02-15 | Add/delete module, change re-export/root/target version; unrelated edit | Relevant invalidation; unrelated caller checks reuse, proven by query events | L06 |
| V02-16 | Move provider close/annotation lines without changing effects | Related spans refresh; summary semantics unchanged; unaffected semantic work reused | L06 |
| V02-17 | No-op update; cancel/recover; recursion; CFG/alias/call/import budgets | No stale publication/panic; correct revision; session recovers; exhaustion visibly incomplete | L05, L06 |
| V02-18 | Side-effect marker; loop/with/async/closure; file reorder/repeat | Marker absent; relevant state weakened; deterministic results/coverage | L04, L05 |
| V02-19 | Rust one-shot/session vs text CLI; definite/possible/boundary/invalid-input | Facts/spans/certainty agree; documented exits 1/0/0/2; visible coverage | L01 |
| V02-20 | Clean tutorial/artifact install; inherited inspection/helper/checker suite | Reproduced finding; inspection output/assurance preserved; lightweight typing-compatible helpers | L01 |

## Assertions and differential checks

Each row needs concrete source files, primary/related spans, normalized facts or summaries, certainty, coverage reasons and exits where applicable. Include valid, invalid and unknown counterparts. Golden output does not replace assertions about state, joins or missing coverage.

Retain representative CFG/summary snapshots for straight-line, branch and return cases. Cross-interface comparisons remove source-only provenance/locations while retaining lifecycle, access, effects, certainty and trust. Keep location snapshots separately. Across session resets compare alias/state relationships rather than database-local object IDs.

Incremental cases start cold, apply an edit to a reused session, then compare with a new session on edited source. Record query execution events for invalidation and reuse; timings alone prove neither. Compare findings, facts, spans, certainty, provenance, coverage and revision ownership.

Bounded generated branch/alias cases can additionally check that joins never improve uncertainty into safety and replacing a known summary with unknown never creates an open proof. They complement required counterexamples.

## Qualification record

For every row retain fixture path, candidate revision, invocation, expected/actual result, log/snapshot link and reviewer. Pending fixtures are not passes. Record an issue/owner for failures. Re-run affected rows after changes; explain reuse of unaffected evidence.

Measure fixed small/medium corpora: cold/warm checks, body/effect/location edits, elapsed time, peak/retained memory and query counts. Record machine, toolchain, interpreter, corpus hashes and limits. Compare parsing/import measurements with 0.1. Accept numerical budgets in LC01 after measurement; unexplained regressions hold G4.

Run inherited formatting, static analysis, Rust, Python/checker and artifact checks as applicable. Local results do not imply new platform support. Complete L01–L06 and G1–G7 in the candidate record; maintainer tag authorization and post-publication G8 remain separate.

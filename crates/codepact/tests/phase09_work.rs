use codepact::{
    check_with_foundations,
    foundation::{FoundationInputs, RuntimeProfile},
    AnalysisSession, CheckOptions, CheckResult, Ownership, SourceFile,
};

fn options() -> CheckOptions {
    CheckOptions {
        ownership: Ownership::AdvancedAssumeDefaultTaskFactory,
        ..CheckOptions::default()
    }
}

fn inputs() -> FoundationInputs {
    FoundationInputs {
        runtime: Some(RuntimeProfile {
            implementation: "cpython".into(),
            python_version: "3.13.16".into(),
            os: "linux".into(),
            architecture: "x86_64".into(),
            event_loop: Some("default".into()),
            start_method: Some("spawn".into()),
            assume_standard_process_wait: true,
        }),
        ..FoundationInputs::default()
    }
}

fn check(source: &str) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new("work.py", "work", source)],
        "3.13",
        options(),
        inputs(),
    )
}

fn observations(report: &CheckResult) -> Vec<usize> {
    report
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "TP3003")
        .map(|finding| finding.primary.line)
        .collect()
}

#[test]
fn asyncio_values_require_observation_after_task_group_and_future_completion() {
    let result = check(
        r#"from asyncio import TaskGroup, create_task, get_running_loop
async def worker(): return 1
async def observed_task():
    task = create_task(worker())
    alias = task
    await alias
    task.result()
async def grouped():
    async with TaskGroup() as group:
        stored = group.create_task(worker())
    stored.done()
async def local_future():
    loop = get_running_loop()
    future = loop.create_future()
    alias = future
    future.set_result(1)
    alias.exception()
async def observed_future():
    future = get_running_loop().create_future()
    alias = future
    future.set_result(1)
    await alias
"#,
    );
    assert_eq!(observations(&result), [10, 14], "{result:#?}");
}

#[test]
fn observed_local_future_has_complete_coverage() {
    let source = r#"from asyncio import get_running_loop
async def observed():
    loop = get_running_loop()
    future = loop.create_future()
    future.set_result(1)
    return await future
"#;
    let result = check(source);
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(observations(&result).is_empty(), "{result:#?}");

    let source = source.replace("return await future", "return future.exception()");
    let result = check(&source.replace("set_result(1)", "set_exception(ValueError())"));
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(observations(&result).is_empty(), "{result:#?}");
    for invalid in ["1", "StopIteration()"] {
        let result = check(&source.replace("set_result(1)", &format!("set_exception({invalid})")));
        assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
        assert!(result
            .foundations
            .unwrap()
            .model_uses
            .iter()
            .any(|usage| usage.symbol == "asyncio.Future"
                && usage.effect == "set_exception"
                && !usage.applicable));
    }
}

#[test]
fn bounded_wait_and_completion_iterator_do_not_observe_members() {
    let result = check(
        r#"from asyncio import as_completed, create_task, wait
async def worker(): return 1
async def waited():
    task = create_task(worker())
    await wait((task,), timeout=1)
async def iterated():
    task = create_task(worker())
    async for ready in as_completed((task,)):
        ready.result()
"#,
    );
    let uses = &result.foundations.as_ref().unwrap().model_uses;
    assert!(
        uses.iter()
            .any(|site| site.symbol == "asyncio.as_completed" && site.applicable),
        "{result:#?}"
    );
    assert!(
        uses.iter().any(|site| site.symbol == "asyncio.Task"
            && site.effect == "result"
            && site.applicable),
        "{result:#?}"
    );
    assert!(observations(&result).contains(&4), "{result:#?}");
    let iterated_findings = result
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "TP3003" && finding.primary.line == 7);
    assert!(
        iterated_findings
            .into_iter()
            .all(|finding| finding.related.iter().all(|site| site.line != 9)),
        "{result:#?}"
    );
}

#[test]
fn pool_alias_observes_only_on_successful_or_exceptional_get() {
    let result = check(
        r#"from multiprocessing import get_context
def worker(): return 7
def fails(): raise ValueError()
def loses_after_drain():
    pool = get_context("spawn").Pool(1)
    pending = pool.apply_async(worker)
    pending.wait()
    pending.successful()
    pool.close()
    pool.join()
def gets():
    pool = get_context("spawn").Pool(1)
    pending = pool.apply_async(worker)
    alias = pending
    alias.get()
def gets_worker_error():
    pool = get_context("spawn").Pool(1)
    pending = pool.apply_async(fails)
    try:
        pending.get()
    except ValueError:
        return
"#,
    );
    assert_eq!(observations(&result), [6], "{result:#?}");
}

#[test]
fn thread_result_timeout_and_context_exit_keep_observation_independent() {
    let result = check(
        r#"from concurrent.futures import ThreadPoolExecutor
def worker(): return 7
def fail(): raise ValueError()
def loses_value():
    with ThreadPoolExecutor() as executor:
        stored = executor.submit(worker)
    stored.exception()
def timed():
    with ThreadPoolExecutor() as executor:
        stored = executor.submit(worker)
        try:
            return stored.result(timeout=0)
        except TimeoutError:
            return
def observed():
    with ThreadPoolExecutor() as executor:
        stored = executor.submit(fail)
        alias = stored
        alias.exception()
"#,
    );
    assert_eq!(observations(&result), [6, 10], "{result:#?}");
}

#[test]
fn work_result_delivery_preserves_must_handle_identity_after_retrieval() {
    let result = check(
        r#"from concurrent.futures import ThreadPoolExecutor
from asyncio import create_task
from codepact import MustHandle, Result, Ok, Err, discard_outcome
def worker() -> MustHandle[Result[int, str]]: return Ok(1)
async def async_worker() -> MustHandle[Result[int, str]]: return Ok(1)
def observed_thread():
    with ThreadPoolExecutor() as executor:
        pending = executor.submit(worker)
        first = pending.result()
        discard_outcome(first, reason="checked")
        again = pending.result()
async def observed_task():
    pending = create_task(async_worker())
    first = await pending
    discard_outcome(first, reason="checked")
    again = pending.result()
"#,
    );
    assert!(observations(&result).is_empty(), "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .all(|finding| finding.code != "TP5002"),
        "{result:#?}"
    );
}

#[test]
fn gather_delivers_fixed_slots_without_acknowledging_their_outcomes() {
    let result = check(
        r#"from asyncio import create_task, gather
from codepact import MustHandle, Result, Ok, discard_outcome
async def marked() -> MustHandle[Result[int, str]]: return Ok(1)
async def fail(): raise ValueError()
async def handled():
    task = create_task(marked())
    sibling = create_task(fail())
    value, error = await gather(task, sibling, return_exceptions=True)
    discard_outcome(value, reason="reviewed")
    again = task.result()
async def missing():
    task = create_task(marked())
    sibling = create_task(fail())
    value, error = await gather(task, sibling, return_exceptions=True)
"#,
    );
    let uses = &result.foundations.as_ref().unwrap().model_uses;
    assert!(
        uses.iter()
            .any(|site| site.symbol == "asyncio.gather" && site.applicable),
        "{result:#?}"
    );
    let outcome_findings = result
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "TP5002")
        .collect::<Vec<_>>();
    assert_eq!(outcome_findings.len(), 1, "{result:#?}");
    assert_eq!(outcome_findings[0].primary.line, 11, "{result:#?}");
    assert!(
        outcome_findings[0]
            .related
            .iter()
            .any(|site| site.line == 14),
        "{result:#?}"
    );
}

#[test]
fn pool_timeout_is_distinct_from_builtin_timeout() {
    let result = check(
        r#"from multiprocessing import get_context, TimeoutError as PoolTimeout
def worker(): return 7
def timed():
    pool = get_context("spawn").Pool(1)
    pending = pool.apply_async(worker)
    try:
        pending.get(0)
    except TimeoutError:
        pending.get()
    except PoolTimeout:
        return
"#,
    );
    assert_eq!(observations(&result), [5], "{result:#?}");
}

#[test]
fn running_future_can_reject_cancellation_and_remain_unobserved() {
    let result = check(
        r#"from concurrent.futures import ThreadPoolExecutor
def worker(): return 7
def submitted():
    executor = ThreadPoolExecutor()
    future = executor.submit(worker)
    if future.running():
        if future.cancel():
            abandoned = open("unreachable.txt")
        return
    future.result()
"#,
    );
    assert_eq!(observations(&result), [5], "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .all(|finding| finding.code != "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn a_settled_local_future_retains_the_producers_outcome_identity() {
    let result = check(
        r#"from asyncio import get_running_loop
from codepact import MustHandle, Result, Ok, discard_outcome
def produce() -> MustHandle[Result[int, str]]: return Ok(1)
async def handles_from_future():
    future = get_running_loop().create_future()
    value = produce()
    future.set_result(value)
    del value
    delivered = await future
    discard_outcome(delivered, reason="handled")
    again = future.result()
async def misses():
    future = get_running_loop().create_future()
    future.set_result(produce())
    delivered = future.result()
"#,
    );
    assert!(observations(&result).is_empty(), "{result:#?}");
    let missed = result
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "TP5002")
        .collect::<Vec<_>>();
    assert_eq!(missed.len(), 1, "{result:#?}");
    assert_eq!(missed[0].primary.line, 12, "{result:#?}");
}

#[test]
fn work_observation_edits_refresh_reused_analysis() {
    let mut session = AnalysisSession::default();
    let unchanged = "from asyncio import create_task\nasync def worker(): return 1\nasync def run():\n    pending = create_task(worker())\n";
    let fixed = format!("{unchanged}    alias = pending\n    await alias\n");
    for (source, expected) in [
        (unchanged, &[4][..]),
        (&fixed, &[][..]),
        (unchanged, &[4][..]),
    ] {
        let files = [SourceFile::new("work.py", "work", source)];
        let reused = session.check_with_foundations(&files, "3.13", options(), inputs());
        let fresh = check(source);
        assert_eq!(observations(&reused), expected);
        assert_eq!(
            serde_json::to_value(reused).unwrap(),
            serde_json::to_value(fresh).unwrap()
        );
    }
}

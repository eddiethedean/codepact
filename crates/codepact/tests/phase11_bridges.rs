use codepact::{
    check_with_foundations,
    foundation::{FoundationInputs, RuntimeProfile},
    CheckOptions, CheckResult, Ownership, SourceFile,
};

const SCRIPT: &str = "phase11_bridge.py";

fn check(source: &str) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new(SCRIPT, "phase11_bridge", source)],
        "3.13",
        CheckOptions {
            ownership: Ownership::AdvancedAssumeDefaultTaskFactory,
            ..CheckOptions::default()
        },
        FoundationInputs {
            runtime: Some(RuntimeProfile {
                implementation: "cpython".into(),
                python_version: "3.13.16".into(),
                os: "linux".into(),
                architecture: "x86_64".into(),
                event_loop: Some("default".into()),
                start_method: Some("spawn".into()),
                assume_standard_process_wait: true,
                spawn_main_script: Some(SCRIPT.into()),
            }),
            ..FoundationInputs::default()
        },
    )
}

fn applied(report: &CheckResult, symbol: &str, effect: &str) -> bool {
    report
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|site| site.symbol == symbol && site.effect == effect && site.applicable)
}

#[test]
fn bridge_coroutine_retains_capture_before_dispatch() {
    let report = check(
        r#"from asyncio import to_thread
def work(handle): return 42
async def bad():
    handle = open("work.txt")
    delayed = to_thread(work, handle)
    handle.close()
    delayed.close()
async def closed_before_dispatch():
    handle = open("work.txt")
    delayed = to_thread(work, handle)
    delayed.close()
    handle.close()
"#,
    );
    assert!(
        applied(&report, "asyncio.to_thread", "create-coroutine"),
        "{report:#?}"
    );
    assert!(
        applied(&report, "asyncio.to_thread", "close-before-dispatch"),
        "{report:#?}"
    );
    let conflicts: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP2006")
        .map(|diagnostic| diagnostic.primary.line)
        .collect();
    assert_eq!(conflicts, [6], "{report:#?}");
}

#[test]
fn pre_start_task_cancel_releases_unstarted_bridge_capture() {
    let report = check(
        r#"from asyncio import create_task, to_thread
def work(handle): return 42
async def run():
    handle = open("work.txt")
    task = create_task(to_thread(work, handle))
    task.cancel()
    handle.close()
"#,
    );
    assert!(
        applied(&report, "asyncio.create_task", "schedule-executor-bridge"),
        "{report:#?}"
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP2006"),
        "{report:#?}"
    );
}

#[test]
fn wrapper_cancellation_preserves_underlying_work_but_source_can_observe() {
    let report = check(
        r#"from asyncio import CancelledError, wrap_future
from concurrent.futures import ThreadPoolExecutor
def work(handle): return 42
async def bad():
    handle = open("work.txt")
    with ThreadPoolExecutor() as executor:
        original = executor.submit(work, handle)
        if original.running():
            wrapper = wrap_future(original)
            wrapper.cancel()
            handle.close()
        original.result()
async def good():
    handle = open("work.txt")
    with ThreadPoolExecutor() as executor:
        original = executor.submit(work, handle)
        wrapper = wrap_future(original)
        try:
            await wrapper
        except CancelledError:
            original.result()
        handle.close()
"#,
    );
    assert!(
        applied(&report, "asyncio.wrap_future", "link-existing-work"),
        "{report:#?}"
    );
    assert!(
        applied(&report, "asyncio.Future", "await-bridge-result"),
        "{report:#?}"
    );
    let conflicts: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP2006")
        .map(|diagnostic| diagnostic.primary.line)
        .collect();
    assert_eq!(conflicts, [11], "{report:#?}");
}

#[test]
fn direct_await_of_default_executor_records_dispatch_and_delivery() {
    let report = check(
        r#"from asyncio import CancelledError, get_running_loop
def work(): return 42
async def direct():
    loop = get_running_loop()
    try:
        return await loop.run_in_executor(None, work)
    except CancelledError:
        return
"#,
    );
    assert!(
        applied(
            &report,
            "asyncio.AbstractEventLoop.run_in_executor",
            "dispatch"
        ),
        "{report:#?}"
    );
    assert!(
        applied(&report, "asyncio.Future", "await-bridge-result"),
        "{report:#?}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP3003"),
        "{report:#?}"
    );
}

#[test]
fn process_context_exit_after_nonwaiting_shutdown_has_no_join_effect() {
    let report = check(
        r#"from concurrent.futures import ProcessPoolExecutor
from multiprocessing import get_context
def work(): return 42
if __name__ == "__main__":
    with ProcessPoolExecutor(max_workers=1, mp_context=get_context("spawn")) as executor:
        detached = executor.submit(work)
        executor.shutdown(wait=False)
    with ProcessPoolExecutor(max_workers=1, mp_context=get_context("spawn")) as other:
        joined = other.submit(work)
    detached.result()
    joined.result()
"#,
    );
    let symbol = "concurrent.futures.ProcessPoolExecutor.shutdown";
    assert!(
        applied(&report, symbol, "manager-already-detached"),
        "{report:#?}"
    );
    assert!(applied(&report, symbol, "join-manager"), "{report:#?}");
}

#[test]
fn known_thread_can_retrieve_coroutine_result_on_known_running_loop() {
    let report = check(
        r#"from asyncio import get_running_loop, run_coroutine_threadsafe, to_thread
async def worker(): return 42
def submit(loop):
    return run_coroutine_threadsafe(worker(), loop).result()
async def main():
    loop = get_running_loop()
    return await to_thread(submit, loop)
"#,
    );
    assert!(
        applied(
            &report,
            "asyncio.run_coroutine_threadsafe",
            "admit-loop-callback"
        ),
        "{report:#?}"
    );
    assert!(
        applied(
            &report,
            "concurrent.futures.Future",
            "retrieve-bridge-result"
        ),
        "{report:#?}"
    );
}

#[test]
fn executor_shutdown_does_not_deliver_pending_asyncio_wrapper_callback() {
    let report = check(
        r#"from asyncio import InvalidStateError, wrap_future
from concurrent.futures import ThreadPoolExecutor

def work(): return 12

async def run():
    with ThreadPoolExecutor(1) as pool:
        original = pool.submit(work)
        wrapped = wrap_future(original)
    try:
        wrapped.result()
    except InvalidStateError:
        leaked = open("example.txt")
        return
"#,
    );
    assert!(
        applied(&report, "asyncio.Future", "retrieve-bridge-result"),
        "{report:#?}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP1002"
                && finding.related.iter().any(|site| site.line == 13)),
        "{report:#?}"
    );

    let report = check(
        r#"from asyncio import wrap_future
from concurrent.futures import ThreadPoolExecutor

def work(): return 12

async def run():
    with ThreadPoolExecutor(1) as pool:
        original = pool.submit(work)
    wrapped = wrap_future(original)
    wrapped.result()
"#,
    );
    assert!(
        applied(&report, "asyncio.Future", "retrieve-bridge-result"),
        "{report:#?}"
    );
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
}

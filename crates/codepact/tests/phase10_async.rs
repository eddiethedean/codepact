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
            spawn_main_script: None,
        }),
        ..FoundationInputs::default()
    }
}

fn check(source: &str) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new(
            "async_protocols.py",
            "async_protocols",
            source,
        )],
        "3.13",
        options(),
        inputs(),
    )
}

fn findings<'a>(result: &'a CheckResult, code: &str) -> Vec<(usize, &'a str)> {
    result
        .diagnostics
        .iter()
        .filter(|finding| finding.code == code)
        .map(|finding| (finding.primary.line, finding.message.as_str()))
        .collect()
}

fn model_effect(report: &CheckResult, symbol: &str, effect: &str) -> bool {
    report
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|usage| usage.symbol == symbol && usage.effect == effect && usage.applicable)
}

#[test]
fn queue_admissions_are_distinct_across_same_payload_and_aliases() {
    let result = check(
        r#"from asyncio import Queue as FIFO

async def consume():
    queue = FIFO(2)
    alias = queue
    await queue.put(7)
    await alias.put(7)
    first = await alias.get()
    queue.task_done()
    second = queue.get_nowait()
    alias.task_done()
    await queue.join()
"#,
    );
    assert!(findings(&result, "TP7001").is_empty(), "{result:#?}");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Queue.join", "counter-zero"),
        "{result:#?}"
    );
}

#[test]
fn an_acknowledgment_on_another_queue_keeps_the_real_receipt_pending() {
    let result = check(
        r#"from asyncio import Queue

async def consume():
    first = Queue()
    second = Queue()
    first.put_nowait(1)
    second.put_nowait(1)
    await first.get()
    second.task_done()
    await second.join()
"#,
    );
    let queue = findings(&result, "TP7001");
    assert!(
        queue
            .iter()
            .any(|(line, text)| *line == 8 && text.contains("outstanding")),
        "{result:#?}"
    );
    assert!(
        queue
            .iter()
            .any(|(line, text)| *line == 9 && text.contains("lacks")),
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Queue.join", "counter-zero"),
        "{result:#?}"
    );
}

#[test]
fn typed_queue_rejections_and_shutdown_keep_independent_effects() {
    let result = check(
        r#"import asyncio as aio

async def run():
    queue = aio.Queue(1)
    try:
        queue.get_nowait()
    except aio.QueueEmpty:
        queue.put_nowait(1)
    try:
        queue.put_nowait(2)
    except aio.QueueFull:
        await queue.get()
    queue.put_nowait(3)
    queue.shutdown(immediate=True)
    try:
        queue.get_nowait()
    except aio.QueueShutDown:
        queue.task_done()
    await queue.join()
"#,
    );
    assert!(findings(&result, "TP7001").is_empty(), "{result:#?}");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Queue.shutdown", "abandon-buffered-item"),
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Queue.join", "counter-zero"),
        "{result:#?}"
    );
}

#[test]
fn cancelling_after_receipt_runs_finally_without_acknowledging_at_acquire() {
    let source = r#"from asyncio import Queue, sleep

async def worker():
    queue = Queue()
    queue.put_nowait(1)
    await queue.get()
    try:
        await sleep(0)
    finally:
        queue.task_done()
"#;
    let result = check(source);
    assert!(findings(&result, "TP7001").is_empty(), "{result:#?}");
    let result = check(&source.replace("queue.task_done()", "pass"));
    assert_eq!(
        findings(&result, "TP7001")
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        [6],
        "{result:#?}"
    );
}

#[test]
fn exact_lock_scopes_and_condition_guards_survive_separate_exit_paths() {
    let result = check(
        r#"from asyncio import Condition, Lock, sleep

async def run():
    lock = Lock()
    alias = lock
    condition = Condition(lock=lock)
    async with alias:
        condition.notify_all()
        await sleep(0)
    try:
        condition.notify()
    except RuntimeError:
        pass

async def debt():
    lock = Lock()
    await lock.acquire()
"#,
    );
    let sync = findings(&result, "TP7002");
    assert!(
        sync.iter()
            .any(|(line, text)| *line == 11 && text.contains("condition")),
        "{result:#?}"
    );
    assert!(
        sync.iter()
            .any(|(line, text)| *line == 17 && text.contains("outstanding")),
        "{result:#?}"
    );
    assert!(
        !sync.iter().any(|(line, _)| *line == 7 || *line == 8),
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Lock", "context-exit"),
        "{result:#?}"
    );
}

#[test]
fn semaphores_have_distinct_over_release_and_event_signals_only_refine_flags() {
    let result = check(
        r#"from asyncio import BoundedSemaphore, Event, Semaphore

async def run():
    ordinary = Semaphore(1)
    ordinary.release()
    await ordinary.acquire()
    await ordinary.acquire()
    ordinary.release()
    ordinary.release()
    bounded = BoundedSemaphore(1)
    try:
        bounded.release()
    except ValueError:
        event = Event()
        event.set()
        if event.is_set():
            await event.wait()
        event.clear()
"#,
    );
    assert_eq!(
        findings(&result, "TP7002")
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        [5, 12],
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(
        model_effect(&result, "asyncio.Event.wait", "wait"),
        "{result:#?}"
    );
}

#[test]
fn fixed_task_group_roles_compose_at_completion_and_fresh_analysis_agrees() {
    let source = r#"from asyncio import Queue, TaskGroup

async def producer(queue):
    await queue.put(7)

async def consumer(queue):
    result = await queue.get()
    queue.task_done()
    return result

async def run():
    queue = Queue(1)
    async with TaskGroup() as group:
        send = group.create_task(producer(queue))
        receive = group.create_task(consumer(queue))
    send.result()
    receive.result()
    await queue.join()
"#;
    let result = check(source);
    assert!(
        result.coverage.reasons.iter().all(|reason| [4, 7, 8]
            .into_iter()
            .any(|line| { reason.starts_with(&format!("async_protocols.py:{line}: ")) })),
        "{result:#?}"
    );
    assert!(findings(&result, "TP7001").is_empty(), "{result:#?}");
    assert!(
        model_effect(&result, "asyncio.Queue.join", "counter-zero"),
        "{result:#?}"
    );

    let mut session = AnalysisSession::default();
    for revision in [
        source.to_owned(),
        source.replace("queue.task_done()", "pass"),
        source.to_owned(),
    ] {
        let updated = session.check_with_foundations(
            &[SourceFile::new(
                "async_protocols.py",
                "async_protocols",
                &revision,
            )],
            "3.13",
            options(),
            inputs(),
        );
        assert_eq!(updated, check(&revision));
    }
}

#[test]
fn unqualified_target_and_known_actor_limit_surface_coverage() {
    let file = SourceFile::new(
        "async_protocols.py",
        "async_protocols",
        "from asyncio import Queue\nasync def run():\n    queue = Queue()\n    queue.shutdown()\n",
    );
    let result = check_with_foundations(&[file], "3.12", options(), inputs());
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result
        .foundations
        .unwrap()
        .model_uses
        .iter()
        .any(|use_site| { use_site.symbol == "asyncio.Queue" && !use_site.applicable }));

    let result = check(
        r#"from asyncio import Queue, TaskGroup

async def worker(queue):
    queue.put_nowait(1)

async def run():
    queue = Queue()
    async with TaskGroup() as group:
        a = group.create_task(worker(queue))
        b = group.create_task(worker(queue))
        c = group.create_task(worker(queue))
        d = group.create_task(worker(queue))
"#,
    );
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("actor budget")),
        "{result:#?}"
    );
}

#[test]
fn primitive_and_message_limits_keep_known_receipts_and_recover_after_an_edit() {
    let first_receipt = "from asyncio import Lock, Queue\nasync def run():\n    first = Queue()\n    first.put_nowait(11)\n    first.get_nowait()\n";
    let mut at_primitive_limit = first_receipt.to_owned();
    for index in 0..7 {
        at_primitive_limit.push_str(&format!("    lock_{index} = Lock()\n"));
    }
    let mut at_message_limit = format!("{first_receipt}    backlog = Queue()\n");
    // The outstanding receipt also counts toward the live-message limit.
    for index in 0..15 {
        at_message_limit.push_str(&format!("    backlog.put_nowait({index})\n"));
    }

    let mut session = AnalysisSession::default();
    for (within_limit, extra, reason) in [
        (
            at_primitive_limit,
            "    overflow = Lock()\n",
            "asyncio protocol primitive budget exceeded",
        ),
        (
            at_message_limit,
            "    backlog.put_nowait(15)\n",
            "asyncio queue live message budget exceeded",
        ),
    ] {
        let over_limit = format!("{within_limit}{extra}");
        let exceeded = check(&over_limit);
        assert_eq!(exceeded.coverage.status, "incomplete", "{exceeded:#?}");
        assert!(exceeded
            .coverage
            .reasons
            .iter()
            .any(|item| item.contains(reason)));
        assert_eq!(
            findings(&exceeded, "TP7001"),
            [(
                5,
                "asyncio queue receipt leaves an outstanding accounting duty"
            )]
        );
        let recovered = check(&within_limit);
        assert_eq!(recovered.coverage.status, "complete-for-supported-subset");
        assert_eq!(findings(&recovered, "TP7001").len(), 1);

        for (source, expected) in [(&over_limit, exceeded), (&within_limit, recovered)] {
            let revised = session.check_with_foundations(
                &[SourceFile::new(
                    "async_protocols.py",
                    "async_protocols",
                    source,
                )],
                "3.13",
                options(),
                inputs(),
            );
            assert_eq!(revised, expected);
        }
    }
}

#[test]
fn blocked_join_observes_zero_before_the_new_admission() {
    let result = check(
        r#"from asyncio import Queue, create_task

async def settle_and_add(queue):
    queue.get_nowait()
    queue.task_done()
    queue.put_nowait(2)

async def run():
    queue = Queue()
    queue.put_nowait(1)
    peer = create_task(settle_and_add(queue))
    await queue.join()
    queue.get_nowait()
    peer.result()
"#,
    );
    assert!(
        model_effect(&result, "asyncio.Queue.join", "counter-zero-notified"),
        "{result:#?}"
    );
    assert!(
        findings(&result, "TP7001")
            .iter()
            .any(|(line, message)| *line == 13 && message.contains("outstanding")),
        "{result:#?}"
    );
}

#[test]
fn a_pulse_wakes_the_blocked_waiter_with_the_flag_clear() {
    let result = check(
        r#"from asyncio import Event, create_task

async def pulse(event):
    event.set()
    event.clear()

async def run():
    event = Event()
    peer = create_task(pulse(event))
    await event.wait()
    if event.is_set():
        unsupported_callback()
    peer.result()
"#,
    );
    assert!(
        model_effect(&result, "asyncio.Event.wait", "signal-observed"),
        "{result:#?}"
    );
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .all(|reason| !reason.starts_with("async_protocols.py:11: ")),
        "{result:#?}"
    );
}

#[test]
fn condition_wait_releases_and_reacquires_the_same_guard() {
    let result = check(
        r#"from asyncio import Condition, Lock, create_task

async def notify(condition):
    async with condition:
        condition.notify()

async def run():
    lock = Lock()
    condition = Condition(lock)
    async with lock:
        peer = create_task(notify(condition))
        try:
            await condition.wait()
        finally:
            condition.notify_all()
    peer.result()
"#,
    );
    assert!(findings(&result, "TP7002").is_empty(), "{result:#?}");
    for effect in [
        "release-for-wait",
        "reacquire-after-wait",
        "signal-observed",
    ] {
        assert!(
            model_effect(&result, "asyncio.Condition.wait", effect),
            "{result:#?}"
        );
    }
}

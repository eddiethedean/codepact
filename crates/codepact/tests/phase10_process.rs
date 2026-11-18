use codepact::{
    check_with_foundations,
    foundation::{FoundationInputs, RuntimeProfile},
    CheckOptions, CheckResult, Ownership, SourceFile,
};

fn inspect(source: &str, launch: Option<&str>) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new("entry.py", "entry", source)],
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
                spawn_main_script: launch.map(str::to_owned),
            }),
            ..FoundationInputs::default()
        },
    )
}

fn model(report: &CheckResult, effect: &str, applicable: bool) -> bool {
    report
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| entry.effect == effect && entry.applicable == applicable)
}

fn has_finding(report: &CheckResult, code: &str, description: &str) -> bool {
    report
        .diagnostics
        .iter()
        .any(|item| item.code == code && item.message.contains(description))
}

#[test]
fn separates_buffered_transport_caller_write_receipt_and_accounting() {
    let report = inspect(
        r#"from multiprocessing import get_context
def run():
    ctx = get_context("spawn")
    buffered = ctx.JoinableQueue()
    buffered.put("one")
    value = buffered.get()
    buffered.task_done()
    buffered.join()
    buffered.close()
    buffered.join_thread()
    simple = ctx.SimpleQueue()
    simple.put(value)
    simple.get()
    simple.close()
"#,
        None,
    );
    for effect in [
        "buffer-admission",
        "receive",
        "item-accounting",
        "accounting-join",
        "close",
        "feeder-completion",
        "write",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    assert!(!has_finding(&report, "TP7003", "remains"), "{report:#?}");
}

#[test]
fn raw_decrement_does_not_acknowledge_unreceived_item() {
    let report = inspect(
        r#"from multiprocessing import get_context
def run():
    ctx = get_context("spawn")
    queue = ctx.JoinableQueue()
    queue.put("one")
    queue.task_done()
    queue.join()
    queue.close()
    queue.join_thread()
"#,
        None,
    );
    assert!(model(&report, "counter-only", true), "{report:#?}");
    assert!(model(&report, "accounting-join", false), "{report:#?}");
    assert!(
        has_finding(&report, "TP7003", "no matching receipt"),
        "{report:#?}"
    );
}

#[test]
fn pipe_directions_and_eof_have_independent_evidence() {
    let report = inspect(
        r#"from multiprocessing import get_context
def receive():
    ctx = get_context("spawn")
    reader, writer = ctx.Pipe(duplex=False)
    alias = writer
    alias.send((17, "item"))
    writer.close()
    reader.recv()
    reader.poll(0)
    try:
        reader.recv()
    except EOFError:
        reader.close()
def bad():
    ctx = get_context("spawn")
    reader, writer = ctx.Pipe(False)
    try:
        reader.send(1)
    except OSError:
        reader.close()
        writer.close()
"#,
        None,
    );
    for effect in ["write", "receive", "ready-eof", "eof", "wrong-direction"] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    assert!(has_finding(&report, "TP7003", "read-only"), "{report:#?}");
}

#[test]
fn exact_wrapper_guard_and_partial_acquisition_refine_the_right_path() {
    let report = inspect(
        r#"from multiprocessing import get_context
def guarded():
    ctx = get_context("spawn")
    first = ctx.RLock()
    second = ctx.Lock()
    value = ctx.Value("i", 0, lock=first)
    array = ctx.Array("i", (1, 2), lock=first)
    same = value.get_lock()
    with second:
        with same:
            acquired = second.acquire(block=False)
            if acquired:
                unknown_branch()
            else:
                value.value += 1
                array[0] += 1
    with first:
        pass
def wrong():
    ctx = get_context("spawn")
    value = ctx.Value("i", 0)
    other = ctx.RLock()
    with other:
        value.value += 1
"#,
        None,
    );
    assert!(
        model(&report, "guarded-compound-update", true),
        "{report:#?}"
    );
    assert!(
        has_finding(&report, "TP7004", "lacks the exact wrapper lock"),
        "{report:#?}"
    );
    assert!(
        !has_finding(&report, "TP7004", "remains acquired"),
        "{report:#?}"
    );
    assert!(
        !report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("unknown_branch")),
        "{report:#?}"
    );
}

#[test]
fn spawn_entry_and_process_worker_results_require_the_declared_script() {
    let source = r#"from multiprocessing import get_context
def worker(queue, reply):
    item = queue.get()
    queue.task_done()
    reply.send(item)
    queue.close()
    reply.close()

if __name__ == "__main__":
    ctx = get_context("spawn")
    incoming = ctx.JoinableQueue()
    reader, writer = ctx.Pipe(False)
    child = ctx.Process(target=worker, args=(incoming, writer))
    child.start()
    writer.close()
    incoming.put("request")
    incoming.join()
    answer = reader.recv()
    child.join()
    reader.close()
    incoming.close()
    incoming.join_thread()
    child.close()
"#;
    let report = inspect(source, Some("entry.py"));
    for effect in [
        "qualified-worker-entry",
        "accounting-join",
        "receive",
        "write",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    let unqualified = inspect(source, Some("different.py"));
    assert!(!model(&unqualified, "qualified-worker-entry", true));
    assert!(
        model(&unqualified, "worker-entry", false),
        "{unqualified:#?}"
    );
}

#[test]
fn distinguishes_start_outside_the_guard_and_local_target_rejection() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker():
    pass
def start():
    ctx = get_context("spawn")
    process = ctx.Process(target=worker)
    process.start()
start()
if __name__ == "__main__":
    ctx = get_context("spawn")
    process = ctx.Process(target=lambda: None)
    try:
        process.start()
    except AttributeError:
        process.close()
"#,
        Some("entry.py"),
    );
    assert!(has_finding(&report, "TP7005", "outside"), "{report:#?}");
    assert!(has_finding(&report, "TP7005", "lambda"), "{report:#?}");
    assert!(
        model(&report, "serialization-rejected-before-launch", true),
        "{report:#?}"
    );
}

#[test]
fn spawn_checks_the_captured_targets_current_module_export() {
    let source = r#"from multiprocessing import get_context
def worker():
    pass

if __name__ == "__main__":
    ctx = get_context("spawn")
    process = ctx.Process(target=worker)
    worker = 0
    process.start()
    process.join()
    process.close()
"#;
    let valid = inspect(&source.replace("    worker = 0\n", ""), Some("entry.py"));
    assert!(model(&valid, "qualified-worker-entry", true), "{valid:#?}");
    assert!(model(&valid, "join", true), "{valid:#?}");

    let conditional = source.replace("    worker = 0", "    if unknown:\n        worker = 0");
    let through_helper = source
        .replace(
            "if __name__",
            "def reset():\n    global worker\n    worker = 0\n\nif __name__",
        )
        .replace(
            "    worker = 0\n    process.start()",
            "    reset()\n    process.start()",
        );
    for source in [source, &conditional, &through_helper] {
        let report = inspect(source, Some("entry.py"));
        assert!(model(&report, "start", false), "{report:#?}");
        assert!(model(&report, "worker-entry", false), "{report:#?}");
        assert!(
            !model(&report, "qualified-worker-entry", true),
            "{report:#?}"
        );
    }
}

#[test]
fn process_join_leaves_worker_completion_unproved_when_receive_blocks() {
    let source = r#"from multiprocessing import get_context
def worker(queue):
    queue.get()
    queue.close()

if __name__ == "__main__":
    ctx = get_context("spawn")
    queue = ctx.SimpleQueue()
    process = ctx.Process(target=worker, args=(queue,))
    process.start()
    process.join()
    process.close()
    queue.close()
    queue.put(1)
"#;
    let report = inspect(source, Some("entry.py"));
    assert!(
        model(&report, "qualified-worker-entry", true),
        "{report:#?}"
    );
    assert!(model(&report, "receive", false), "{report:#?}");
    assert!(model(&report, "join", false), "{report:#?}");
    assert!(!model(&report, "close", true), "{report:#?}");
    assert!(
        !has_finding(&report, "TP7003", "uses a closed local endpoint"),
        "{report:#?}"
    );

    let delivered = source
        .replace("    process.join()", "    queue.put(1)\n    process.join()")
        .replace("    queue.close()\n    queue.put(1)", "    queue.close()");
    let report = inspect(&delivered, Some("entry.py"));
    assert!(model(&report, "receive", true), "{report:#?}");
    assert!(model(&report, "join", true), "{report:#?}");
    assert!(model(&report, "close", true), "{report:#?}");
}

#[test]
fn a_blocked_worker_can_deliver_a_message_sent_before_its_wait() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker(reply, incoming):
    reply.put(1)
    incoming.get()
    incoming.close()
    reply.close()

if __name__ == "__main__":
    ctx = get_context("spawn")
    reply = ctx.SimpleQueue()
    incoming = ctx.SimpleQueue()
    process = ctx.Process(target=worker, args=(reply, incoming))
    process.start()
    reply.get()
    process.join()
    process.close()
    reply.close()
    incoming.close()
"#,
        Some("entry.py"),
    );
    assert!(model(&report, "receive", true), "{report:#?}");
    assert!(model(&report, "receive", false), "{report:#?}");
    assert!(model(&report, "join", false), "{report:#?}");
    assert!(!model(&report, "close", true), "{report:#?}");
}

#[test]
fn feeder_failure_keeps_accounting_while_caller_failure_delivers_nothing() {
    let report = inspect(
        r#"from multiprocessing import get_context
def run():
    ctx = get_context("spawn")
    queue = ctx.JoinableQueue()
    queue.put(lambda: 1)
    queue.join()
    queue.close()
    queue.join_thread()
    simple = ctx.SimpleQueue()
    try:
        simple.put(lambda: 1)
    except AttributeError:
        simple.close()
"#,
        None,
    );
    for effect in [
        "buffer-admission",
        "feeder-serialization-rejected-after-admission",
        "caller-serialization-rejected",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    assert!(model(&report, "accounting-join", false), "{report:#?}");
    assert!(!model(&report, "write", true), "{report:#?}");
    assert!(
        has_finding(&report, "TP7003", "accounting outstanding"),
        "{report:#?}"
    );
}

#[test]
fn fork_guard_is_rejected_before_spawn_launch() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker(guard):
    pass
if __name__ == "__main__":
    fork = get_context("fork")
    spawn = get_context("spawn")
    guard = fork.Lock()
    alias = guard
    process = spawn.Process(target=worker, kwargs={"guard": alias})
    try:
        process.start()
    except RuntimeError:
        process.close()
"#,
        Some("entry.py"),
    );
    assert!(
        model(&report, "context-mismatch-before-launch", true),
        "{report:#?}"
    );
    assert!(
        !model(&report, "qualified-worker-entry", true),
        "{report:#?}"
    );
    assert!(
        has_finding(&report, "TP7005", "fork context"),
        "{report:#?}"
    );
}

#[test]
fn two_live_same_shaped_guard_allocations_leave_review_identity_ambiguous() {
    let report = inspect(
        r#"from multiprocessing import get_context
def run():
    ctx = get_context("spawn")
    lock = ctx.RLock()
    early = lock
    lock = ctx.RLock()
    value = ctx.Value("i", 0, lock=early)
    with early:
        value.value += 1
"#,
        None,
    );
    assert!(
        model(&report, "guarded-compound-update", true),
        "{report:#?}"
    );
    assert!(
        !has_finding(&report, "TP7004", "lacks the exact wrapper lock"),
        "{report:#?}"
    );
    assert!(
        report.coverage.reasons.iter().any(|reason| {
            reason.contains("allocation anchors do not uniquely identify this process guard")
        }),
        "{report:#?}"
    );
}

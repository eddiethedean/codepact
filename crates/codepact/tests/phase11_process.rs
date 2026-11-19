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
fn shared_memory_tracks_partial_close_and_release_retry() {
    let report = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory
def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    base = memory.buf
    view = base[:1]
    alias = view
    try:
        memory.close()
    except BufferError:
        empty_property = memory.buf
        try:
            invalid = base[0]
        except ValueError:
            pass
        alias.release()
        memory.close()
    memory.unlink()
"#,
        None,
    );
    for effect in [
        "create-shared-memory-track-name",
        "bind-base-buffer-view",
        "export-buffer-view",
        "base-released-mapping-close-rejected",
        "buffer-property-after-close",
        "release-buffer-view",
        "close-local-mapping",
        "unlink-name-request-tracker-unregistration",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    assert!(has_finding(
        &report,
        "TP7003",
        "while an exported view is live"
    ));
    assert!(!has_finding(&report, "TP7003", "remains"), "{report:#?}");
}

#[test]
fn shared_memory_unlink_and_mapping_release_have_independent_duties() {
    let report = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory
def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    name = memory.name
    memory.unlink()
    memory.buf[0] = 42
    try:
        receiver = SharedMemory(name)
    except FileNotFoundError:
        memory.close()
"#,
        None,
    );
    assert!(model(&report, "access-buffer-view", true), "{report:#?}");
    assert!(
        model(&report, "attachment-rejected-name-removed", true),
        "{report:#?}"
    );
    assert!(has_finding(&report, "TP7003", "attachment follows removal"));
    assert!(!has_finding(&report, "TP7003", "remains"), "{report:#?}");

    let missing = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory
def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    memory.close()
"#,
        None,
    );
    assert!(has_finding(&missing, "TP7003", "name remains linked"));
    assert!(!has_finding(
        &missing,
        "TP7003",
        "mapping remains locally open"
    ));
}

#[test]
fn failed_attachment_keeps_mapping_facts_and_weakens_name_evidence() {
    let report = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory
def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    try:
        child = SharedMemory(memory.name)
    except OSError:
        memory.buf[0] = 42
    else:
        child.close()
    memory.close()
    try:
        memory.unlink()
    except FileNotFoundError:
        pass
"#,
        None,
    );
    assert!(model(&report, "attach-local-mapping", true), "{report:#?}");
    assert!(
        model(&report, "attachment-mapping-failure-may-unlink", true),
        "{report:#?}"
    );
    assert!(
        model(&report, "unlink-may-find-name-already-removed", true),
        "{report:#?}"
    );
    assert!(model(&report, "access-buffer-view", true), "{report:#?}");
    assert!(
        !has_finding(&report, "TP7003", "mapping remains locally open"),
        "{report:#?}"
    );
}

#[test]
fn spawned_attachment_uses_the_exact_name_and_local_close() {
    let report = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory
def worker(name):
    try:
        local = SharedMemory(name)
    except OSError:
        return
    local.close()
if __name__ == "__main__":
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    child = ctx.Process(target=worker, args=(memory.name,))
    child.start()
    child.join()
    child.close()
    memory.close()
    try:
        memory.unlink()
    except FileNotFoundError:
        pass
"#,
        Some("entry.py"),
    );
    assert!(
        model(&report, "qualified-worker-entry", true),
        "{report:#?}"
    );
    assert!(model(&report, "attach-local-mapping", true), "{report:#?}");
    assert!(model(&report, "close-local-mapping", true), "{report:#?}");
}

#[test]
fn first_workers_reply_survives_second_prelaunch_rejection() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker(sender):
    sender.send(42)
    sender.close()
if __name__ == "__main__":
    ctx = get_context("spawn")
    recv, sender = ctx.Pipe(duplex=False)
    first = ctx.Process(target=worker, args=(sender,))
    second = ctx.Process(target=lambda: None)
    first.start()
    try:
        second.start()
    except AttributeError:
        reply = recv.recv()
        first.join()
        first.close()
    recv.close()
    sender.close()
"#,
        Some("entry.py"),
    );
    for effect in [
        "qualified-worker-entry",
        "serialization-rejected-before-launch",
        "receive",
        "join",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
}

#[test]
fn stop_request_preserves_possible_live_child_and_ipc() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker(queue):
    value = queue.get()
    queue.task_done()
    queue.close()
if __name__ == "__main__":
    ctx = get_context("spawn")
    queue = ctx.JoinableQueue()
    child = ctx.Process(target=worker, args=(queue,))
    child.start()
    queue.put("ticket")
    child.terminate()
    child.join(0)
    child.kill()
    child.join()
    child.close()
    queue.close()
    queue.join_thread()
    queue.join()
"#,
        Some("entry.py"),
    );
    for effect in [
        "buffer-admission",
        "terminate-requested",
        "kill-requested",
        "stop-request-reaped",
        "close",
    ] {
        assert!(model(&report, effect, true), "{effect}: {report:#?}");
    }
    assert!(
        model(&report, "worker-effects-after-stop-request", false),
        "{report:#?}"
    );
    assert!(model(&report, "accounting-join", false), "{report:#?}");
    assert!(!model(&report, "item-accounting", true), "{report:#?}");
}

#[test]
fn worker_exit_does_not_observe_result_or_exception() {
    let report = inspect(
        r#"from multiprocessing import get_context
def worker(sender):
    sender.send(42)
    sender.close()
    raise RuntimeError
if __name__ == "__main__":
    ctx = get_context("spawn")
    recv, sender = ctx.Pipe(duplex=False)
    child = ctx.Process(target=worker, args=(sender,))
    child.start()
    child.join()
    child.close()
    recv.close()
    sender.close()
"#,
        Some("entry.py"),
    );
    assert!(model(&report, "join", true), "{report:#?}");
    assert!(
        model(&report, "worker-exception-exit-unobserved", true),
        "{report:#?}"
    );
    assert!(has_finding(
        &report,
        "TP7003",
        "worker reply remains unobserved"
    ));
}

#[test]
fn shared_view_bounds_preserve_exceptional_cleanup_duties() {
    let source = r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory

def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    try:
        mark = memory.buf[1024]
    except IndexError:
        return
    memory.close()
    memory.unlink()
"#;
    let report = inspect(source, None);
    assert!(
        model(&report, "reject-out-of-range-buffer-index", true),
        "{report:#?}"
    );
    assert!(
        has_finding(&report, "TP7003", "mapping remains locally open"),
        "{report:#?}"
    );

    let report = inspect(
        r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory

def run():
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    view = memory.buf[1:7:2]
    reverse = view[::-1]
    reverse[2] = 42
    try:
        reverse[3] = 7
    except IndexError:
        reverse.release()
        view.release()
        memory.close()
        memory.unlink()
"#,
        None,
    );
    assert!(model(&report, "access-buffer-view", true), "{report:#?}");
    assert!(
        model(&report, "reject-out-of-range-buffer-index", true),
        "{report:#?}"
    );
    assert!(
        !has_finding(&report, "TP7003", "mapping remains locally open"),
        "{report:#?}"
    );
}

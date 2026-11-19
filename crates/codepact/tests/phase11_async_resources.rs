use codepact::{
    check_with_foundations,
    foundation::{FoundationInputs, RuntimeProfile},
    CheckOptions, CheckResult, Ownership, SourceFile,
};

fn check(source: &str) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new("app.py", "app", source)],
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
                spawn_main_script: None,
            }),
            ..FoundationInputs::default()
        },
    )
}

fn has_effect(report: &CheckResult, symbol: &str, effect: &str) -> bool {
    report
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|usage| usage.symbol == symbol && usage.effect == effect && usage.applicable)
}

const GENERATOR: &str = r#"from contextlib import aclosing

async def records():
    handle = open("records.txt", "r")
    try:
        yield 1
    finally:
        handle.close()

async def use():
"#;

#[test]
fn closing_an_unstarted_generator_does_not_acquire_its_body_resource() {
    let report = check(&format!(
        "{GENERATOR}    generator = records()\n    await generator.aclose()\n"
    ));
    assert!(
        has_effect(&report, "async_generator", "finalize-frame"),
        "{report:#?}"
    );
    assert!(
        !has_effect(&report, "builtins.open", "fresh-owned-return"),
        "{report:#?}"
    );
    assert!(report.diagnostics.is_empty(), "{report:#?}");
}

#[test]
fn explicit_close_and_aclosing_finalize_after_a_suspended_yield() {
    for body in [
        "    generator = records()\n    await anext(generator)\n    await generator.aclose()\n",
        "    async with aclosing(records()) as generator:\n        async for item in generator:\n            break\n",
        "    async for item in records():\n        pass\n",
    ] {
        let report = check(&format!("{GENERATOR}{body}"));
        assert!(has_effect(&report, "builtins.open", "fresh-owned-return"), "{report:#?}");
        assert!(!report.diagnostics.iter().any(|d| {
            d.message.contains("abandoned") || (d.code == "TP1002" && d.certainty == "definite")
        }), "{report:#?}");
    }
}

#[test]
fn early_break_and_unawaited_close_leave_the_generator_suspended() {
    for body in [
        "    generator = records()\n    async for item in generator:\n        break\n",
        "    generator = records()\n    await anext(generator)\n    generator.aclose()\n",
    ] {
        let report = check(&format!("{GENERATOR}{body}"));
        assert!(
            report
                .coverage
                .reasons
                .iter()
                .any(|reason| reason.contains("abandoned async generator")),
            "{report:#?}"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "TP1002"),
            "{report:#?}"
        );
    }
}

#[test]
fn interrupted_generator_cleanup_cannot_be_retried_through_a_closed_frame() {
    let report = check(
        r#"from asyncio import sleep

async def generator():
    handle = open("records.txt", "r")
    try:
        yield 1
    finally:
        await sleep(0)
        handle.close()

async def use():
    frame = generator()
    await anext(frame)
    try:
        await frame.aclose()
    except BaseException:
        await frame.aclose()
"#,
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == "TP1002"),
        "{report:#?}"
    );
    assert!(
        !report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("abandoned async generator")),
        "{report:#?}"
    );
}

#[test]
fn a_never_started_generator_still_captures_an_existing_resource() {
    let source = r#"async def record(handle):
    handle.read()
    yield 1

async def use():
    handle = open("records.txt", "r")
    iterator = record(handle)
    handle.close()
    await iterator.aclose()
"#;
    let report = check(source);
    assert!(
        report.diagnostics.iter().any(|d| d.code == "TP2006"),
        "{report:#?}"
    );
    let report = check(&source.replace(
        "    handle.close()\n    await iterator.aclose()",
        "    await iterator.aclose()\n    handle.close()",
    ));
    assert!(
        !report.diagnostics.iter().any(|d| d.code == "TP2006"),
        "{report:#?}"
    );
}

#[test]
fn source_resolved_async_context_propagates_body_errors_and_finalizes() {
    let report = check(
        r#"import contextlib as cx

@cx.asynccontextmanager
async def manager():
    handle = open("records.txt", "r")
    try:
        yield handle
    finally:
        handle.close()

async def use():
    try:
        async with manager() as handle:
            raise ValueError()
    except ValueError:
        handle.read()
"#,
    );
    assert!(
        has_effect(
            &report,
            "contextlib.asynccontextmanager",
            "construct-lazy-frame"
        ),
        "{report:#?}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "TP1001" && d.primary.line == 16),
        "{report:#?}"
    );
}

#[test]
fn stream_writer_close_and_wait_are_distinct() {
    let report = check(
        r#"import asyncio as aio

async def exchange():
    reader, writer = await aio.open_connection("127.0.0.1", 8000)
    alias = writer
    alias.write(b"job")
    try:
        await writer.drain()
    finally:
        alias.close()
        await writer.wait_closed()
"#,
    );
    for (symbol, effect) in [
        ("asyncio.open_connection", "acquire-stream-pair"),
        ("asyncio.StreamWriter.write", "buffer-local-write"),
        ("asyncio.StreamWriter.drain", "wait-for-flow-control"),
        ("asyncio.StreamWriter.close", "request-close"),
        ("asyncio.StreamWriter.wait_closed", "observe-closure"),
    ] {
        assert!(has_effect(&report, symbol, effect), "{report:#?}");
    }
    let report = check(
        r#"import asyncio

async def exchange():
    reader, writer = await asyncio.open_connection("127.0.0.1", 8000)
    writer.close()
"#,
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "TP1002" && d.certainty == "definite"),
        "{report:#?}"
    );
}

#[test]
fn accepted_transport_shutdown_does_not_complete_the_handler_or_local_writer() {
    let report = check(
        r#"import asyncio

async def handle(reader, writer):
    pass

async def exchange():
    server = await asyncio.start_server(handle, "127.0.0.1", 8000)
    reader, writer = await asyncio.open_connection("127.0.0.1", 8000)
    server.close()
    server.abort_clients()
    await server.wait_closed()
"#,
    );
    for (symbol, effect) in [
        ("asyncio.start_server", "acquire-local-handle"),
        ("asyncio.Server.close", "request-close"),
        ("asyncio.Server.abort_clients", "request-client-abort"),
        ("asyncio.Server.wait_closed", "observe-closure"),
    ] {
        assert!(has_effect(&report, symbol, effect), "{report:#?}");
    }
    assert!(
        report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("client-handler completion")),
        "{report:#?}"
    );
    let operation = report
        .diagnostics
        .iter()
        .find(|d| d.code == "TP1002" && d.related.iter().any(|span| span.line == 8));
    assert!(operation.is_some(), "{report:#?}");
    assert!(
        report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("interrupted after listening starts")),
        "{report:#?}"
    );
}

#[test]
fn starting_an_already_owned_listener_retains_the_handle_on_cancellation() {
    let report = check(
        r#"import asyncio

async def handle(reader, writer):
    pass

async def serve():
    server = await asyncio.start_server(handle, "127.0.0.1", 8000, start_serving=False)
    try:
        await server.start_serving()
    finally:
        server.close()
        await server.wait_closed()
"#,
    );
    assert!(has_effect(
        &report,
        "asyncio.Server.start_serving",
        "start-listening"
    ));
    assert!(has_effect(
        &report,
        "asyncio.Server.wait_closed",
        "observe-closure"
    ));
    assert!(
        !report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("before the handle is delivered")),
        "{report:#?}"
    );
}

#[test]
fn a_stop_request_and_wait_with_undrained_pipes_preserve_process_duties() {
    let report = check(
        r#"import asyncio as aio

async def launch():
    first = await aio.create_subprocess_exec("/usr/bin/true")
    first.terminate()
    second = await aio.create_subprocess_exec("/usr/bin/true", stdout=aio.PIPE)
    await second.wait()
"#,
    );
    assert!(
        has_effect(
            &report,
            "asyncio.subprocess.Process.terminate",
            "request-process-stop"
        ),
        "{report:#?}"
    );
    assert!(
        report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("undrained output")),
        "{report:#?}"
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == "TP1002"),
        "{report:#?}"
    );
    let report = check(
        r#"from asyncio import PIPE, create_subprocess_exec as execute

async def launch():
    child = await execute("/usr/bin/true", stdout=PIPE)
    stdout, stderr = await child.communicate()
"#,
    );
    assert!(
        has_effect(
            &report,
            "asyncio.subprocess.Process.communicate",
            "collect-pipes-and-observe-exit"
        ),
        "{report:#?}"
    );
}

#[test]
fn stream_writer_retries_reuse_the_cancelled_close_waiter() {
    let report = check(
        r#"from asyncio import CancelledError, open_connection

async def run():
    reader, writer = await open_connection("127.0.0.1", 9000)
    writer.close()
    try:
        await writer.wait_closed()
    except CancelledError:
        await writer.wait_closed()
"#,
    );
    assert!(
        has_effect(
            &report,
            "asyncio.StreamWriter.wait_closed",
            "cancelled-close-waiter"
        ),
        "{report:#?}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP1002" && finding.primary.line == 9),
        "{report:#?}"
    );
}

#[test]
fn resolved_asynccontextmanager_decorators_have_complete_coverage() {
    for (import, decorator) in [
        (
            "from contextlib import asynccontextmanager",
            "asynccontextmanager",
        ),
        (
            "from contextlib import asynccontextmanager as manager",
            "manager",
        ),
        (
            "import contextlib as contexts",
            "contexts.asynccontextmanager",
        ),
    ] {
        let report = check(&format!(
            "{import}\n@{decorator}\nasync def resource():\n    yield 1\n\nasync def run():\n    async with resource():\n        pass\n"
        ));
        assert!(
            has_effect(
                &report,
                "contextlib.asynccontextmanager",
                "construct-lazy-frame"
            ),
            "{report:#?}"
        );
        assert_eq!(
            report.coverage.status, "complete-for-supported-subset",
            "{report:#?}"
        );
    }
}

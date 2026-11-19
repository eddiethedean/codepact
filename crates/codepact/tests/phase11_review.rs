use codepact::{
    adoption::{resolve_config, EffectiveConfig, Overrides},
    foundation::{FoundationInputs, RuntimeProfile},
    review::{capture, compare, ReviewSnapshot, ReviewVerdict},
    AnalysisSession, CheckOptions, Ownership, SourceFile,
};
use std::collections::BTreeSet;

const BRIDGES: &str = r#"from asyncio import wrap_future
from concurrent.futures import ThreadPoolExecutor

def worker(value):
    return value

async def run():
    with ThreadPoolExecutor(1) as pool:
        first = pool.submit(worker, 1)
        try:
            second = pool.submit(worker, 2)
            try:
                selected = first
                await wrap_future(selected)
            finally:
                second.result()
        finally:
            first.result()
"#;

fn config() -> EffectiveConfig {
    let mut config = resolve_config(
        &std::env::current_dir().unwrap(),
        &Overrides {
            no_config: true,
            python_version: Some("3.13".into()),
            experimental_ownership: Some(true),
            ..Overrides::default()
        },
    )
    .unwrap();
    config.ownership = Ownership::AdvancedAssumeDefaultTaskFactory;
    config.runtime_profile = Some(RuntimeProfile {
        implementation: "cpython".into(),
        python_version: "3.13.16".into(),
        os: "linux".into(),
        architecture: "x86_64".into(),
        event_loop: Some("default".into()),
        start_method: Some("spawn".into()),
        assume_standard_process_wait: true,
        spawn_main_script: Some("main.py".into()),
    });
    config
}

fn snapshot(
    session: &mut AnalysisSession,
    config: &EffectiveConfig,
    source: &str,
    unrelated: &str,
) -> ReviewSnapshot {
    let files = [
        SourceFile::new("main.py", "main", source),
        SourceFile::new("extra.py", "extra", unrelated),
    ];
    let options = CheckOptions {
        ownership: config.ownership,
        ..CheckOptions::default()
    };
    let inputs = FoundationInputs {
        runtime: config.runtime_profile.clone(),
        ..FoundationInputs::default()
    };
    capture(session, &files, config, options, Some(&inputs), None).unwrap()
}

#[test]
fn a_matched_bridge_site_retains_changes_to_the_selected_work() {
    let config = config();
    let mut session = AnalysisSession::default();
    let before = snapshot(&mut session, &config, BRIDGES, "value = 7\n");
    let modified = BRIDGES.replace("selected = first", "selected = second");
    let after = snapshot(&mut session, &config, &modified, "value = 7\n");
    assert!(before.findings.is_empty(), "{before:#?}");
    assert!(after.findings.is_empty(), "{after:#?}");
    let review = compare(&before, &after, &BTreeSet::new(), None).unwrap();
    assert_eq!(review.verdict, ReviewVerdict::ReviewRequired, "{review:#?}");
}

#[test]
fn runtime_edit_invalidates_bridge_evidence_and_reuses_unaffected_files() {
    let config = config();
    let mut session = AnalysisSession::default();
    let original = snapshot(&mut session, &config, BRIDGES, "value = 7\n");
    assert_eq!(
        original,
        snapshot(&mut session, &config, BRIDGES, "value = 7\n")
    );

    let unrelated = snapshot(&mut session, &config, BRIDGES, "value = 9\n");
    assert_eq!(
        unrelated,
        snapshot(
            &mut AnalysisSession::default(),
            &config,
            BRIDGES,
            "value = 9\n"
        )
    );
    assert_eq!(original.coverage, unrelated.coverage);
    assert_eq!(
        original.foundations.as_ref().unwrap().model_uses,
        unrelated.foundations.as_ref().unwrap().model_uses
    );

    let mut alternate = config.clone();
    alternate.runtime_profile.as_mut().unwrap().architecture = "aarch64".into();
    let unsupported = snapshot(&mut session, &alternate, BRIDGES, "value = 9\n");
    assert_eq!(
        unsupported,
        snapshot(
            &mut AnalysisSession::default(),
            &alternate,
            BRIDGES,
            "value = 9\n"
        )
    );
    assert_eq!(unsupported.coverage.status, "incomplete");
    assert!(unsupported
        .foundations
        .unwrap()
        .model_uses
        .iter()
        .all(|usage| usage.symbol != "asyncio.wrap_future" || !usage.applicable));

    assert_eq!(
        unrelated,
        snapshot(&mut session, &config, BRIDGES, "value = 9\n")
    );
}

#[test]
fn bridge_cancellation_keeps_queue_accounting_observation_and_acknowledgment_distinct() {
    let source = r#"from asyncio import Queue, wrap_future
from concurrent.futures import ThreadPoolExecutor
from codepact import MustHandle, Ok, Result

def worker(handle) -> MustHandle[Result[int, str]]:
    return Ok(1)

async def consume():
    queue = Queue(1)
    await queue.put(1)
    await queue.get()
    with open("example.txt") as handle:
        with ThreadPoolExecutor(1) as pool:
            future = pool.submit(worker, handle)
            if future.running():
                wrapper = wrap_future(future)
                wrapper.cancel()
                handle.close()
            value = future.result()
"#;
    let mut config = config();
    config.enabled_rules.insert("TP7001".into());
    config.enabled_rules.insert("TP3003".into());
    let report = snapshot(&mut AnalysisSession::default(), &config, source, "pass\n");
    for code in ["TP2006", "TP5002", "TP7001"] {
        assert!(
            report.findings.iter().any(|finding| finding.code == code),
            "expected {code}: {:?}",
            report.findings
        );
    }
    let uses = &report.foundations.unwrap().model_uses;
    assert!(uses.iter().any(|usage| {
        usage.applicable && usage.symbol == "concurrent.futures.Future" && usage.effect == "result"
    }));
}

#[test]
fn forced_worker_stop_keeps_ipc_and_local_shared_memory_duties_distinct() {
    let source = r#"from multiprocessing import get_context
from multiprocessing.shared_memory import SharedMemory

def worker(name, sender):
    try:
        local = SharedMemory(name)
    except OSError:
        return
    try:
        sender.send(1)
    finally:
        local.close()
        sender.close()

if __name__ == "__main__":
    ctx = get_context("spawn")
    memory = SharedMemory(create=True, size=8)
    reader, sender = ctx.Pipe(False)
    child = ctx.Process(target=worker, args=(memory.name, sender))
    child.start()
    child.terminate()
    child.join()
    child.close()
    memory.close()
    try:
        memory.unlink()
    except FileNotFoundError:
        pass
"#;
    let mut config = config();
    config.enabled_rules.insert("TP7003".into());
    let report = snapshot(&mut AnalysisSession::default(), &config, source, "pass\n");
    let uses = &report.foundations.as_ref().unwrap().model_uses;
    assert!(
        uses.iter().any(|usage| {
            usage.applicable
                && usage.symbol.contains("Process")
                && usage.effect.contains("terminate")
        }),
        "{uses:?}"
    );
    assert!(uses
        .iter()
        .any(|usage| { usage.applicable && usage.effect == "close-local-mapping" }));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == "TP7003"));
}

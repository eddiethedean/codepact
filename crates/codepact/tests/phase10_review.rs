use codepact::{
    adoption::{resolve_config, Overrides},
    foundation::{AsyncGuardModel, FoundationInputs, RuntimeProfile},
    review::{capture, compare, ReviewSnapshot, ReviewVerdict},
    AnalysisSession, CheckOptions, SourceFile,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CONFIG: &str = r#"[tool.codepact]
config-version = 1
python-version = "3.13"
experimental-ownership = true
experimental-advanced-contracts = true
assume-default-task-factory = true

[tool.codepact.runtime-profile]
implementation = "cpython"
python-version = "3.13.16"
os = "linux"
architecture = "x86_64"
event-loop = "default"
start-method = "spawn"
spawn-main-script = "main.py"
assume-standard-process-wait = true

[[tool.codepact.async-guards]]
module = "main"
coroutine = "update"
resource = "counter"
lock = "first"
method = "increment"
"#;

const SOURCE: &str = r#"from asyncio import Lock
from codepact import resource

@resource(closed_by="close")
class Counter:
    def increment(self): pass
    def close(self): pass

async def update():
    first = Lock()
    second = Lock()
    counter = Counter()
    alias = counter
    try:
        async with first:
            alias.increment()
    finally:
        counter.close()
"#;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "codepact-phase10-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("pyproject.toml"), CONFIG).unwrap();
        Self(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(
    session: &mut AnalysisSession,
    config: &codepact::adoption::EffectiveConfig,
    source: &str,
    lock: &str,
) -> ReviewSnapshot {
    let mut guard: AsyncGuardModel = config.async_guards[0].clone();
    guard.lock = lock.into();
    let mut config = config.clone();
    config.async_guards = vec![guard.clone()];
    let runtime: RuntimeProfile = config.runtime_profile.clone().unwrap();
    let foundations = FoundationInputs {
        runtime: Some(runtime),
        async_guards: vec![guard],
        ..FoundationInputs::default()
    };
    capture(
        session,
        &[SourceFile::new("main.py", "main", source)],
        &config,
        CheckOptions {
            ownership: config.ownership,
            ..CheckOptions::default()
        },
        Some(&foundations),
        None,
    )
    .unwrap()
}

#[test]
fn guard_change_matches_the_clean_operation_and_preserves_runtime_provenance() {
    let project = Project::new();
    let config = resolve_config(&project.0, &Overrides::default()).unwrap();
    let mut reused = AnalysisSession::default();
    let before = snapshot(&mut reused, &config, SOURCE, "first");
    let changed = SOURCE.replace("async with first:", "async with second:");
    let after = snapshot(&mut reused, &config, &changed, "second");
    assert_eq!(
        after,
        snapshot(&mut AnalysisSession::default(), &config, &changed, "second")
    );
    assert!(before.findings.is_empty(), "{:#?}", before.findings);
    assert!(after.findings.is_empty(), "{:#?}", after.findings);
    let guard_sites = |capture: &ReviewSnapshot| {
        capture
            .protocol_contracts
            .iter()
            .filter(|site| site.family == "asyncio.guarded-access")
            .map(|site| site.identity.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(guard_sites(&before).len(), 1);
    assert_eq!(guard_sites(&before), guard_sites(&after));

    let report = compare(&before, &after, &BTreeSet::new(), None).unwrap();
    assert_eq!(report.verdict, ReviewVerdict::ReviewRequired, "{report:#?}");
    assert!(
        report.reasons.iter().any(|reason| {
            reason.category == "contracts"
                && reason.detail.contains("first:")
                && reason.detail.contains("second:")
        }),
        "{report:#?}"
    );
    let runtime = after
        .foundations
        .as_ref()
        .unwrap()
        .runtime
        .as_ref()
        .unwrap();
    assert_eq!(runtime.spawn_main_script.as_deref(), Some("main.py"));
}

#[test]
fn cli_reports_the_wrong_guard_and_an_exact_acquisition_repairs_it() {
    let project = Project::new();
    let config = resolve_config(&project.0, &Overrides::default()).unwrap();
    let wrong = SOURCE.replace("async with first:", "async with second:");
    fs::write(project.0.join("main.py"), &wrong).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(&project.0)
        .args(["check", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        run.status.code(),
        Some(1),
        "{:?}",
        String::from_utf8_lossy(&run.stderr)
    );
    let cli: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        cli["foundations"]["runtime"]["spawn-main-script"],
        "main.py"
    );
    assert!(
        cli["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "TP7002"),
        "{cli:#}"
    );

    let mut reused = AnalysisSession::default();
    let before = snapshot(&mut reused, &config, &wrong, "first");
    let after = snapshot(&mut reused, &config, SOURCE, "first");
    let fingerprint = before
        .findings
        .iter()
        .find(|finding| finding.code == "TP7002")
        .expect("missing guard finding")
        .fingerprint
        .clone();
    let report = compare(&before, &after, &BTreeSet::from([fingerprint]), None).unwrap();
    assert_eq!(report.verdict, ReviewVerdict::Passed, "{report:#?}");
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.classification == "resolved-under-preserved-contracts"));
}

#[test]
fn acknowledgment_repair_preserves_the_receipt_contract() {
    let project = Project::new();
    let mut config = resolve_config(&project.0, &Overrides::default()).unwrap();
    config.async_guards.clear();
    let options = CheckOptions {
        ownership: config.ownership,
        ..CheckOptions::default()
    };
    let inputs = FoundationInputs {
        runtime: config.runtime_profile.clone(),
        ..FoundationInputs::default()
    };
    let source = "from asyncio import Queue\nasync def consume():\n    queue = Queue()\n    queue.put_nowait(7)\n    queue.get_nowait()\n";
    let mut reused = AnalysisSession::default();
    let run = |session: &mut AnalysisSession, source| {
        capture(
            session,
            &[SourceFile::new("main.py", "main", source)],
            &config,
            options.clone(),
            Some(&inputs),
            None,
        )
        .unwrap()
    };
    let before = run(&mut reused, source);
    let fixed = format!("{source}    queue.task_done()\n");
    let after = run(&mut reused, &fixed);
    let finding = before
        .findings
        .iter()
        .find(|finding| finding.code == "TP7001")
        .expect("one missing acknowledgment");
    assert_eq!(before.protocol_contracts, after.protocol_contracts);
    let review = compare(
        &before,
        &after,
        &BTreeSet::from([finding.fingerprint.clone()]),
        None,
    )
    .unwrap();
    assert_eq!(review.verdict, ReviewVerdict::Passed, "{review:#?}");
}

#[test]
fn same_line_process_locks_remain_distinct_in_clean_review() {
    let project = Project::new();
    let mut config = resolve_config(&project.0, &Overrides::default()).unwrap();
    config.async_guards.clear();
    let options = CheckOptions {
        ownership: config.ownership,
        ..CheckOptions::default()
    };
    let inputs = FoundationInputs {
        runtime: config.runtime_profile.clone(),
        ..FoundationInputs::default()
    };
    let source = "from multiprocessing import get_context\ndef increment():\n    ctx = get_context('spawn')\n    first = ctx.RLock(); second = ctx.RLock()\n    value = ctx.Value('i', 0, lock=first)\n    with first:\n        value.value += 1\n";
    let mut session = AnalysisSession::default();
    let run = |session: &mut AnalysisSession, source| {
        capture(
            session,
            &[SourceFile::new("main.py", "main", source)],
            &config,
            options.clone(),
            Some(&inputs),
            None,
        )
        .unwrap()
    };
    let before = run(&mut session, source);
    let changed = source
        .replace("lock=first", "lock=second")
        .replace("with first:", "with second:");
    let after = run(&mut session, &changed);
    assert!(before.findings.is_empty(), "{:#?}", before.findings);
    assert!(after.findings.is_empty(), "{:#?}", after.findings);
    let review = compare(&before, &after, &BTreeSet::new(), None).unwrap();
    assert_eq!(review.verdict, ReviewVerdict::ReviewRequired, "{review:#?}");
    assert!(review.reasons.iter().any(|reason| {
        reason.category == "contracts" && reason.detail.contains("process-guarded-compound-update")
    }));
}

#[test]
fn stale_guard_selectors_fail_strict_checks() {
    let project = Project::new();
    fs::write(
        project.0.join("main.py"),
        SOURCE.replace("async with first:", "async with second:"),
    )
    .unwrap();
    for (live, stale) in [
        ("module = \"main\"", "module = \"missing\""),
        ("coroutine = \"update\"", "coroutine = \"missing\""),
        ("method = \"increment\"", "method = \"missing\""),
    ] {
        fs::write(
            project.0.join("pyproject.toml"),
            CONFIG.replace(live, stale),
        )
        .unwrap();
        let run = Command::new(env!("CARGO_BIN_EXE_codepact"))
            .current_dir(&project.0)
            .args(["check", "--mode", "strict", "--json"])
            .output()
            .unwrap();
        assert_eq!(
            run.status.code(),
            Some(1),
            "{stale}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(report["coverage"]["status"], "incomplete", "{stale}");
    }
}

#[test]
fn rust_review_rejects_missing_configured_guards() {
    let project = Project::new();
    let config = resolve_config(&project.0, &Overrides::default()).unwrap();
    let error = capture(
        &mut AnalysisSession::default(),
        &[SourceFile::new("main.py", "main", SOURCE)],
        &config,
        CheckOptions {
            ownership: config.ownership,
            ..CheckOptions::default()
        },
        None,
        None,
    )
    .unwrap_err();
    assert!(error.contains("matching analysis settings"));
}

#[test]
fn async_guard_applies_when_a_lazy_method_runs() {
    let project = Project::new();
    let config = resolve_config(&project.0, &Overrides::default()).unwrap();
    let mut session = AnalysisSession::default();
    let guarded = SOURCE
        .replace("def increment(", "async def increment(")
        .replace(
        "        async with first:\n            alias.increment()",
        "        pending = alias.increment()\n        async with first:\n            await pending",
    );
    let safe = snapshot(&mut session, &config, &guarded, "first");
    assert!(
        !safe.findings.iter().any(|finding| finding.code == "TP7002"),
        "{:#?}",
        safe.findings
    );
    assert!(safe
        .protocol_contracts
        .iter()
        .any(|contract| contract.family == "asyncio.guarded-access"));

    let unguarded = guarded.replace("async with first:", "async with second:");
    let unsafe_access = snapshot(&mut session, &config, &unguarded, "first");
    assert!(
        unsafe_access
            .findings
            .iter()
            .any(|finding| finding.code == "TP7002"),
        "{:#?}",
        unsafe_access.findings
    );
}

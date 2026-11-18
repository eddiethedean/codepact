use codepact::{
    check_with_foundations,
    foundation::{
        export_manifest, ExceptionalClose, FoundationInputs, ProjectResourceModel, ProviderPackage,
        RuntimeProfile,
    },
    AnalysisSession, CheckOptions, CheckResult, Ownership, SourceFile,
};

const TARGET: &str = "3.13";
const IMPORTS: &str = "from vendor import acquire, consume, fetch, hidden\n";
const PROVIDER: &str = r#"from __future__ import annotations
from codepact import must_use, resource
from codepact.rust import Owned

@resource(closed_by="close")
class Channel:
    def close(self): pass
    def read(self): return "data"

def acquire() -> "Owned[Channel]":
    return Channel()

def consume(value: "Owned[Channel]", fail=False):
    if fail:
        raise ValueError("interrupt")
    value.close()

@must_use
def fetch(): return 1
"#;

fn package() -> ProviderPackage {
    ProviderPackage {
        distribution: "test-vendor".into(),
        version: "1.2".into(),
        import_scope: "vendor".into(),
        sources: vec![
            SourceFile::new(
                "vendor/__init__.py",
                "vendor",
                "from ._implementation import Channel as Channel, acquire as acquire, consume as consume, fetch as fetch, fetch as hidden\n",
            ),
            SourceFile::new("vendor/_implementation.py", "vendor._implementation", PROVIDER),
        ],
        marked: true,
        manifest: None,
    }
}

fn runtime() -> RuntimeProfile {
    RuntimeProfile {
        implementation: "cpython".into(),
        python_version: "3.13.16".into(),
        os: "linux".into(),
        architecture: "x86_64".into(),
        event_loop: Some("default".into()),
        start_method: Some("spawn".into()),
        assume_standard_process_wait: true,
        spawn_main_script: None,
    }
}

fn options() -> CheckOptions {
    CheckOptions {
        ownership: Ownership::AdvancedAssumeDefaultTaskFactory,
        ..CheckOptions::default()
    }
}

fn check(source: &str, inputs: FoundationInputs) -> CheckResult {
    check_with_foundations(
        &[SourceFile::new("caller.py", "caller", source)],
        TARGET,
        options(),
        inputs,
    )
}

fn diagnostics(report: &CheckResult, code: &str) -> Vec<usize> {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .map(|diagnostic| diagnostic.primary.line)
        .collect()
}

#[test]
fn chained_task_factory_mutation_invalidates_default_scheduling() {
    let report = check(
        r#"from asyncio import create_task, eager_task_factory, get_running_loop
async def worker(): return 1
async def run():
    get_running_loop().set_task_factory(eager_task_factory)
    task = create_task(worker())
    task.cancel()
    try:
        await task
    except BaseException:
        return
    handle = open("reachable.txt")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let uses = &report.foundations.as_ref().unwrap().model_uses;
    assert!(
        uses.iter()
            .any(|event| event.symbol == "asyncio.create_task" && !event.applicable),
        "{report:#?}"
    );
    assert!(
        uses.iter().any(|event| event.symbol == "builtins.open"),
        "{report:#?}"
    );
}

#[test]
fn await_after_task_group_exit_replays_cancelled_child() {
    let report = check(
        r#"from asyncio import CancelledError as Cancelled, TaskGroup
async def worker(): pass
async def run():
    async with TaskGroup() as group:
        task = group.create_task(worker())
        task.cancel()
    try:
        await task
    except Cancelled:
        return
    handle = open("unreachable.txt")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    assert!(
        !report
            .foundations
            .as_ref()
            .unwrap()
            .model_uses
            .iter()
            .any(|event| event.symbol == "builtins.open"),
        "{report:#?}"
    );
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
}

#[test]
fn timeout_converts_its_deadline_instead_of_a_child_cancellation() {
    let report = check(
        r#"from asyncio import CancelledError, create_task, sleep, timeout
async def worker(): pass
async def child_cancelled():
    child = create_task(worker())
    child.cancel()
    try:
        async with timeout(60):
            await child
    except TimeoutError:
        handle = open("unreachable.txt")
    except CancelledError:
        return

async def deadline():
    try:
        async with timeout(0):
            try:
                async with timeout(60):
                    await sleep(1)
            except TimeoutError:
                pass
    except TimeoutError:
        handle = open("reachable.txt")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let opens = report
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .filter(|event| event.symbol == "builtins.open")
        .map(|event| event.site.line)
        .collect::<Vec<_>>();
    assert_eq!(opens, [23], "{report:#?}");
}

#[test]
fn pool_binds_submission_before_checking_shutdown_state() {
    let report = check(
        r#"from multiprocessing import get_context
def run():
    pool = get_context("spawn").Pool(1)
    pool.close()
    try:
        pool.apply_async(args=(1,))
    except TypeError:
        handle = open("reachable.txt")
    pool.join()
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    assert!(diagnostics(&report, "TP2102").is_empty(), "{report:#?}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP1002"
                && finding.completion.as_deref() == Some("fallthrough")),
        "{report:#?}"
    );
}

#[test]
fn from_package_imports_selected_submodules_but_respects_parent_attributes() {
    let mut provider = package();
    provider.sources.push(SourceFile::new(
        "vendor/operations.py",
        "vendor.operations",
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\n",
    ));
    let caller = "from vendor import operations as ops\nops.fetch()\n";
    let inputs = |mut provider| {
        let manifest = export_manifest(&provider, TARGET).unwrap();
        provider.manifest = Some(serde_json::to_string(&manifest).unwrap());
        FoundationInputs {
            providers: vec![provider],
            ..FoundationInputs::default()
        }
    };
    let mut session = AnalysisSession::default();
    let sources = [SourceFile::new("caller.py", "caller", caller)];
    let report =
        session.check_with_foundations(&sources, TARGET, options(), inputs(provider.clone()));
    assert_eq!(diagnostics(&report, "TP3001"), [2], "{report:#?}");
    let scope = report.foundations.unwrap().scope;
    assert!(scope.iter().any(|source| source.module == "caller"
        && source
            .supplied_imports
            .contains(&"vendor.operations".into())));

    provider.sources[0].source.push_str("\noperations = None\n");
    let shadowed = inputs(provider);
    let report = session.check_with_foundations(&sources, TARGET, options(), shadowed.clone());
    assert!(diagnostics(&report, "TP3001").is_empty(), "{report:#?}");
    assert_eq!(report, check(caller, shadowed));
}

#[test]
fn join_binding_failure_runs_type_error_handler_before_checking_handle_state() {
    let report = check(
        r#"from multiprocessing import get_context
def worker(): pass
def run():
    proc = get_context("spawn").Process(target=worker)
    try:
        proc.join(invalid=True)
    except TypeError:
        handle = open("text.txt")
        handle.read()
    proc.close()
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    assert!(diagnostics(&report, "TP2102").is_empty(), "{report:#?}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP1002"
                && finding.completion.as_deref() == Some("fallthrough")),
        "{report:#?}"
    );
}

#[test]
fn repeated_task_await_preserves_failure_and_possible_success() {
    let report = check(
        r#"from asyncio import create_task
async def always_fails(): raise ValueError()
async def maybe_fails(flag):
    if flag:
        raise ValueError()
async def rejected():
    task = create_task(always_fails())
    try:
        await task
    except ValueError:
        pass
    try:
        await task
    except ValueError:
        return
    handle = open("unreachable.txt")
async def mixed(flag):
    task = create_task(maybe_fails(flag))
    try:
        await task
    except ValueError:
        pass
    try:
        await task
    except ValueError:
        return
    handle = open("reachable.txt")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let ledger = &report.foundations.as_ref().unwrap().model_uses;
    let opens = ledger
        .iter()
        .filter(|event| event.symbol == "builtins.open")
        .map(|event| event.site.line)
        .collect::<Vec<_>>();
    assert_eq!(opens, [27], "{report:#?}");
}

#[test]
fn entering_an_existing_text_file_closes_all_aliases_and_rejects_closed_entry() {
    let report = check(
        r#"def run():
    handle = open("text.txt")
    with handle as alias:
        alias.read()
    handle.read()

def closed_entry():
    handle = open("text.txt")
    handle.close()
    with handle:
        unreachable = open("never.txt")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    assert_eq!(diagnostics(&report, "TP1001"), [5], "{report:#?}");
    assert!(
        !report
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("context-manager cleanup has no qualified model")),
        "{report:#?}"
    );
    assert!(
        !report
            .foundations
            .as_ref()
            .unwrap()
            .model_uses
            .iter()
            .any(|event| { event.symbol == "builtins.open" && event.site.line == 11 }),
        "{report:#?}"
    );
}

#[test]
fn installed_inline_and_manifest_share_checked_ownership_effects() {
    let inline = package();
    let mut encoded = inline.clone();
    let manifest = export_manifest(&inline, TARGET).unwrap();
    let acquire = manifest
        .symbols
        .iter()
        .find(|symbol| symbol.public_name == "vendor.acquire")
        .unwrap();
    assert_eq!(acquire.canonical_name, "vendor._implementation.acquire");
    assert_eq!(acquire.contracts[0].kind, "owned-return");
    encoded.manifest = Some(serde_json::to_string(&manifest).unwrap());

    let source = format!("{IMPORTS}from codepact import move\n\ndef leak():\n    r = acquire()\n\ndef repaired():\n    r = acquire()\n    r.close()\n\ndef bad_move():\n    r = acquire()\n    consume(move(r))\n    r.read()\n\ndef unused():\n    fetch()\n");
    let results = [inline, encoded].map(|provider| {
        check(
            &source,
            FoundationInputs {
                providers: vec![provider],
                ..FoundationInputs::default()
            },
        )
    });
    for result in &results {
        assert_eq!(diagnostics(result, "TP3001"), [17], "{result:#?}");
        assert_eq!(diagnostics(result, "TP2002"), [14], "{result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|finding| finding.code == "TP1002"
                    && finding.primary.line == 4
                    && finding.completion.as_deref() == Some("fallthrough")),
            "{result:#?}"
        );
        assert!(!diagnostics(result, "TP1002").contains(&7), "{result:#?}");
        let uses = &result.foundations.as_ref().unwrap().model_uses;
        assert!(
            uses.iter()
                .any(|entry| entry.effect == "owned-return" && entry.applicable),
            "{uses:#?}"
        );
        assert!(
            uses.iter()
                .any(|entry| entry.effect == "owned-parameter" && entry.applicable),
            "{uses:#?}"
        );
    }
    assert_eq!(
        diagnostics(&results[0], "TP1002"),
        diagnostics(&results[1], "TP1002")
    );
}

#[test]
fn manifest_filters_each_public_export_and_never_falls_back_on_stale_inputs() {
    let mut provider = package();
    let mut manifest = export_manifest(&provider, TARGET).unwrap();
    manifest
        .symbols
        .retain(|symbol| symbol.public_name != "vendor.hidden");
    let acquire = manifest
        .symbols
        .iter_mut()
        .find(|symbol| symbol.public_name == "vendor.acquire")
        .unwrap();
    acquire.contracts[0]
        .required_capabilities
        .push("future-required-effect".into());
    provider.manifest = Some(serde_json::to_string(&manifest).unwrap());
    let source = format!("{IMPORTS}def run():\n    fetch()\n    hidden()\n    r = acquire()\n");
    let inputs = |package| FoundationInputs {
        providers: vec![package],
        ..FoundationInputs::default()
    };
    let caller = [SourceFile::new("caller.py", "caller", &source)];
    let mut session = AnalysisSession::default();
    let good = session.check_with_foundations(&caller, TARGET, options(), inputs(provider.clone()));
    assert_eq!(diagnostics(&good, "TP3001"), [3], "{good:#?}");
    assert!(good
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("unsupported mandatory capability")));
    assert!(diagnostics(&good, "TP1002").is_empty());

    provider.sources[1]
        .source
        .push_str("\n# change the installed file without changing its version\n");
    let stale =
        session.check_with_foundations(&caller, TARGET, options(), inputs(provider.clone()));
    assert!(diagnostics(&stale, "TP3001").is_empty(), "{stale:#?}");
    assert!(stale
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("stale contract manifest")));
    assert_eq!(stale, check(&source, inputs(provider)));
}

#[test]
fn exact_project_models_apply_only_to_selected_provider_and_lock() {
    let mut provider = package();
    provider.marked = false;
    let model = ProjectResourceModel {
        distribution: provider.distribution.clone(),
        version: provider.version.clone(),
        factory: "vendor.acquire".into(),
        resource: "vendor._implementation.Channel".into(),
        close_method: "close".into(),
        provenance: "fixture revision 1".into(),
        override_publication: false,
        exceptional_close: ExceptionalClose::Unknown,
    };
    let lock = |version| {
        format!("version = 1\nrevision = 3\n[[package]]\nname = 'test-vendor'\nversion = '{version}'\nsource = {{registry = 'https://pypi.org/simple'}}\n")
    };
    let mut inputs = FoundationInputs {
        providers: vec![provider],
        project_models: vec![model],
        uv_lock: Some(lock("1.2")),
        ..FoundationInputs::default()
    };
    let source = "from vendor import acquire\n\ndef leak():\n    owner = acquire()\n\ndef repair():\n    owner = acquire()\n    owner.close()\n";
    let result = check(source, inputs.clone());
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "TP1002"
                && finding.completion.as_deref() == Some("fallthrough"))
            .map(|finding| finding.primary.line)
            .collect::<Vec<_>>(),
        [3],
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("exceptional postcondition")));
    assert!(result
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| entry.applicable
            && entry.provider.is_some()
            && entry.effect == "receiver-cleanup"));
    let mut rebound = inputs.clone();
    rebound.providers[0].sources[1].source = PROVIDER.replace(
        "    def read(self): return \"data\"",
        "    def read(self): return \"data\"\n    close = None",
    );
    let rejected = check(source, rebound);
    assert!(
        rejected
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("cleanup method") && reason.contains(".close")),
        "{rejected:#?}"
    );
    assert!(
        !rejected
            .foundations
            .as_ref()
            .unwrap()
            .model_uses
            .iter()
            .any(|entry| entry.applicable && entry.effect == "receiver-cleanup"),
        "{rejected:#?}"
    );
    inputs.uv_lock = Some(lock("1.1"));
    let incorrect = check(source, inputs.clone());
    assert!(incorrect
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("uv.lock version")));
    assert!(diagnostics(&incorrect, "TP1002").is_empty());
    inputs.providers[0].version = "1.3".into();
    let upgrade = check(source, inputs);
    assert!(upgrade
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("project model expects")));
}

#[test]
fn override_reports_replaced_origin_and_updates_exceptional_cleanup() {
    let provider = package();
    let mut model = ProjectResourceModel {
        distribution: provider.distribution.clone(),
        version: provider.version.clone(),
        factory: "vendor.acquire".into(),
        resource: "vendor._implementation.Channel".into(),
        close_method: "close".into(),
        provenance: "control review".into(),
        override_publication: true,
        exceptional_close: ExceptionalClose::Unknown,
    };
    let files = [
        SourceFile::new(
            "caller.py",
            "caller",
            "from vendor import acquire\ndef run():\n    handle = acquire()\n    handle.close()\n",
        ),
        SourceFile::new("sibling.py", "sibling", "def unchanged(): return 1\n"),
    ];
    let inputs = |model| FoundationInputs {
        providers: vec![provider.clone()],
        project_models: vec![model],
        ..FoundationInputs::default()
    };
    let mut session = AnalysisSession::default();
    let before = session.check_with_foundations(&files, TARGET, options(), inputs(model.clone()));
    assert!(before
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("exceptional postcondition")));
    model.exceptional_close = ExceptionalClose::Closed;
    let after = session.check_with_foundations(&files, TARGET, options(), inputs(model.clone()));
    assert_eq!(
        after,
        check_with_foundations(&files, TARGET, options(), inputs(model))
    );
    assert!(diagnostics(&after, "TP1002").is_empty(), "{after:#?}");
    assert!(after
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| entry
            .provider
            .as_ref()
            .and_then(|origin| origin.replaced_origin.as_deref())
            == Some("inline-source")));
}

#[test]
fn installed_handoff_keeps_pre_entry_move_and_binding_failures_separate() {
    let mut provider = package();
    let manifest = export_manifest(&provider, TARGET).unwrap();
    provider.manifest = Some(serde_json::to_string(&manifest).unwrap());
    let source = r#"from vendor import acquire, consume
from codepact import move

def fail():
    raise ValueError("control")

def before_move():
    owner = acquire()
    try:
        consume(fail=fail(), value=move(owner))
    except ValueError:
        owner.close()

def after_move():
    owner = acquire()
    try:
        consume(value=move(owner), fail=fail())
    except ValueError:
        owner.close()

def failed_binding():
    owner = acquire()
    try:
        consume(move(owner), value=0)
    except TypeError:
        owner.close()
"#;
    let result = check(
        source,
        FoundationInputs {
            providers: vec![provider],
            ..FoundationInputs::default()
        },
    );
    assert_eq!(diagnostics(&result, "TP2002"), [19, 26], "{result:#?}");
}

#[test]
fn process_failed_launch_and_unqualified_untimed_wait_keep_independent_evidence() {
    let result = check(
        r#"from multiprocessing import get_context
from signal import signal

def worker(value): return value
def handler(signum, frame): raise OSError()
signal(14, handler)

def rejected():
    p = get_context("spawn").Process(target=worker, args=(lambda: 1,))
    try:
        p.start()
    except AttributeError:
        p.close()

def unknown_wait():
    p = get_context("spawn").Process(target=worker, args=(1,))
    p.start()
    p.join()
    p.close()
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let ledger = &result.foundations.as_ref().unwrap().model_uses;
    assert!(ledger
        .iter()
        .any(|entry| entry.effect == "serialization-rejected-before-launch" && entry.applicable));
    assert!(ledger
        .iter()
        .any(|entry| entry.effect == "join" && !entry.applicable));
    assert_eq!(diagnostics(&result, "TP2102"), [19], "{result:#?}");
}

#[test]
fn local_file_model_keeps_aliases_and_context_cleanup() {
    let result = check(
        r#"def reads_after_close():
    owner = open("fixture.txt")
    a = owner
    owner.close()
    a.read()

def cleanup():
    with open("fixture.txt", "r") as reader:
        reader.read()

def leak():
    file = open("fixture.txt", "w", encoding="utf-8")
    file.write("text")
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    assert_eq!(diagnostics(&result, "TP1001"), [5], "{result:#?}");
    assert!(!diagnostics(&result, "TP1002").contains(&7));
    assert!(diagnostics(&result, "TP1002").contains(&11), "{result:#?}");
    let unavailable = check(
        "def read():\n    handle = open('x')\n",
        FoundationInputs::default(),
    );
    assert!(unavailable
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("explicit analyzed runtime profile")));
    assert!(diagnostics(&unavailable, "TP1002").is_empty());
}

#[test]
fn local_async_task_completion_does_not_follow_a_cancel_request_or_shield_timeout() {
    let result = check(
        r#"from asyncio import create_task, shield, sleep, timeout
from codepact import resource
from codepact.rust import Borrowed

@resource(closed_by="close")
class Channel:
    def close(self): pass
    def read(self): return 1

async def use(value: Borrowed[Channel]):
    await sleep(0)
    value.read()

async def joined():
    owner = Channel()
    task = create_task(use(owner))
    alias = task
    await alias
    owner.close()

async def cancelled_only():
    owner = Channel()
    task = create_task(use(owner))
    task.cancel()
    owner.close()

async def shielded():
    owner = Channel()
    task = create_task(use(owner))
    try:
        async with timeout(0):
            await shield(task)
    except TimeoutError:
        owner.close()
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let sites = diagnostics(&result, "TP2006");
    assert!(!sites.contains(&19), "{result:#?}");
    assert!(sites.contains(&25), "{result:#?}");
    assert!(sites.contains(&34), "{result:#?}");
}

#[test]
fn process_join_requires_proof_and_pool_shutdown_preserves_protocol_state() {
    let result = check(
        r#"from multiprocessing import get_context

def worker(value):
    return value

def probe():
    ctx = get_context("spawn")
    safe = ctx.Process(target=worker, args=(1,))
    alias = safe
    alias.start()
    safe.join()
    alias.close()

def timed():
    proc = get_context("spawn").Process(target=worker, args=(1,))
    proc.start()
    proc.join(0)
    proc.close()

def guarded():
    proc = get_context("spawn").Process(target=worker, args=(1,))
    proc.start()
    proc.join(0)
    if proc.exitcode is not None:
        proc.close()

def post_shutdown():
    pool = get_context("spawn").Pool(1)
    pool.apply_async(worker, (1,))
    pool.close()
    pool.join()
    pool.apply_async(worker, (2,))

def context_shutdown():
    with get_context("spawn").Pool(1) as pool:
        pool.apply_async(worker, (1,))
    pool.apply_async(worker, (2,))
"#,
        FoundationInputs {
            runtime: Some(runtime()),
            ..FoundationInputs::default()
        },
    );
    let sites = diagnostics(&result, "TP2102");
    assert_eq!(sites, [18, 32, 37], "{result:#?}");
    assert!(result
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| entry.effect == "forced-context-exit"));
    assert!(result
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| {
            entry.effect == "admitted-work-drained-results-unobserved" && entry.applicable
        }));
}

#[test]
fn runtime_profile_edits_refresh_unchanged_sources_and_recover() {
    let files = [SourceFile::new(
        "caller.py",
        "caller",
        "from asyncio import create_task\nasync def worker(): pass\nasync def run():\n    task = create_task(worker())\n    await task\n    file = open('text.txt')\n",
    )];
    let mut session = AnalysisSession::default();
    let inputs = |profile| FoundationInputs {
        runtime: Some(profile),
        ..FoundationInputs::default()
    };
    let check = |session: &mut AnalysisSession, profile| {
        session.check_with_foundations(&files, TARGET, options(), inputs(profile))
    };
    let matches = check(&mut session, runtime());
    assert!(matches.diagnostics.iter().any(|d| d.code == "TP1002"));
    let mut wrong_patch = runtime();
    wrong_patch.python_version = "3.13.15".into();
    let unsupported = check(&mut session, wrong_patch.clone());
    assert_eq!(
        unsupported,
        check(&mut AnalysisSession::default(), wrong_patch)
    );
    assert!(diagnostics(&unsupported, "TP1002").is_empty());
    assert!(unsupported
        .coverage
        .reasons
        .iter()
        .any(|r| r.contains("requires CPython 3.13.16")));

    let mut wrong_loop = runtime();
    wrong_loop.event_loop = Some("custom".into());
    let unavailable = check(&mut session, wrong_loop);
    assert!(unavailable.diagnostics.iter().any(|d| d.code == "TP1002"));
    assert!(unavailable
        .foundations
        .as_ref()
        .unwrap()
        .model_uses
        .iter()
        .any(|entry| { entry.symbol == "asyncio.create_task" && !entry.applicable }));
    assert_eq!(check(&mut session, runtime()), matches);
}

use codepact::{check_with_ownership, CheckResult, Ownership, SourceFile};

const HEADER: &str = "from codepact import resource, move\nfrom codepact.rust import Owned, Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def read(self): pass\n";

fn check(body: &str) -> CheckResult {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!("{HEADER}{body}"),
    )];
    check_with_ownership(&files, "3.11", Ownership::Enabled)
}

fn rules(result: &CheckResult) -> Vec<&str> {
    result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn has(result: &CheckResult, code: &str) {
    assert!(
        rules(result).contains(&code),
        "{code} missing from {result:#?}"
    );
}

fn lacks(result: &CheckResult, code: &str) {
    assert!(
        !rules(result).contains(&code),
        "{code} unexpected in {result:#?}"
    );
}

fn group(body: &str) -> CheckResult {
    let files = [SourceFile::new("main.py", "main", format!("{HEADER}from asyncio import TaskGroup\nasync def read(value: Borrowed[C]):\n    value.read()\n{body}"))];
    check_with_ownership(&files, "3.11", Ownership::AssumeDefaultTaskFactory)
}

#[test]
fn v05_01_default_scope_is_unchanged() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!("{HEADER}def run():\n    a = C()\n    b = move(a)\n    a.read()\n    b.close()\n"),
    )];
    let legacy = codepact::check_with_must_use(&files, "3.11");
    let default = check_with_ownership(&files, "3.11", Ownership::Disabled);
    assert_eq!(
        default,
        codepact::CheckResult::from_inspection(legacy, "3.11")
    );
    lacks(&default, "TP2002");
}

#[test]
fn v05_02_move_preserves_instance_and_duty() {
    let result = check("def run():\n    a = C()\n    b = move(a)\n    b.read()\n    b.close()\n");
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_02_reuse_close_copy_and_double_move() {
    for use_after in ["a.read()", "a.close()", "alias = a", "other = move(a)"] {
        let result = check(&format!(
            "def run():\n    a = C()\n    b = move(a)\n    {use_after}\n    b.close()\n"
        ));
        has(&result, "TP2002");
    }
}

#[test]
fn v05_02_rebinding_restores_only_the_new_instance() {
    let result = check(
        "def run():\n    a = C()\n    b = move(a)\n    a = C()\n    a.close()\n    b.close()\n",
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_02_branch_move_retains_possible_reuse() {
    let result = check("def run(flag):\n    a = C()\n    if flag:\n        b = move(a)\n        b.close()\n    a.read()\n");
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2002")
        .expect("possible use after move");
    assert_eq!(finding.certainty, "possible");
}

#[test]
fn v05_03_argument_evaluation_preserves_committed_move_on_raise() {
    let result = check("def fail():\n    raise ValueError()\ndef take(value: Owned[C], extra):\n    value.close()\ndef run():\n    a = C()\n    try:\n        take(move(a), fail())\n    except ValueError:\n        a.read()\n");
    has(&result, "TP2002");
    has(&result, "TP1002");
}

#[test]
fn v05_03_failure_before_argument_does_not_move() {
    let result = check("def fail():\n    raise ValueError()\ndef take(extra, value: Owned[C]):\n    value.close()\ndef run():\n    a = C()\n    try:\n        take(fail(), move(a))\n    except ValueError:\n        a.close()\n");
    lacks(&result, "TP2002");
    lacks(&result, "TP1002");
}

#[test]
fn v05_03_binding_failure_does_not_undo_move() {
    let result = check("def take(value: Owned[C]):\n    value.close()\ndef run():\n    a = C()\n    try:\n        take(move(a), 1)\n    except TypeError:\n        a.read()\n");
    has(&result, "TP2002");
    has(&result, "TP1002");
}

#[test]
fn v05_03_earlier_argument_pins_an_alias() {
    let result = check("def take(borrow: Borrowed[C], owner: Owned[C]):\n    borrow.read()\n    owner.close()\ndef run():\n    a = C()\n    take(a, move(a))\n    a.close()\n");
    has(&result, "TP2003");
}

#[test]
fn v05_03_later_argument_reads_moved_binding() {
    let result = check("def take(owner: Owned[C], borrow: Borrowed[C]):\n    owner.close()\ndef run():\n    a = C()\n    take(move(a), a)\n");
    has(&result, "TP2002");
}

#[test]
fn v05_04_owned_callee_receives_the_existing_duty() {
    let result = check("def take(value: Owned[C]):\n    value.close()\ndef run():\n    a = C()\n    take(move(a))\n");
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_04_bare_owned_argument_requires_move() {
    let result = check(
        "def take(value: Owned[C]):\n    value.close()\ndef run():\n    a = C()\n    take(a)\n",
    );
    has(&result, "TP2005");
    has(&result, "TP1002");
}

#[test]
fn v05_04_exceptional_close_effects_keep_source_moved() {
    for body in [
        "raise ValueError()",
        "value.close()\n    raise ValueError()",
    ] {
        let result = check(&format!("def take(value: Owned[C]):\n    {body}\ndef run():\n    a = C()\n    try:\n        take(move(a))\n    except ValueError:\n        a.read()\n"));
        has(&result, "TP2002");
        if body.starts_with("raise") {
            has(&result, "TP1002");
        } else {
            lacks(&result, "TP1002");
        }
    }
}

#[test]
fn v05_05_owned_returns_and_transitive_moves() {
    let result = check("def make() -> Owned[C]:\n    return C()\ndef relay(value: Owned[C]) -> Owned[C]:\n    return move(value)\ndef run():\n    a = make()\n    b = relay(move(a))\n    b.close()\n");
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_05_return_replaced_by_finally_keeps_lost_duty() {
    let result = check("def make() -> Owned[C]:\n    a = C()\n    try:\n        return move(a)\n    finally:\n        return None\n");
    has(&result, "TP1002");
}

#[test]
fn v05_06_alias_conflict_and_rebinding() {
    let invalid = check("def run():\n    a = C()\n    alias = a\n    b = move(a)\n    a.close()\n");
    has(&invalid, "TP2003");
    let valid = check("def run():\n    a = C()\n    alias = a\n    alias = None\n    b = move(a)\n    b.close()\n");
    assert!(valid.diagnostics.is_empty(), "{valid:#?}");
}

#[test]
fn v05_06_plain_alias_is_not_an_owner() {
    let result =
        check("def run():\n    a = C()\n    alias = a\n    a = None\n    b = move(alias)\n");
    has(&result, "TP2003");
}

#[test]
fn v05_06_cleanup_frame_blocks_transfer() {
    let result = check("from contextlib import closing\ndef run():\n    a = C()\n    with closing(a):\n        b = move(a)\n");
    has(&result, "TP2003");
}

#[test]
fn v05_07_nested_shared_borrow_ends_at_return() {
    let result = check("def read(value: Borrowed[C]):\n    value.read()\ndef nested(value: Borrowed[C]):\n    read(value)\ndef run():\n    a = C()\n    nested(a)\n    b = move(a)\n    b.close()\n");
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_07_borrow_close_move_and_known_escapes() {
    for operation in [
        "value.close()",
        "other = move(value)",
        "return value",
        "saved = [value]",
        "holder.value = value",
        "holder[0] = value",
        "global saved\n    saved = value",
        "return lambda: value.read()",
    ] {
        let result = check(&format!("def bad(value: Borrowed[C]):\n    {operation}\n"));
        has(
            &result,
            if operation == "value.close()" {
                "TP2001"
            } else {
                "TP2004"
            },
        );
    }
}

#[test]
fn v05_07_returned_closure_and_callback_capture() {
    let result = check("def bad(value: Borrowed[C]):\n    def closure():\n        value.read()\n    return closure\n");
    has(&result, "TP2004");
    let result = check("def boundary(value: Borrowed[C]):\n    def closure():\n        value.read()\n    unknown(closure)\n");
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v05_08_identity_aliases_and_qualified_helpers() {
    for imports in [
        "from codepact import move as transfer",
        "import codepact as cp\ntransfer = cp.move",
    ] {
        let result = check(&format!("{imports}\ndef run():\n    a = C()\n    b = transfer(a)\n    a.read()\n    b.close()\n"));
        has(&result, "TP2002");
    }
    let result = check("def move(value):\n    return value\ndef run():\n    a = C()\n    b = move(a)\n    a.close()\n");
    lacks(&result, "TP2002");
}

#[test]
fn v05_08_cross_module_reexports_and_qualified_calls() {
    let files = [
        SourceFile::new("models.py", "models", HEADER),
        SourceFile::new("api.py", "api", "from codepact import move as transfer\nfrom models import C\nfrom codepact.rust import Owned\ndef take(value: Owned[C]):\n    value.close()\n"),
        SourceFile::new("main.py", "main", "from models import C\nfrom api import transfer\nimport api\ndef run():\n    a = C()\n    api.take(transfer(a))\n    a.read()\n"),
    ];
    let result = check_with_ownership(&files, "3.11", Ownership::Enabled);
    has(&result, "TP2002");
    lacks(&result, "TP1002");
}

#[test]
fn v05_09_native_escape_never_restores_uniqueness() {
    let result = check("def run():\n    a = C()\n    native(a)\n    b = move(a)\n");
    assert_eq!(result.coverage.status, "incomplete");
    lacks(&result, "TP2002");
}

#[test]
fn v05_11_group_completion_precedes_owner_cleanup() {
    let result = group("async def run():\n    a = C()\n    try:\n        async with TaskGroup() as group:\n            group.create_task(read(a))\n    finally:\n        a.close()\n");
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn v05_11_close_with_outstanding_task_borrow() {
    let result = group("async def run():\n    a = C()\n    async with TaskGroup() as group:\n        group.create_task(read(a))\n        a.close()\n");
    has(&result, "TP2006");
}

#[test]
fn v05_11_nested_group_does_not_release_outer_borrow() {
    let result = group("async def run():\n    a = C()\n    async with TaskGroup() as outer:\n        outer.create_task(read(a))\n        async with TaskGroup() as inner:\n            inner.create_task(read(a))\n        a.close()\n");
    has(&result, "TP2006");
}

#[test]
fn v05_11_cancellation_request_keeps_borrow_outstanding() {
    let result = group("async def run():\n    a = C()\n    async with TaskGroup() as group:\n        task = group.create_task(read(a))\n        task.cancel()\n        a.close()\n");
    has(&result, "TP2006");
}

#[test]
fn v05_11_pending_coroutine_captures_before_scheduling() {
    let result = group("async def run():\n    a = C()\n    pending = read(a)\n    a.close()\n");
    has(&result, "TP2006");
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v05_12_prestart_cancel_does_not_execute_owned_cleanup() {
    let result = group("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    a = C()\n    async with TaskGroup() as group:\n        group.create_task(take(move(a)))\n    a.read()\n");
    has(&result, "TP2002");
    has(&result, "TP1002");
}

#[test]
fn v05_12_inactive_submission_preserves_the_move() {
    let result = group("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    async with TaskGroup() as group:\n        pass\n    a = C()\n    try:\n        group.create_task(take(move(a)))\n    except RuntimeError:\n        a.read()\n");
    has(&result, "TP2002");
    has(&result, "TP1002");
}

#[test]
fn v05_13_session_matches_fresh_after_scope_and_effect_edits() {
    let original = SourceFile::new(
        "main.py",
        "main",
        format!("{HEADER}def run():\n    a = C()\n    b = move(a)\n    a.read()\n    b.close()\n"),
    );
    let changed = SourceFile::new(
        "main.py",
        "main",
        original.source.replace("    a.read()\n", ""),
    );
    let mut session = codepact::AnalysisSession::default();
    for files in [[original.clone()], [changed], [original]] {
        for mode in [
            Ownership::Disabled,
            Ownership::Enabled,
            Ownership::AssumeDefaultTaskFactory,
        ] {
            assert_eq!(
                session.check_with_ownership(&files, "3.11", mode),
                check_with_ownership(&files, "3.11", mode)
            );
        }
    }
}

#[test]
fn v05_10_recursive_fixed_point_applies_base_exit_effect() {
    let result = check("def take(value: Owned[C], again):\n    if again:\n        take(move(value), False)\n    else:\n        value.close()\ndef run():\n    a = C()\n    take(move(a), True)\n");
    lacks(&result, "TP1002");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v05_10_mutual_recursion_and_missing_base_exit() {
    let good = check("def left(value: Owned[C], again):\n    if again:\n        right(move(value), False)\n    else:\n        value.close()\ndef right(value: Owned[C], again):\n    left(move(value), again)\ndef run():\n    a = C()\n    left(move(a), True)\n");
    lacks(&good, "TP1002");
    assert_eq!(
        good.coverage.status, "complete-for-supported-subset",
        "{good:#?}"
    );
    let unresolved = check("def recurse(value: Owned[C]):\n    recurse(move(value))\ndef run():\n    a = C()\n    recurse(move(a))\n");
    assert!(
        unresolved
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("no usable completion")),
        "{unresolved:#?}"
    );
}

#[test]
fn v05_10_recursive_exceptional_and_normal_outcomes_remain_distinct() {
    let result = check("def take(value: Owned[C], again, fail):\n    if again:\n        take(move(value), False, fail)\n    else:\n        if fail:\n            raise ValueError()\n        else:\n            value.close()\ndef run():\n    a = C()\n    take(move(a), True, True)\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"
                && diagnostic.completion.as_deref() == Some("raise")),
        "{result:#?}"
    );
}

#[test]
fn v05_12_direct_await_commits_before_worker_entry() {
    let result = check("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    a = C()\n    await take(move(a))\n    a.read()\n");
    has(&result, "TP2002");
}

#[test]
fn v05_12_normal_task_join_and_pre_start_cancellation_have_distinct_duties() {
    let result = group("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    a = C()\n    async with TaskGroup() as group:\n        group.create_task(take(move(a)))\n");
    let duties = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(!duties.is_empty(), "{result:#?}");
    assert!(
        duties
            .iter()
            .all(|diagnostic| diagnostic.completion.as_deref() == Some("cancelled")),
        "{result:#?}"
    );
    let cancelled = group("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    a = C()\n    async with TaskGroup() as group:\n        task = group.create_task(take(move(a)))\n        task.cancel()\n");
    assert!(
        cancelled
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"
                && diagnostic.completion.as_deref() == Some("fallthrough")
                && diagnostic.certainty == "definite"),
        "{cancelled:#?}"
    );
}

#[test]
fn v05_12_task_factory_mutation_or_unqualified_target_is_a_boundary() {
    let source = "import asyncio\nasync def run(loop, factory):\n    loop.set_task_factory(factory)\n    async with asyncio.TaskGroup() as group:\n        group.create_task(run(loop, factory))\n";
    let files = [SourceFile::new("main.py", "main", source)];
    let result = check_with_ownership(&files, "3.13", Ownership::AssumeDefaultTaskFactory);
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("invalidates the declared default-factory model")));
    let boundary = check("from asyncio import TaskGroup\nasync def run():\n    async with TaskGroup() as group:\n        pass\n");
    assert!(boundary
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("requires the explicit default-task-factory assumption")));
}

#[test]
fn v05_15_preview_project_oracle_and_current_binary_replay() {
    use serde_json::Value;
    use std::{fs, path::Path, process::Command};

    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/projects");
    let oracle: Value =
        serde_json::from_str(include_str!("projects/ownership-oracle.json")).unwrap();
    let binary = std::env::var_os("CODEPACT_QUALIFICATION_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_codepact").into());
    let retention = std::env::var_os("CODEPACT_PHASE05_EVIDENCE_DIR");
    let run = |name: &str, variant: &str, target: &str, strict: bool| {
        let root = corpus.join(name).join(variant);
        let mut command = Command::new(&binary);
        command
            .current_dir(&root)
            .args(["check", "--python-version", target, "--json"]);
        if strict {
            command.args(["--mode", "strict"]);
        }
        let output = command.output().unwrap();
        if let Some(directory) = &retention {
            let path = Path::new(directory).join(format!(
                "{name}-{variant}-{target}-{}.json",
                if strict { "strict" } else { "advisory" }
            ));
            fs::write(path, &output.stdout).unwrap();
        }
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        (output.status.code(), report)
    };
    for target in ["3.11", "3.12", "3.13"] {
        for case in oracle["pairs"].as_array().unwrap() {
            let project = case["project"].as_str().unwrap();
            let (status, buggy) = run(project, "buggy", target, false);
            assert_eq!(status, Some(1), "{project} {buggy:#}");
            let findings = buggy["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|finding| finding["code"] == case["code"])
                .collect::<Vec<_>>();
            assert_eq!(findings.len(), 1, "{project} {buggy:#}");
            assert_eq!(findings[0]["primary"]["line"], case["line"], "{project}");
            assert_eq!(findings[0]["certainty"], case["certainty"]);
            let (status, fixed) = run(project, "fixed", target, true);
            assert_eq!(status, Some(0), "{project} {fixed:#}");
            assert_eq!(fixed["total_diagnostics"], 0, "{project} {fixed:#}");
            let (_, repaired) = run(project, "fixed", target, false);
            assert_eq!(buggy["policy_fingerprint"], repaired["policy_fingerprint"]);

            let mut session = codepact::AnalysisSession::default();
            for variant in ["buggy", "fixed"] {
                let source =
                    fs::read_to_string(corpus.join(project).join(variant).join("src/main.py"))
                        .unwrap();
                let files = [SourceFile::new("src/main.py", "src.main", source)];
                let mode = Ownership::AssumeDefaultTaskFactory;
                let expected = check_with_ownership(&files, target, mode);
                assert_eq!(session.check_with_ownership(&files, target, mode), expected);
                assert_eq!(session.check_with_ownership(&files, target, mode), expected);
            }
        }
        let boundary = &oracle["boundary"];
        let (status, report) = run(
            boundary["project"].as_str().unwrap(),
            "boundary",
            target,
            true,
        );
        assert_eq!(status, Some(1), "{report:#}");
        assert_eq!(report["coverage"]["status"], "incomplete");
        for reason in boundary["reasons"].as_array().unwrap() {
            assert!(
                report["coverage"]["reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|actual| actual.as_str().unwrap().contains(reason.as_str().unwrap())),
                "{report:#}"
            );
        }
    }
}

#[test]
fn v05_14_cli_scope_reports_and_baselines_follow_the_opt_in() {
    use serde_json::Value;
    use std::{fs, process::Command};

    let dir =
        std::env::temp_dir().join(format!("codepact-ownership-policy-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_codepact"))
            .current_dir(&dir)
            .args(arguments)
            .output()
            .unwrap();
        let document = serde_json::from_slice::<Value>(&output.stdout).unwrap_or(Value::Null);
        (
            output.status.code(),
            document,
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    };
    fs::write(
        dir.join("main.py"),
        format!("{HEADER}def run():\n    a = C()\n    b = move(a)\n    a.read()\n    b.close()\n"),
    )
    .unwrap();
    fs::write(
        dir.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version = 1\nexperimental-ownership = true\n",
    )
    .unwrap();

    let (exit, config, _) = run(&["check", "--show-settings", "--json"]);
    assert_eq!(exit, Some(0));
    assert_eq!(config["experimental_ownership"], true);
    assert_eq!(
        config["settings_origins"]["experimental-ownership"],
        "pyproject.toml"
    );
    let (_, disabled, _) = run(&[
        "check",
        "--no-experimental-ownership",
        "--show-settings",
        "--json",
    ]);
    assert_eq!(disabled["experimental_ownership"], false);
    assert_eq!(
        disabled["settings_origins"]["experimental-ownership"],
        "command-line"
    );
    let (_, strict_default, _) = run(&["check", "--no-config", "--mode", "strict", "--json"]);
    assert_eq!(strict_default["assurance"], codepact::ADOPTION_ASSURANCE);
    assert!(strict_default["coverage"]["status"] == "incomplete");
    let (exit, _, _) = run(&["check", "--no-config", "--rules", "TP2002", "--json"]);
    assert_eq!(exit, Some(2));

    let (exit, report, _) = run(&["check", "--json"]);
    assert_eq!(exit, Some(1));
    assert_eq!(report["assurance"], codepact::OWNERSHIP_ASSURANCE);
    assert_eq!(report["counts"]["TP2002"], 1);
    let (exit, _, text) = run(&["check"]);
    assert_eq!(exit, Some(1));
    assert!(text.contains("TP2002"));
    let (exit, _, concise) = run(&["check", "--output-format", "concise"]);
    assert_eq!(exit, Some(1));
    assert!(concise
        .lines()
        .any(|line| line.split('\t').nth(4) == Some("TP2002")));

    let (exit, _, _) = run(&[
        "baseline",
        "update",
        "--no-experimental-ownership",
        "--output",
        "legacy.json",
        "--reason",
        "legacy scope",
        "--json",
    ]);
    assert_eq!(exit, Some(0));
    let (exit, stale, _) = run(&["check", "--baseline", "legacy.json", "--json"]);
    assert_eq!(exit, Some(1), "{stale:#}");
    let (exit, _, _) = run(&[
        "baseline",
        "update",
        "--output",
        "ownership.json",
        "--reason",
        "reviewed fixture",
        "--json",
    ]);
    assert_eq!(exit, Some(0));
    let (exit, baselined, _) = run(&["check", "--baseline", "ownership.json", "--json"]);
    assert_eq!(exit, Some(0), "{baselined:#}");
    assert_eq!(baselined["baseline"]["matched"], 1);
}

#[test]
fn v05_10_handle_limit_is_inclusive_and_overflow_is_visible() {
    for (aliases, expected) in [(4095, false), (4096, true)] {
        let mut body = String::from("def run():\n    a = C()\n");
        for index in 0..aliases {
            body.push_str(&format!("    alias_{index} = a\n"));
        }
        body.push_str("    a.close()\n");
        let result = check(&body);
        assert_eq!(
            result
                .coverage
                .reasons
                .iter()
                .any(|reason| reason.contains("ownership handle budget exceeded")),
            expected
        );
    }
}

#[test]
fn v05_10_capture_limit_preserves_incompleteness_without_affecting_the_next_check() {
    for (captures, expected) in [(128, false), (129, true)] {
        let mut body = String::from(
            "async def run():\n    a = C()\n    try:\n        async with TaskGroup() as group:\n",
        );
        for _ in 0..captures {
            body.push_str("            group.create_task(read(a))\n");
        }
        body.push_str("    finally:\n        a.close()\n");
        let result = group(&body);
        assert_eq!(
            result
                .coverage
                .reasons
                .iter()
                .any(|reason| reason.contains("pending coroutine budget exceeded")),
            expected
        );
    }
    let valid = check("def run():\n    a = C()\n    b = move(a)\n    b.close()\n");
    assert!(valid.diagnostics.is_empty());
}

#[test]
fn v05_04_async_owned_precondition_is_checked_at_coroutine_creation() {
    let result = check("async def take(value: Owned[C]):\n    value.close()\nasync def run():\n    a = C()\n    pending = take(a)\n");
    has(&result, "TP2005");
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v05_11_checked_method_retains_the_callers_task_borrow() {
    let result = group(
        "@resource(closed_by='close')\nclass D:\n    def close(self): pass\n    def read(self): pass\n    def finish(self): self.close()\nasync def worker(a: Borrowed[D]):\n    a.read()\nasync def run():\n    a = D()\n    async with TaskGroup() as group:\n        group.create_task(worker(a))\n        a.finish()\n",
    );
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2006")
        .expect("delegated close conflicts with the outstanding task borrow");
    assert_eq!(finding.certainty, "definite");
    assert_eq!(finding.related.len(), 1);
}

#[test]
fn v05_11_group_target_rebinding_does_not_complete_an_older_coroutine() {
    let result = group(
        "async def run():\n    a = C()\n    pending = read(a)\n    saved = pending\n    try:\n        async with TaskGroup() as pending:\n            await pending\n    finally:\n        a.close()\n",
    );
    has(&result, "TP2006");
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v05_12_task_group_wraps_body_exception_after_waiting() {
    let source = "async def run():\n    a = C()\n    try:\n        async with TaskGroup() as group:\n            group.create_task(read(a))\n            raise ValueError()\n    except HANDLER:\n        a.close()\n";
    let wrong_handler = group(&source.replace("HANDLER", "ValueError"));
    assert!(
        wrong_handler.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.completion.as_deref() == Some("raise")
                && diagnostic.certainty == "definite"
        }),
        "{wrong_handler:#?}"
    );
    let matching_handler = group(&source.replace("HANDLER", "ExceptionGroup"));
    assert!(
        matching_handler.diagnostics.is_empty(),
        "{matching_handler:#?}"
    );
}

#[test]
fn v05_04_non_owning_method_cannot_discharge_a_moved_duty() {
    let result = check(
        "@resource(closed_by='close')\nclass Receiver:\n    def close(self): pass\n    def dispose(self, value): value.close()\ndef run():\n    a = C()\n    receiver = Receiver()\n    receiver.dispose(move(a))\n    receiver.close()\n",
    );
    has(&result, "TP2001");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v05_06_move_retains_the_known_closed_branch() {
    let result =
        check("def run(flag):\n    a = C()\n    if flag:\n        a.close()\n    b = move(a)\n");
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2003")
        .expect("closed predecessor produces a possible transfer conflict");
    assert_eq!(finding.certainty, "possible");
}

#[test]
fn v05_07_lambda_capture_stays_local_until_the_closure_escapes() {
    let source =
        "def observe(a: Borrowed[C]):\n    read_local = lambda: a.read()\n    alias = read_local\n";
    let local = check(&format!(
        "{source}def run():\n    a = C()\n    observe(a)\n    a.close()\n"
    ));
    assert!(local.diagnostics.is_empty(), "{local:#?}");
    let escaped = check(&format!("{source}    return alias\n"));
    has(&escaped, "TP2004");
}

#[test]
fn v05_12_task_capture_checks_apply_at_worker_entry() {
    let result = group(
        "def observe(value): pass\nasync def worker():\n    local = C()\n    observe(local)\n    local.close()\nasync def run():\n    async with TaskGroup() as group:\n        group.create_task(worker())\n",
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    let capture = group(
        "async def worker(value): pass\nasync def run():\n    a = C()\n    async with TaskGroup() as group:\n        group.create_task(worker(a))\n    a.close()\n",
    );
    assert!(
        capture.coverage.reasons.iter().any(|reason| {
            reason.contains("task capture lacks a resolved Borrowed or transferred Owned parameter")
        }),
        "{capture:#?}"
    );
}

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn project(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("codepact-phase04-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}
fn cli(root: &Path, args: &[&str]) -> Output {
    cli_from(root, args)
}

fn cli_from(cwd: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("CLI starts")
}

#[test]
fn must_use_forms_report_discard_and_accept_assignment_and_explicit_discard() {
    let root = project("must-use");
    fs::write(root.join("main.py"),"from typing import Annotated\nfrom codepact import must_use, discard\nfrom codepact.contracts import MustUseResult\nfrom codepact.rust import MustUse\n@must_use\ndef decorated() -> int: return 1\ndef aliased() -> MustUse[int]: return 2\ndef verbose() -> Annotated[int, MustUseResult()]: return 3\ndef bad():\n    decorated()\n    aliased()\n    verbose()\ndef good():\n    a = decorated()\n    _ = aliased()\n    discard(verbose())\n\ndecorated()\n").unwrap();
    let output = cli(
        &root,
        &["check", "--no-config", "--fail-on", "warning", "--json"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["check_schema_version"], 2);
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 4, "{report:#}");
    assert_eq!(findings[0]["severity"], "warning");
    assert_eq!(findings[0]["certainty"], "definite");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn declaration_inspection_keeps_module_expressions_as_coverage_boundaries() {
    let root = project("inspection-module-expression");
    fs::write(root.join("main.py"), "open('resource.txt', 'w').close()\n").unwrap();
    let output = cli(&root, &["inspect-contracts", "main.py"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["coverage"]["status"], "incomplete");
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn misplaced_must_use_metadata_is_rejected_in_parameters_locals_classes_and_decorators() {
    let root = project("must-use-misplacement");
    fs::write(
        root.join("main.py"),
        "from typing import Annotated\nimport codepact.contracts as contracts\nfrom codepact.contracts import MustUseResult\nfrom codepact.rust import MustUse\ndef invalid_parameter(value: MustUse[int]) -> None: pass\ndef invalid_locals(value: int):\n    assigned: MustUse[int] = value\n    unassigned: Annotated[int, MustUseResult()]\n    opaque: list[contracts.MustUseResult() + 1] = []\nclass InvalidClass:\n    field: MustUse[int]\n    @MustUseResult()\n    def method(self): pass\n",
    )
    .unwrap();
    fs::write(
        root.join("decorators.py"),
        "from codepact.contracts import MustUseResult\nfrom codepact.rust import MustUse\n@MustUseResult()\ndef invalid_function_decorator(): pass\n",
    )
    .unwrap();
    fs::write(
        root.join("class_decorator.py"),
        "from codepact.rust import MustUse\n@MustUse\nclass InvalidClassDecorator: pass\n",
    )
    .unwrap();
    let out = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let misplaced = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP4002")
        .collect::<Vec<_>>();
    assert!(misplaced.len() >= 7, "{report:#}");
    assert!(!report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP3001"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_config_strict_mode_and_rule_selection_are_applied() {
    let root = project("config");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("pyproject.toml"),"[tool.codepact]\nconfig-version=1\nroot='src'\npython-version='3.12'\nmode='strict'\nenabled-rules=['TP1001','TP1002','TP1003','TP2001','TP3001']\n").unwrap();
    fs::write(root.join("src/main.py"),"from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef f(flag):\n    c=C()\n    if flag: c.close()\n").unwrap();
    let out = cli(&root, &["check", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["mode"], "strict");
    assert_eq!(report["target_python_version"], "3.12");
    assert_eq!(report["selected_rules"].as_array().unwrap().len(), 5);
    let invalid = cli(&root, &["check", "--mode", "unsound", "--json"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&invalid.stdout).unwrap()["execution_status"],
        "failed"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_resolution_is_read_only_and_reports_origins() {
    let root = project("settings");
    fs::write(
        root.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\npython-version='3.12'\nmode='strict'\n",
    )
    .unwrap();
    fs::write(root.join("broken.py"), "def broken(:\n").unwrap();
    let resolved = cli(&root, &["check", "--show-settings", "--json"]);
    assert_eq!(resolved.status.code(), Some(0));
    let settings: serde_json::Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(settings["execution_status"], "settings-resolved");
    assert_eq!(settings["python_version"], "3.12");
    assert_eq!(settings["settings_origins"]["mode"], "pyproject.toml");
    assert_eq!(settings["settings_origins"]["fail-on"], "default");
    let operand = cli(&root, &["check", "--show-settings", "--json", "broken.py"]);
    assert_eq!(operand.status.code(), Some(2));
    let detail_limit = cli(
        &root,
        &[
            "check",
            "--show-settings",
            "--json",
            "--max-diagnostics",
            "1",
        ],
    );
    assert_eq!(detail_limit.status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn advisory_strict_and_mandatory_severity_follow_policy_contract() {
    let root = project("policy");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f(): fetch()\nimport third_party\n",
    )
    .unwrap();
    let advisory = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(advisory.status.code(), Some(0));
    let advisory_report: serde_json::Value = serde_json::from_slice(&advisory.stdout).unwrap();
    assert_eq!(advisory_report["coverage"]["status"], "incomplete");
    let must_use = advisory_report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "TP3001")
        .unwrap();
    assert_eq!(must_use["effective_severity"], "warning");
    assert_eq!(must_use["actionable"], false);
    let strict = cli(
        &root,
        &["check", "--no-config", "--mode", "strict", "--json"],
    );
    assert_eq!(strict.status.code(), Some(1));
    let strict_report: serde_json::Value = serde_json::from_slice(&strict.stdout).unwrap();
    assert_eq!(strict_report["mode"], "strict");
    assert!(strict_report["policy_failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|failure| failure == "analysis coverage is incomplete"));
    let demote_mandatory = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--severity",
            "TP4002=info",
            "--json",
        ],
    );
    assert_eq!(demote_mandatory.status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn line_suppression_is_reasoned_and_does_not_delete_the_audit_finding() {
    let root = project("suppression");
    fs::write(root.join("main.py"),"from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()  # codepact: ignore[TP3001] -- intentionally ignored\n").unwrap();
    let out = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let finding = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "TP3001")
        .unwrap();
    assert_eq!(finding["disposition"], "suppressed-inline");
    assert_eq!(finding["disposition_reason"], "intentionally ignored");
    fs::write(
        root.join("main.py"),
        "# codepact: ignore[TP9001] -- invalid target\n",
    )
    .unwrap();
    let invalid = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(invalid.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "TP4002"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn baseline_update_is_explicit_and_baseline_only_hides_matching_results() {
    let root = project("baseline");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()\n",
    )
    .unwrap();
    let out = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "initial adoption",
            "--json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["baseline_schema_version"], 1);
    assert!(summary["output_path"]
        .as_str()
        .unwrap()
        .ends_with("baseline.json"));
    let check = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--json",
            "--fail-on",
            "warning",
        ],
    );
    assert_eq!(check.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let finding = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "TP3001")
        .unwrap();
    assert_eq!(finding["disposition"], "suppressed-baseline");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_suppressions_cannot_be_baselined_and_stale_targets_do_not_hide_findings() {
    let root = project("baseline-validation");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()  # codepact: ignore[TP9001] -- invalid category\n",
    )
    .unwrap();
    let update = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "must validate directives first",
        ],
    );
    assert_eq!(update.status.code(), Some(2));
    assert!(!root.join("baseline.json").exists());

    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f(): fetch()\n",
    )
    .unwrap();
    let created = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "initial adoption",
        ],
    );
    assert_eq!(created.status.code(), Some(0));
    let stale = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--python-version",
            "3.12",
            "--fail-on",
            "warning",
            "--json",
        ],
    );
    assert_eq!(stale.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(report["baseline"]["compatible"], false);
    assert!(report["policy_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning.as_str().unwrap().contains("incompatible")));
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP3001" && diagnostic["disposition"] == "active"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn baseline_inputs_and_replace_targets_are_validated_before_source_discovery() {
    let root = project("baseline-preflight");
    fs::write(root.join("README.md"), "no Python files here\n").unwrap();
    fs::write(root.join("read-baseline.json"), "not JSON\n").unwrap();
    let check = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "read-baseline.json",
            "--json",
        ],
    );
    assert_eq!(check.status.code(), Some(2));
    let check_failure: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(check_failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("invalid baseline"));

    fs::write(root.join("replace-baseline.json"), "not JSON\n").unwrap();
    let update = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "replace-baseline.json",
            "--reason",
            "preflight existing state",
            "--replace",
        ],
    );
    assert_eq!(update.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&update.stderr).contains("invalid baseline"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn baseline_matching_keeps_same_named_methods_in_distinct_classes_separate() {
    let root = project("baseline-class-identity");
    let pre_baseline = "from codepact import must_use\n@must_use\ndef fetch(): return 1\nclass A:\n    def run(self):\n        fetch()\nclass B:\n    def run(self):\n        pass\n";
    fs::write(root.join("main.py"), pre_baseline).unwrap();
    let created = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "record A.run",
        ],
    );
    assert_eq!(created.status.code(), Some(0));

    let after = "from codepact import must_use\n@must_use\ndef fetch(): return 1\nclass A:\n    def run(self):\n        pass\nclass B:\n    def run(self):\n        fetch()\n";
    fs::write(root.join("main.py"), after).unwrap();
    let checked = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--fail-on",
            "warning",
            "--json",
        ],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let finding = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "TP3001")
        .unwrap();
    assert_eq!(finding["primary"]["line"], 9);
    assert_eq!(finding["disposition"], "active");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn baseline_fingerprint_distinguishes_identical_methods_on_different_classes() {
    let root = project("baseline-resolved-method");
    let before = "from codepact import must_use\nclass A:\n    @must_use\n    def fetch(self): return 1\nclass B:\n    @must_use\n    def fetch(self): return 1\ndef run():\n    obj = A()\n    obj.fetch()\n";
    fs::write(root.join("main.py"), before).unwrap();
    let created = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "record A.fetch",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let after = before.replace("obj = A()", "obj = B()");
    fs::write(root.join("main.py"), after).unwrap();
    let checked = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--fail-on",
            "warning",
            "--json",
        ],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let finding = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "TP3001")
        .unwrap();
    assert_eq!(finding["disposition"], "active");
    assert_eq!(finding["certainty"], "definite");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn loop_and_context_manager_targets_clear_stale_must_use_aliases() {
    let root = project("must-use-target-rebinding");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run(manager):\n    action = fetch\n    for action in (print,):\n        action()\n    with manager as action:\n        action()\n",
    )
    .unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--fail-on", "warning", "--json"],
    );
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert!(findings.is_empty(), "{report:#}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn break_and_continue_stop_analysis_of_the_unreachable_loop_tail() {
    let root = project("must-use-loop-transfers");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef broken():\n    for item in (1,):\n        break\n        fetch()\n    fetch()\ndef continued():\n    for item in (1,):\n        continue\n        fetch()\n    fetch()\n",
    )
    .unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--fail-on", "warning", "--json"],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let lines = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .map(|diagnostic| diagnostic["primary"]["line"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines, [8, 13], "{report:#}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fstring_expression_text_is_not_a_suppression_comment() {
    let root = project("fstring-suppression-text");
    fs::write(
        root.join("main.py"),
        "value = f\"{\"# codepact: ignore[TP9001] -- harmless string\"}\"\n",
    )
    .unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--python-version", "3.12", "--json"],
    );
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert!(!report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP4002"));
    assert_eq!(report["policy_failures"].as_array().unwrap().len(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn module_must_use_bindings_follow_source_order_and_unreachable_calls_are_ignored() {
    let root = project("must-use-source-order");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\nfetch()\ndef fetch(): return 2\ndef unreachable():\n    return\n    fetch()\n",
    )
    .unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--fail-on", "warning", "--json"],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{report:#}");
    assert_eq!(findings[0]["primary"]["line"], 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_suppressions_remain_completed_errors_and_docstrings_are_ignored() {
    let root = project("suppression-comments");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef invalid():\n    fetch()  # codepact: ignore[TP9001] -- invalid target\ndef documentation_only():\n    \"\"\"\n    # codepact: ignore[TP9001] -- example text\n    \"\"\"\n    return 1\n",
    )
    .unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--fail-on", "warning", "--json"],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["execution_status"], "completed");
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "TP3001" && diagnostic["disposition"] == "active"
    }));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP4002"));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic["primary"]["line"] == 8));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn must_use_tracks_local_captured_and_reexported_callable_identities() {
    let root = project("must-use-aliases");
    fs::write(
        root.join("producer.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef plain(): return 2\n",
    )
    .unwrap();
    fs::write(
        root.join("facade.py"),
        "from producer import fetch as forwarded\n",
    )
    .unwrap();
    fs::write(
        root.join("main.py"),
        "from producer import fetch, plain\nfrom facade import forwarded\ncaptured = fetch\nfetch = plain\ndef f():\n    captured()\n    forwarded()\n    local = captured\n    local()\n    local = plain\n    local()\n",
    )
    .unwrap();
    let out = cli(
        &root,
        &["check", "--no-config", "--json", "--fail-on", "warning"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 3, "{report:#}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn must_use_resolves_known_instance_static_and_class_methods() {
    let root = project("must-use-methods");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\nclass Client:\n    @must_use\n    def read(self) -> int: return 1\n    @staticmethod\n    @must_use\n    def create() -> int: return 2\n    @classmethod\n    @must_use\n    def build(cls) -> int: return 3\n    @staticmethod\n    def helper(receiver): receiver.read()\n    def consume(self): self.read()\ndef f():\n    client = Client()\n    client.read()\n    Client.create()\n    Client.build()\n    client.consume()\n",
    )
    .unwrap();
    let out = cli(
        &root,
        &["check", "--no-config", "--json", "--fail-on", "warning"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 4, "{report:#}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn must_use_reports_unsupported_nested_and_async_uses_without_mislabeling_arguments() {
    let root = project("must-use-boundaries");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use, discard\n@must_use\ndef fetch() -> int: return 1\ndef consume(value): pass\ndef f():\n    consume(fetch())\n    discard([fetch()])\n    value = discard(fetch() + 1)\nasync def g():\n    await fetch()\n",
    )
    .unwrap();
    let out = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP3001"));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic["primary"]["line"] == 6));
    let boundaries = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP9001")
        .collect::<Vec<_>>();
    assert!(boundaries.len() >= 3, "{report:#}");
    assert!(boundaries.iter().any(|diagnostic| diagnostic["message"]
        .as_str()
        .unwrap()
        .contains("explicit discard expression")));
    assert!(boundaries.iter().any(|diagnostic| diagnostic["message"]
        .as_str()
        .unwrap()
        .contains("assignment expression")));
    assert!(boundaries.iter().any(|diagnostic| diagnostic["message"]
        .as_str()
        .unwrap()
        .contains("immediate handling is unsupported")));
    assert_eq!(report["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_rust_check_keeps_its_rule_set_and_schema() {
    let file = codepact::SourceFile::new(
        "main.py",
        "main",
        "from codepact import must_use\n@must_use\ndef fetch() -> int: return 1\ndef f(): fetch()\n",
    );
    let legacy = codepact::check_report(std::slice::from_ref(&file), "3.11");
    assert_eq!(legacy.check_schema_version, 1);
    assert!(!legacy
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "TP3001"));
    let adoption = codepact::check_with_must_use(std::slice::from_ref(&file), "3.11");
    assert!(adoption
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "TP3001"));
}

#[test]
fn baseline_preserves_unambiguous_sites_and_surfaces_inserted_duplicates() {
    let root = project("baseline-ambiguity");
    let baseline_args = [
        "baseline",
        "update",
        "--no-config",
        "--output",
        "baseline.json",
        "--reason",
        "test baseline",
    ];
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()\n",
    )
    .unwrap();
    let created = cli(&root, &baseline_args);
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n# comment and line movement do not change finding identity\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()  # moved harmlessly\n",
    )
    .unwrap();
    let stable = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--json",
        ],
    );
    assert_eq!(
        stable.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&stable.stdout)
    );
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()\n    fetch()\n",
    )
    .unwrap();
    let duplicate = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--json",
            "--fail-on",
            "warning",
        ],
    );
    assert_eq!(
        duplicate.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&duplicate.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&duplicate.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 2);
    assert!(findings
        .iter()
        .all(|diagnostic| diagnostic["disposition"] == "active"));
    assert_eq!(report["baseline"]["matched"], 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn output_policy_keeps_full_counts_and_concise_records_have_seven_fields() {
    let root = project("output-policy");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef f():\n    fetch()\n    fetch()\n",
    )
    .unwrap();
    let limited = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--json",
            "--max-diagnostics",
            "1",
            "--fail-on",
            "warning",
        ],
    );
    assert_eq!(limited.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&limited.stdout).unwrap();
    assert_eq!(report["total_diagnostics"], 2);
    assert_eq!(report["returned_diagnostics"], 1);
    assert_eq!(report["actionable_diagnostics"], 2);
    assert_eq!(report["details_truncated"], true);
    let concise = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--output-format",
            "concise",
            "--fail-on",
            "warning",
        ],
    );
    assert_eq!(concise.status.code(), Some(1));
    let records = String::from_utf8(concise.stdout).unwrap();
    assert_eq!(records.lines().count(), 4);
    assert!(records.lines().all(|line| line.split('\t').count() == 7));
    let conflict = cli(
        &root,
        &["check", "--no-config", "--json", "--output-format", "text"],
    );
    assert_eq!(conflict.status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn capabilities_and_rule_explanations_publish_the_supported_surface_and_limits() {
    let root = project("capabilities-and-explanations");
    let output = cli(&root, &["capabilities", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let capabilities: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capabilities["capability_schema_version"], 1);
    assert_eq!(capabilities["checker_schema_version"], 2);
    assert_eq!(capabilities["inspection_schema_version"], 0);
    assert_eq!(
        capabilities["python_targets"],
        serde_json::json!(["3.11", "3.12", "3.13"])
    );
    assert_eq!(capabilities["limits"]["config_bytes"], 65_536);
    assert_eq!(capabilities["limits"]["baseline_bytes"], 16_777_216);
    assert_eq!(capabilities["limits"]["baseline_entries"], 100_000);
    assert_eq!(capabilities["limits"]["report_bytes"], 33_554_432);
    assert_eq!(capabilities["limits"]["related_sites_per_finding"], 32);
    assert_eq!(
        capabilities["limits"]["source_snippet_bytes_per_finding"],
        2048
    );
    assert_eq!(
        capabilities["limits"]["explanation_bytes_per_finding"],
        8192
    );
    assert_eq!(capabilities["services"]["pagination"], false);
    assert_eq!(capabilities["services"]["mcp"], false);
    assert_eq!(capabilities["services"]["language_server"], false);
    assert_eq!(capabilities["services"]["persistent_cache"], false);

    let rules = capabilities["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 9);
    for rule in rules {
        let code = rule["code"].as_str().unwrap();
        let explanation = cli(&root, &["explain", code, "--json"]);
        assert_eq!(explanation.status.code(), Some(0), "{code}");
        let document: serde_json::Value = serde_json::from_slice(&explanation.stdout).unwrap();
        assert_eq!(document["explanation_schema_version"], 1, "{code}");
        assert_eq!(document["code"], code, "{code}");
        assert_eq!(document["explanation_id"], rule["explanation_id"], "{code}");
        assert_eq!(document["automatic_fix"], false, "{code}");
        for example in ["valid", "invalid", "unknown"] {
            assert!(
                document["examples"][example]
                    .as_str()
                    .is_some_and(|text| !text.is_empty()),
                "{code} {example}"
            );
        }
        assert!(
            !document["preconditions"].as_array().unwrap().is_empty(),
            "{code}"
        );
        assert!(
            !document["limitations"].as_array().unwrap().is_empty(),
            "{code}"
        );
    }
    let unknown = cli(&root, &["explain", "TP9999", "--json"]);
    assert_eq!(unknown.status.code(), Some(2));
    let failed: serde_json::Value = serde_json::from_slice(&unknown.stdout).unwrap();
    assert_eq!(failed["execution_status"], "failed");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn nearest_config_wins_and_invalid_project_settings_fail_before_analysis() {
    let root = project("config-precedence");
    let outer = root.join("outer");
    let nested = outer.join("packages/one");
    let cwd = nested.join("deep");
    fs::create_dir_all(&cwd).unwrap();
    fs::write(
        outer.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\nmode='strict'\npython-version='3.13'\n",
    )
    .unwrap();
    fs::write(
        nested.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\nmode='advisory'\n",
    )
    .unwrap();

    let resolved = cli_from(&cwd, &["check", "--show-settings", "--json"]);
    assert_eq!(resolved.status.code(), Some(0));
    let settings: serde_json::Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(settings["mode"], "advisory");
    assert_eq!(settings["python_version"], "3.11");
    assert_eq!(settings["settings_origins"]["mode"], "pyproject.toml");
    assert_eq!(settings["settings_origins"]["python-version"], "default");
    assert_eq!(
        settings["root"],
        nested.canonicalize().unwrap().to_string_lossy().as_ref()
    );
    assert!(settings["config_file"]
        .as_str()
        .unwrap()
        .ends_with("packages/one/pyproject.toml"));

    for config in [
        "[tool.codepact]\nconfig-version=1\nunknown-key=true\n",
        "[tool.codepact]\nmode='strict'\n",
        "[tool.codepact]\nconfig-version=2\n",
        "[tool.codepact]\nconfig-version=1\npython-version='3.10'\n",
        "[tool.codepact]\nconfig-version=1\nenabled-rules=['TP9999']\n",
        "[tool.codepact]\nconfig-version=1\ninclude=['../outside.py']\n",
        "[tool.codepact]\nconfig-version=1\nmode='unknown'\n",
        "[tool.codepact]\nconfig-version=1\nenabled-rules=['TP3001','TP3001']\n",
        "[tool.codepact]\nconfig-version=1\n[tool.codepact.severity]\nTP9001='info'\n",
        "[tool.codepact]\nconfig-version=1\n[tool.codepact.severity]\nTP3001='off'\n",
        "[tool.codepact]\nconfig-version=1\nmode='strict'\nmode='advisory'\n",
    ] {
        fs::write(nested.join("pyproject.toml"), config).unwrap();
        let invalid = cli_from(&cwd, &["check", "--json"]);
        assert_eq!(invalid.status.code(), Some(2), "{config}");
        let failure: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
        assert_eq!(failure["execution_status"], "failed", "{config}");
        assert!(failure["error"]["message"].as_str().is_some(), "{config}");
    }
    fs::write(
        nested.join("pyproject.toml"),
        "[tool.other]\nunknown='accepted'\n[tool.codepact]\nconfig-version=1\n",
    )
    .unwrap();
    let unrelated = cli_from(&cwd, &["check", "--show-settings", "--json"]);
    assert_eq!(unrelated.status.code(), Some(0));
    let conflict = cli_from(
        &cwd,
        &[
            "check",
            "--config",
            "pyproject.toml",
            "--no-config",
            "--json",
        ],
    );
    assert_eq!(conflict.status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_config_and_root_overrides_have_one_deterministic_precedence() {
    let root = project("config-explicit-precedence");
    let workspace = root.join("workspace");
    let nested = workspace.join("package");
    let cwd = nested.join("deep");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(nested.join("src")).unwrap();
    fs::create_dir_all(workspace.join("outer-src")).unwrap();
    fs::write(
        workspace.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\nmode='strict'\npython-version='3.13'\nroot='outer-src'\n",
    )
    .unwrap();
    fs::write(
        nested.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\nmode='advisory'\npython-version='3.12'\nroot='src'\n",
    )
    .unwrap();
    fs::write(nested.join("src/main.py"), "value = 1\n").unwrap();

    let nearest = cli_from(&cwd, &["check", "--show-settings", "--json"]);
    assert_eq!(nearest.status.code(), Some(0));
    let nearest: serde_json::Value = serde_json::from_slice(&nearest.stdout).unwrap();
    assert_eq!(nearest["mode"], "advisory");
    assert_eq!(nearest["python_version"], "3.12");
    assert_eq!(
        nearest["root"],
        nested
            .join("src")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert!(nearest["config_file"]
        .as_str()
        .unwrap()
        .ends_with("package/pyproject.toml"));

    let no_config = cli_from(&cwd, &["check", "--show-settings", "--no-config", "--json"]);
    assert_eq!(no_config.status.code(), Some(0));
    let no_config: serde_json::Value = serde_json::from_slice(&no_config.stdout).unwrap();
    assert_eq!(no_config["mode"], "advisory");
    assert_eq!(no_config["python_version"], "3.11");
    assert_eq!(no_config["config_file"], serde_json::Value::Null);
    assert_eq!(
        no_config["root"],
        cwd.canonicalize().unwrap().to_string_lossy().as_ref()
    );

    let explicit_relative = cli_from(
        &cwd,
        &[
            "check",
            "--show-settings",
            "--config",
            "../pyproject.toml",
            "--json",
        ],
    );
    assert_eq!(explicit_relative.status.code(), Some(0));
    let explicit_relative: serde_json::Value =
        serde_json::from_slice(&explicit_relative.stdout).unwrap();
    assert_eq!(explicit_relative["mode"], "advisory");
    assert_eq!(explicit_relative["python_version"], "3.12");
    assert_eq!(
        explicit_relative["root"],
        nested
            .join("src")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );

    let absolute_config_path = nested.join("pyproject.toml");
    let absolute_config = cli_from(
        &cwd,
        &[
            "check",
            "--show-settings",
            "--config",
            absolute_config_path.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(absolute_config.status.code(), Some(0));
    let absolute_config: serde_json::Value =
        serde_json::from_slice(&absolute_config.stdout).unwrap();
    assert_eq!(
        absolute_config["config_file"],
        absolute_config_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );

    let absolute_root_path = nested.join("src");
    let absolute_root = cli_from(
        &cwd,
        &[
            "check",
            "--show-settings",
            "--root",
            absolute_root_path.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(absolute_root.status.code(), Some(0));
    let absolute_root: serde_json::Value = serde_json::from_slice(&absolute_root.stdout).unwrap();
    assert_eq!(
        absolute_root["root"],
        absolute_root_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(absolute_root["settings_origins"]["root"], "command-line");

    let root_override = cli_from(
        &cwd,
        &["check", "--show-settings", "--root", "../src", "--json"],
    );
    assert_eq!(root_override.status.code(), Some(0));
    let root_override: serde_json::Value = serde_json::from_slice(&root_override.stdout).unwrap();
    assert_eq!(root_override["mode"], "advisory");
    assert_eq!(root_override["python_version"], "3.12");
    assert_eq!(
        root_override["root"],
        nested
            .join("src")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(root_override["settings_origins"]["root"], "command-line");
    assert!(root_override["config_file"]
        .as_str()
        .unwrap()
        .ends_with("package/pyproject.toml"));

    let missing_root = cli_from(
        &cwd,
        &[
            "check",
            "--show-settings",
            "--root",
            "missing-project-root",
            "--json",
        ],
    );
    assert_eq!(missing_root.status.code(), Some(2));
    let missing_root: serde_json::Value = serde_json::from_slice(&missing_root.stdout).unwrap();
    assert_eq!(missing_root["execution_status"], "failed");
    assert!(missing_root["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cannot resolve project root"));

    #[cfg(unix)]
    {
        let symlink_root = workspace.join("src-link");
        std::os::unix::fs::symlink(nested.join("src"), &symlink_root).unwrap();
        let linked_settings = cli_from(
            &cwd,
            &[
                "check",
                "--show-settings",
                "--root",
                symlink_root.to_str().unwrap(),
                "--json",
            ],
        );
        assert_eq!(linked_settings.status.code(), Some(2));
        let linked_settings: serde_json::Value =
            serde_json::from_slice(&linked_settings.stdout).unwrap();
        assert_eq!(linked_settings["execution_status"], "failed");
        assert!(linked_settings["error"]["message"]
            .as_str()
            .unwrap()
            .contains("symlink component"));

        let symlink_config = workspace.join("pyproject-link.toml");
        std::os::unix::fs::symlink(nested.join("pyproject.toml"), &symlink_config).unwrap();
        let linked_config = cli_from(
            &cwd,
            &[
                "check",
                "--show-settings",
                "--config",
                symlink_config.to_str().unwrap(),
                "--json",
            ],
        );
        assert_eq!(linked_config.status.code(), Some(2));
        let linked_config: serde_json::Value =
            serde_json::from_slice(&linked_config.stdout).unwrap();
        assert_eq!(linked_config["execution_status"], "failed");
        assert!(linked_config["error"]["message"]
            .as_str()
            .unwrap()
            .contains("symlink component"));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_config_wrong_types_and_invalid_values_fail_with_actionable_errors() {
    let root = project("config-wrong-types");
    fs::write(
        root.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\n",
    )
    .unwrap();

    for config in [
        "[tool.codepact]\nconfig-version='1'\n",
        "[tool.codepact]\nconfig-version=1\nroot=1\n",
        "[tool.codepact]\nconfig-version=1\ninclude='src/**/*.py'\n",
        "[tool.codepact]\nconfig-version=1\nexclude=[1]\n",
        "[tool.codepact]\nconfig-version=1\nenabled-rules='TP3001'\n",
        "[tool.codepact]\nconfig-version=1\nseverity='warning'\n",
        "[tool.codepact]\nconfig-version=1\nfail-on='info'\n",
        "[tool.codepact]\nconfig-version=1\ninclude=['src/../outside.py']\n",
        "[tool.codepact]\nconfig-version=1\npython-version='3.14'\n",
        "[tool.codepact]\nconfig-version=1\n[tool.codepact.severity]\nTP3001='fatal'\n",
    ] {
        fs::write(root.join("pyproject.toml"), config).unwrap();
        let invalid = cli(&root, &["check", "--json"]);
        assert_eq!(invalid.status.code(), Some(2), "{config}");
        let failure: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
        assert_eq!(failure["execution_status"], "failed", "{config}");
        let message = failure["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.is_empty(), "{config}: {failure:#}");
        assert!(
            failure.get("diagnostics").is_none(),
            "{config}: {failure:#}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn includes_and_excludes_define_scope_and_explicit_operands_cannot_bypass_excludes() {
    let root = project("include-exclude");
    fs::create_dir_all(root.join("src/generated")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[tool.codepact]\nconfig-version=1\ninclude=['src/**/*.py']\nexclude=['src/generated/**']\n",
    )
    .unwrap();
    fs::write(root.join("src/good.py"), "value = 1\n").unwrap();
    fs::write(root.join("src/generated/ignored.py"), "def broken(:\n").unwrap();
    fs::write(root.join("tests/ignored.py"), "def broken(:\n").unwrap();

    let checked = cli(&root, &["check", "--json"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["selected_files"], serde_json::json!(["src/good.py"]));
    assert_eq!(report["coverage"]["source_bytes"], 10);

    let explicit_excluded = cli(&root, &["check", "--json", "src/generated/ignored.py"]);
    assert_eq!(explicit_excluded.status.code(), Some(2));
    let failure: serde_json::Value = serde_json::from_slice(&explicit_excluded.stdout).unwrap();
    assert!(failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("excluded"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn policy_truth_table_keeps_severity_certainty_coverage_and_rule_selection_separate() {
    let root = project("policy-truth-table");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run(): fetch()\n",
    )
    .unwrap();

    for (mode, threshold, severity, expected_exit) in [
        ("advisory", "error", None, 0),
        ("strict", "error", None, 0),
        ("advisory", "warning", None, 1),
        ("strict", "warning", None, 1),
        ("advisory", "error", Some("TP3001=info"), 0),
        ("strict", "error", Some("TP3001=info"), 0),
    ] {
        let mut args = vec![
            "check".to_string(),
            "--no-config".to_string(),
            "--json".to_string(),
            "--mode".to_string(),
            mode.to_string(),
            "--fail-on".to_string(),
            threshold.to_string(),
        ];
        if let Some(severity) = severity {
            args.push("--severity".into());
            args.push(severity.into());
        }
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let checked = cli(&root, &refs);
        assert_eq!(checked.status.code(), Some(expected_exit), "{args:?}");
        let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
        assert_eq!(report["execution_status"], "completed");
        assert_eq!(report["mode"], mode);
        let finding = report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["code"] == "TP3001")
            .unwrap();
        assert_eq!(finding["certainty"], "definite");
        assert_eq!(finding["base_severity"], "warning");
        assert_eq!(finding["actionable"], expected_exit == 1);
    }

    let disabled = cli(
        &root,
        &["check", "--no-config", "--json", "--rules", "TP1002"],
    );
    assert_eq!(disabled.status.code(), Some(0));
    let disabled_report: serde_json::Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert_eq!(
        disabled_report["selected_rules"],
        serde_json::json!(["TP1002"])
    );
    assert_eq!(disabled_report["counts_by_disposition"]["rule-disabled"], 1);
    assert!(!disabled_report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP3001" && diagnostic["actionable"] == true));

    fs::write(root.join("main.py"), "def run():\n    unknown_effect()\n").unwrap();
    let incomplete = cli(
        &root,
        &["check", "--no-config", "--json", "--mode", "strict"],
    );
    assert_eq!(incomplete.status.code(), Some(1));
    let incomplete_report: serde_json::Value = serde_json::from_slice(&incomplete.stdout).unwrap();
    assert_eq!(incomplete_report["coverage"]["status"], "incomplete");
    assert!(incomplete_report["policy_failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|failure| failure == "analysis coverage is incomplete"));
    assert!(incomplete_report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001" && diagnostic["certainty"] == "unknown"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn suppression_targets_only_the_primary_start_line_and_retains_raw_findings() {
    let root = project("suppression-primary-line");
    let source = "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run():\n    fetch()  # codepact: ignore[TP3001] -- reviewed intentional discard\n    fetch()\n";
    fs::write(root.join("main.py"), source).unwrap();
    let checked = cli(
        &root,
        &["check", "--no-config", "--json", "--fail-on", "warning"],
    );
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let findings = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP3001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0]["disposition"], "suppressed-inline");
    assert_eq!(
        findings[0]["disposition_reason"],
        "reviewed intentional discard"
    );
    assert_eq!(findings[1]["disposition"], "active");

    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run():\n    fetch(\n    )  # codepact: ignore[TP3001] -- comment is not on the primary start line\n",
    )
    .unwrap();
    let multiline = cli(
        &root,
        &["check", "--no-config", "--json", "--fail-on", "warning"],
    );
    assert_eq!(multiline.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&multiline.stdout).unwrap();
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP3001" && diagnostic["disposition"] == "active"
        }));
    assert!(!report["unused_suppressions"].as_array().unwrap().is_empty());

    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run():\n    fetch()  # codepact: ignore[TP3001] --\n",
    )
    .unwrap();
    let invalid = cli(&root, &["check", "--no-config", "--json"]);
    assert_eq!(invalid.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| { diagnostic["code"] == "TP4002" && diagnostic["severity"] == "error" }));
    assert!(report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP3001" && diagnostic["disposition"] == "active"
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn baseline_rejects_unsupported_versions_and_duplicate_entries_without_hiding_findings() {
    let root = project("baseline-version-and-duplicates");
    fs::write(
        root.join("main.py"),
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run(): fetch()\n",
    )
    .unwrap();
    let create = cli(
        &root,
        &[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "record existing must-use finding",
        ],
    );
    assert_eq!(create.status.code(), Some(0));
    let original = fs::read_to_string(root.join("baseline.json")).unwrap();
    let original_json: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(original_json["entries"].as_array().unwrap().len(), 1);

    let mut unsupported = original_json.clone();
    unsupported["baseline_schema_version"] = serde_json::json!(2);
    fs::write(
        root.join("baseline.json"),
        serde_json::to_vec(&unsupported).unwrap(),
    )
    .unwrap();
    let invalid_version = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--json",
        ],
    );
    assert_eq!(invalid_version.status.code(), Some(2));
    let failure: serde_json::Value = serde_json::from_slice(&invalid_version.stdout).unwrap();
    assert!(failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unsupported baseline schema"));

    let mut duplicate = original_json;
    let entry = duplicate["entries"][0].clone();
    duplicate["entries"].as_array_mut().unwrap().push(entry);
    fs::write(
        root.join("baseline.json"),
        serde_json::to_vec(&duplicate).unwrap(),
    )
    .unwrap();
    let duplicate_entry = cli(
        &root,
        &[
            "check",
            "--no-config",
            "--baseline",
            "baseline.json",
            "--json",
        ],
    );
    assert_eq!(duplicate_entry.status.code(), Some(2));
    let failure: serde_json::Value = serde_json::from_slice(&duplicate_entry.stdout).unwrap();
    assert!(failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("duplicate entries"));
    fs::remove_dir_all(root).unwrap();
}

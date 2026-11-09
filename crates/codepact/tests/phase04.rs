use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

fn project(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("codepact-phase04-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}
fn cli(root: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(root)
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

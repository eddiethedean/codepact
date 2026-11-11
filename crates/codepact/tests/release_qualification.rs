//! Independent acceptance controls for the 0.4 CLI, including negative controls.
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
static REPORT: AtomicUsize = AtomicUsize::new(0);
fn binary() -> PathBuf {
    std::env::var_os("CODEPACT_QUALIFICATION_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codepact")))
}
struct Project(PathBuf);
impl Project {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codepact-qualification-{}-{label}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(binary())
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn check(&self, extra: &[&str], exit: i32) -> Value {
        let mut args = vec!["check", "--no-config", "--json"];
        args.extend(extra);
        let result = self.run(&args);
        assert_eq!(
            result.status.code(),
            Some(exit),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        if let Some(directory) = std::env::var_os("CODEPACT_QUALIFICATION_EVIDENCE") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            let record = json!({
                "arguments": args,
                "expected_exit": exit,
                "exit": result.status.code(),
                "stdout": String::from_utf8_lossy(&result.stdout),
                "stderr": String::from_utf8_lossy(&result.stderr),
            });
            let number = REPORT.fetch_add(1, Ordering::Relaxed);
            fs::write(
                directory.join(format!("report-{number:04}.json")),
                serde_json::to_vec_pretty(&record).unwrap(),
            )
            .unwrap();
        }
        serde_json::from_slice(&result.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn findings(report: &Value, code: &str) -> Vec<Value> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == code)
        .cloned()
        .collect()
}
const MARKED:&str="from codepact import must_use\n@must_use\ndef fetch() -> int: return 1\ndef run():\n    fetch()\n";

#[test]
fn v04_01_equivalent_invocations_keep_config_and_path_origins() {
    let p = Project::new("config-origins");
    p.write(
        "pyproject.toml",
        "[tool.codepact]\nconfig-version=1\nroot='src'\nmode='strict'\n",
    );
    p.write("src/main.py", "pass\n");
    fs::create_dir_all(p.0.join("src/deep")).unwrap();
    let check = |cwd: &Path, args: &[&str]| -> Value {
        let out = Command::new(binary())
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let root = check(&p.0, &["check", "--show-settings", "--json"]);
    let nested = check(
        &p.0.join("src/deep"),
        &["check", "--show-settings", "--json"],
    );
    assert_eq!(root, nested);
    let config = p.0.join("pyproject.toml");
    let explicit = check(
        &p.0.join("src/deep"),
        &[
            "check",
            "--config",
            config.to_str().unwrap(),
            "--show-settings",
            "--json",
        ],
    );
    assert_eq!(root, explicit);
    assert_eq!(root["settings_origins"]["mode"], "pyproject.toml");
    assert_eq!(root["settings_origins"]["python-version"], "default");
    p.write(
        "src/deep/pyproject.toml",
        "[tool.codepact]\nconfig-version=1\n",
    );
    let nearest = check(
        &p.0.join("src/deep"),
        &["check", "--show-settings", "--json"],
    );
    assert_eq!(nearest["mode"], "advisory"); // ancestor strict must not leak through.
    let defaults = check(&p.0, &["check", "--no-config", "--show-settings", "--json"]);
    assert_eq!(defaults["mode"], "advisory");
}

#[test]
fn v04_03_04_scope_deduplicates_and_rejects_empty_collision_and_escapes() {
    let p = Project::new("scope");
    p.write("pkg/__init__.py", "");
    p.write("pkg/a.py", MARKED);
    p.write("other/a.py", "pass\n");
    p.write(".git/hidden.py", "not Python !");
    p.write(".venv/hidden.py", "not Python !");
    p.write("__pycache__/hidden.py", "not Python !");
    let report = p.check(&["--fail-on", "warning"], 1);
    assert_eq!(
        report["selected_files"],
        json!(["other/a.py", "pkg/__init__.py", "pkg/a.py"])
    );
    let absolute = p.0.join("pkg/a.py");
    let unique = p.check(
        &[
            "pkg/a.py",
            absolute.to_str().unwrap(),
            "pkg/./a.py",
            "--fail-on",
            "warning",
        ],
        1,
    );
    assert_eq!(unique["selected_files"], report["selected_files"]);
    assert_eq!(findings(&unique, "TP3001").len(), 1);
    p.write("pkg.py", "pass\n");
    let collision = p.check(&[], 2);
    assert!(collision["error"]["message"]
        .as_str()
        .unwrap()
        .contains("module pkg"));
    fs::remove_file(p.0.join("pkg.py")).unwrap();
    p.write(
        "pyproject.toml",
        "[tool.codepact]\nconfig-version=1\ninclude=['missing/**/*.py']\n",
    );
    let empty = p.run(&["check", "--json"]);
    assert_eq!(empty.status.code(), Some(2));
    let outside = Project::new("outside");
    outside.write("outside.py", MARKED);
    assert_eq!(
        p.check(&[outside.0.join("outside.py").to_str().unwrap()], 2)["execution_status"],
        "failed"
    );
    p.write("invalid-name.py", "pass\n");
    assert_eq!(p.check(&[], 2)["execution_status"], "failed");
}

#[cfg(unix)]
#[test]
fn v04_04_symlink_inputs_and_baseline_destinations_cannot_escape() {
    use std::os::unix::fs::symlink;
    let p = Project::new("links");
    let outside = Project::new("links-outside");
    outside.write("secret.py", MARKED);
    p.write("main.py", "pass\n");
    symlink(outside.0.join("secret.py"), p.0.join("linked.py")).unwrap();
    assert_eq!(p.check(&["linked.py"], 2)["execution_status"], "failed");
    fs::remove_file(p.0.join("linked.py")).unwrap();
    symlink(&outside.0, p.0.join("linked")).unwrap();
    assert_eq!(
        p.check(&["linked/secret.py"], 2)["execution_status"],
        "failed"
    );
    // Discovery skips symlinks instead of following an outside tree.
    assert_eq!(p.check(&[], 0)["selected_files"], json!(["main.py"]));
    let out = p.run(&[
        "baseline",
        "update",
        "--no-config",
        "--output",
        "linked/baseline.json",
        "--reason",
        "escape attempt",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!outside.0.join("baseline.json").exists());
}

#[test]
fn v04_08_complete_definite_policy_cross_product() {
    let p = Project::new("policy");
    p.write("main.py", MARKED);
    for mode in ["advisory", "strict"] {
        for threshold in ["error", "warning"] {
            for severity in ["error", "warning", "info"] {
                for enabled in [true, false] {
                    let expected = enabled
                        && (severity == "error" || severity == "warning" && threshold == "warning");
                    let assignment = format!("TP3001={severity}");
                    let r = p.check(
                        &[
                            "--mode",
                            mode,
                            "--fail-on",
                            threshold,
                            "--severity",
                            &assignment,
                            "--rules",
                            if enabled { "TP3001" } else { "TP1001" },
                        ],
                        if enabled { i32::from(expected) } else { 2 },
                    );
                    if !enabled {
                        assert_eq!(r["execution_status"], "failed");
                        let disabled = p.check(
                            &["--mode", mode, "--fail-on", threshold, "--rules", "TP1001"],
                            0,
                        );
                        assert_eq!(
                            findings(&disabled, "TP3001")[0]["disposition"],
                            "rule-disabled"
                        );
                        continue;
                    }
                    let d = findings(&r, "TP3001");
                    assert_eq!(d.len(), 1);
                    assert_eq!(d[0]["certainty"], "definite");
                    assert_eq!(d[0]["effective_severity"], severity);
                    assert_eq!(d[0]["actionable"], expected);
                    assert_eq!(r["coverage"]["status"], "complete-for-supported-subset");
                }
            }
        }
    }
    // Inline acknowledgement cannot hide strict coverage failures.
    p.write(
        "main.py",
        &format!(
            "{}\nunknown_effect()\n",
            MARKED.replace(
                "    fetch()",
                "    fetch() # codepact: ignore[TP3001] -- reviewed"
            )
        ),
    );
    let strict = p.check(&["--mode", "strict", "--severity", "TP3001=info"], 1);
    assert_eq!(
        findings(&strict, "TP3001")[0]["disposition"],
        "suppressed-inline"
    );
    assert_eq!(strict["coverage"]["status"], "incomplete");
    assert!(strict["policy_failures"]
        .as_array()
        .unwrap()
        .contains(&json!("analysis coverage is incomplete")));
}

#[test]
fn v04_09_exact_multiple_unused_and_invalid_suppressions() {
    let p = Project::new("suppressions");
    p.write(
        "main.py",
        &MARKED.replace(
            "    fetch()",
            "    fetch() # codepact: ignore[TP3001,TP1001] -- accepted result only",
        ),
    );
    let r = p.check(&["--fail-on", "warning"], 0);
    assert_eq!(
        findings(&r, "TP3001")[0]["disposition"],
        "suppressed-inline"
    );
    p.write(
        "main.py",
        "value = 1 # codepact: ignore[TP3001] -- unused acknowledgement\n",
    );
    let unused = p.check(&[], 0);
    assert_eq!(unused["unused_suppressions"].as_array().unwrap().len(), 1);
    for directive in [
        "ignore[TP9001] -- invalid boundary",
        "ignore[TP3001]",
        "ignore[TP3001] -- ",
        "ignore[TP3001,TP3001] -- duplicate",
        "ignore[TP9999] -- unknown",
        "ignore[] -- empty",
    ] {
        p.write(
            "main.py",
            &MARKED.replace(
                "    fetch()",
                &format!("    fetch() # codepact: {directive}"),
            ),
        );
        let r = p.check(&["--rules", "TP1001"], 1);
        let errors = findings(&r, "TP4002");
        assert!(!errors.is_empty(), "{directive}");
        assert!(errors
            .iter()
            .all(|d| d["disposition"] == "active" && d["effective_severity"] == "error"));
    }
}

#[test]
fn v04_12_cross_all_producer_and_immediate_consumer_forms() {
    let p = Project::new("mustuse-cross");
    let header="from typing import Annotated\nfrom codepact import must_use, discard\nfrom codepact.rust import MustUse\nfrom codepact.contracts import MustUseResult\n";
    for producer in [
        "@must_use\ndef fetch() -> int: return 1\n",
        "def fetch() -> MustUse[int]: return 1\n",
        "def fetch() -> Annotated[int, MustUseResult()]: return 1\n",
        "@must_use\ndef fetch() -> Annotated[int, MustUseResult(), MustUseResult()]: return 1\n",
    ] {
        for consumer in [
            "fetch()",
            "value = fetch()",
            "_ = fetch()",
            "return fetch()",
            "discard(fetch())",
            "accept(fetch())",
        ] {
            let source =
                format!("{header}{producer}def accept(value): pass\ndef run():\n    {consumer}\n");
            p.write("main.py", &source);
            let r = p.check(&["--fail-on", "warning"], i32::from(consumer == "fetch()"));
            assert_eq!(
                findings(&r, "TP3001").len(),
                usize::from(consumer == "fetch()"),
                "{producer}\n{consumer}\n{r}"
            );
            assert_eq!(
                r["coverage"]["status"], "complete-for-supported-subset",
                "{producer}\n{consumer}\n{r}"
            );
        }
    }
}

#[test]
fn v04_14_explicit_discard_preserves_cleanup_and_argument_raises() {
    let p = Project::new("discard-effects");
    let header="from codepact import must_use, resource, discard\nfrom codepact.rust import Owned\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n@must_use\ndef acquire() -> Owned[Connection]: return Connection()\n";
    for statement in ["acquire()", "discard(acquire())"] {
        p.write("main.py", &format!("{header}def run():\n    {statement}\n"));
        let r = p.check(&["--fail-on", "warning"], 1);
        assert_eq!(
            findings(&r, "TP3001").len(),
            usize::from(statement == "acquire()")
        );
        assert!(!findings(&r, "TP1002").is_empty(), "{r}");
    }
    p.write("main.py",&format!("{header}def explode(): raise RuntimeError()\ndef run():\n    conn = Connection()\n    discard(explode())\n    conn.close()\n"));
    let r = p.check(&[], 1);
    assert!(
        findings(&r, "TP1002")
            .iter()
            .any(|d| d["completion"] == "raise"),
        "{r}"
    );
}

#[test]
fn v04_19_hidden_findings_keep_full_exit_counts_and_snapshot() {
    let p = Project::new("limits");
    p.write(
        "main.py",
        &format!("{MARKED}def second():\n    fetch()\ndef third():\n    fetch()\n"),
    );
    let full = p.check(&["--fail-on", "warning"], 1);
    let limited = p.check(&["--fail-on", "warning", "--max-diagnostics", "1"], 1);
    assert_eq!(full["snapshot_id"], limited["snapshot_id"]);
    assert_eq!(limited["total_diagnostics"], 3);
    assert_eq!(limited["actionable_diagnostics"], 3);
    assert_eq!(limited["returned_diagnostics"], 1);
    assert_eq!(limited["details_truncated"], true);
    assert_eq!(
        p.check(&["--max-diagnostics", "0"], 2)["execution_status"],
        "failed"
    );
}

#[cfg(unix)]
#[test]
fn v04_19_broken_output_is_an_execution_failure() {
    use std::process::Stdio;
    let p = Project::new("broken-output");
    p.write("main.py", MARKED);
    let mut child = Command::new(binary())
        .current_dir(&p.0)
        .args(["check", "--no-config", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot write"));
}

#[test]
fn v04_22_24_dependency_repair_preserves_policy_and_never_executes_source() {
    let p = Project::new("repair");
    p.write("AGENTS.md", "instruction text must remain unchanged\n");
    p.write(
        "pyproject.toml",
        "[tool.codepact]\nconfig-version=1\nmode='advisory'\n",
    );
    let provider="from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 1\ndef shutdown(conn):\n    conn.close()\n";
    p.write("provider.py", provider);
    p.write("main.py","from provider import Connection, shutdown\nopen('executed','w')\ndef run():\n    conn = Connection()\n    shutdown(conn)\n    conn.status()\n");
    let agents = fs::read(p.0.join("AGENTS.md")).unwrap();
    let config = fs::read(p.0.join("pyproject.toml")).unwrap();
    let before = p.check(&[], 1);
    assert_eq!(findings(&before, "TP1001").len(), 1);
    p.write(
        "provider.py",
        &provider.replace("    conn.close()", "    pass"),
    );
    let after = p.check(&[], 1);
    assert!(findings(&after, "TP1001").is_empty());
    assert!(!findings(&after, "TP1002").is_empty()); // wrong repair still leaks.
    p.write("provider.py", provider);
    p.write("main.py","from provider import Connection, shutdown\nopen('executed','w')\ndef run():\n    conn = Connection()\n    conn.status()\n    shutdown(conn)\n");
    let repaired = p.check(&[], 0);
    assert!(findings(&repaired, "TP1001").is_empty());
    assert!(findings(&repaired, "TP1002").is_empty());
    assert_eq!(before["policy_fingerprint"], repaired["policy_fingerprint"]);
    assert!(!p.0.join("executed").exists());
    assert_eq!(fs::read(p.0.join("AGENTS.md")).unwrap(), agents);
    assert_eq!(fs::read(p.0.join("pyproject.toml")).unwrap(), config);
}

#[test]
fn v04_08_possible_policy_cross_product_and_mandatory_errors() {
    let p = Project::new("possible-policy");
    p.write("main.py","from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 1\ndef run(flag):\n    c=C()\n    if flag: c.close()\n    c.status()\n    c.close()\n");
    for mode in ["advisory", "strict"] {
        for threshold in ["error", "warning"] {
            for severity in ["error", "warning", "info"] {
                let expected = mode == "strict"
                    || severity == "error"
                    || severity == "warning" && threshold == "warning";
                let setting = format!("TP1001={severity}");
                let r = p.check(
                    &[
                        "--mode",
                        mode,
                        "--fail-on",
                        threshold,
                        "--severity",
                        &setting,
                    ],
                    i32::from(expected),
                );
                let d = findings(&r, "TP1001");
                assert_eq!(d.len(), 1, "{r}");
                assert_eq!(d[0]["certainty"], "possible");
                assert_eq!(d[0]["actionable"], expected);
            }
        }
    }
    p.write("main.py","from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 1\ndef run(flag):\n    c=C()\n    if flag: c.close()\n    c.status() # codepact: ignore[TP1001] -- reviewed possible state\n    c.close()\n");
    for mode in ["advisory", "strict"] {
        let report = p.check(
            &["--mode", mode, "--fail-on", "warning"],
            i32::from(mode == "strict"),
        );
        let possible = findings(&report, "TP1001");
        assert_eq!(possible[0]["certainty"], "possible");
        assert_eq!(possible[0]["disposition"], "suppressed-inline");
        assert_eq!(report["coverage"]["status"], "incomplete");
    }
    p.write(
        "main.py",
        "from codepact.rust import MustUse\ndef bad(value: MustUse[int]): pass\n",
    );
    for mode in ["advisory", "strict"] {
        let r = p.check(&["--mode", mode, "--rules", "TP3001"], 1);
        assert!(!findings(&r, "TP4002").is_empty());
        assert_eq!(
            p.check(&["--severity", "TP4002=info"], 2)["execution_status"],
            "failed"
        );
    }
}

#[test]
fn v04_13_15_unsupported_descriptor_async_generator_and_dynamic_calls_are_boundaries() {
    let p = Project::new("mustuse-boundaries");
    for source in [
        "from codepact import must_use\nclass C:\n    @must_use\n    @staticmethod\n    def fetch() -> int: return 1\nC.fetch()\n",
        "from codepact import must_use\n@must_use\nasync def fetch() -> int: return 1\nasync def run():\n    await fetch()\n",
        "from codepact import must_use\n@must_use\ndef fetch(): yield 1\ndef run(): fetch()\n",
        "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run(choose):\n    choose()()\n",
    ] {
        p.write("main.py",source);let r=p.check(&["--mode","strict"],1);
        assert_eq!(r["coverage"]["status"],"incomplete","{source}\n{r}");
        assert!(!findings(&r,"TP9001").is_empty() || !findings(&r,"TP4002").is_empty(),"{source}\n{r}");
        assert!(findings(&r,"TP3001").is_empty(),"unsupported calls must not be invented as qualified: {source}\n{r}");
    }
}

#[test]
fn report_snapshot_is_independent_of_suppressed_source_file_order() {
    use codepact::{
        adoption::{build_report, resolve_config, Overrides},
        check_with_ownership, SourceFile,
    };

    let p = Project::new("suppression-snapshot-order");
    let config = resolve_config(
        &p.0,
        &Overrides {
            no_config: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut files = [
        SourceFile::new(
            "a.py",
            "a",
            "pass  # codepact: ignore[TP3001] -- first reason\n\n\n\n\n\n\n\n\npass  # codepact: ignore[TP3001] -- second reason\n",
        ),
        SourceFile::new(
            "b.py",
            "b",
            "pass  # codepact: ignore[TP3001] -- another file\n",
        ),
    ];
    let report = |files: &[SourceFile]| {
        let checked = check_with_ownership(files, &config.python_version, config.ownership);
        let report = build_report(checked, files, &config, None, None).unwrap();
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.unused_suppressions, ["a.py:1", "a.py:10", "b.py:1"]);
        report
    };

    let original = report(&files);
    files.reverse();
    let reordered = report(&files);
    assert_eq!(original.policy_fingerprint, reordered.policy_fingerprint);
    assert_eq!(original.snapshot_id, reordered.snapshot_id);

    files[0].source = files[0].source.replace("another file", "updated reason");
    let changed = report(&files);
    assert_ne!(original.policy_fingerprint, changed.policy_fingerprint);
    assert_ne!(original.snapshot_id, changed.snapshot_id);
}

#[test]
fn v04_19_related_and_explanation_limits_preserve_full_policy() {
    use codepact::{AnalysisSession, CheckResult, SourceFile};
    let p = Project::new("detail-limits");
    let files = [SourceFile::new("main.py", "main", MARKED)];
    let mut result = CheckResult::from_inspection(
        AnalysisSession::default().check_with_must_use(&files, "3.11"),
        "3.11",
    );
    let diagnostic = result
        .diagnostics
        .iter_mut()
        .find(|d| d.code == "TP3001")
        .unwrap();
    diagnostic.message = "é".repeat(5000);
    diagnostic.repair = Some("界".repeat(4000));
    diagnostic.related = (0..40).map(|_| diagnostic.primary.clone()).collect();
    let config = codepact::adoption::resolve_config(
        &p.0,
        &codepact::adoption::Overrides {
            no_config: true,
            fail_on: Some("warning".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let report = codepact::adoption::build_report(result, &files, &config, None, None).unwrap();
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.total_diagnostics, 1);
    assert!(report.details_truncated);
    let d = &report.diagnostics[0];
    assert_eq!(d.diagnostic.related.len(), 32);
    assert_eq!(d.related_sites_omitted, 8);
    assert!(
        d.diagnostic.message.len() + d.diagnostic.repair.as_ref().map_or(0, |s| s.len()) <= 8192
    );
    assert!(d.message_omitted_bytes > 0);
    assert!(d.repair_omitted_bytes > 0);
}

#[test]
fn v04_15_generator_bodies_are_deferred_and_nested_yields_do_not_mark_outer_functions() {
    let p = Project::new("generator-deferral");
    p.write("main.py","from codepact import resource, must_use\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n@must_use\ndef values():\n    c=C()\n    yield c\n@must_use\nasync def async_values():\n    yield 1\nclass Service:\n    @must_use\n    def values(self): yield 1\ndef run():\n    values()\n    async_values()\n    service=Service()\n    service.values()\n");
    let r = p.check(&["--mode", "strict"], 1);
    assert!(findings(&r, "TP3001").is_empty(), "{r}");
    assert!(
        findings(&r, "TP1002")
            .iter()
            .all(|d| d["certainty"] != "definite"),
        "unsupported generator execution must not invent definite acquisition: {r}"
    );
    assert_eq!(r["coverage"]["status"], "incomplete");
    p.write("main.py","from codepact import must_use\n@must_use\ndef outer():\n    def nested(): yield 1\n    return 1\nouter()\n");
    let r = p.check(&["--fail-on", "warning"], 1);
    assert_eq!(
        findings(&r, "TP3001").len(),
        1,
        "the outer function is synchronous despite the nested yield: {r}"
    );
}

#[test]
fn v04_11_24_baseline_byte_limit_accepts_exact_boundary_and_recovers() {
    let p = Project::new("baseline-bytes");
    p.write("main.py", MARKED);
    let generated = p.run(&[
        "baseline",
        "update",
        "--no-config",
        "--output",
        "baseline.json",
        "--reason",
        "bounded input",
        "--json",
    ]);
    assert!(generated.status.success());
    let mut bytes = fs::read(p.0.join("baseline.json")).unwrap();
    let original = bytes.clone();
    bytes.resize(16 * 1024 * 1024, b' ');
    fs::write(p.0.join("baseline.json"), &bytes).unwrap();
    assert_eq!(
        p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 0)["baseline"]["matched"],
        1
    );
    bytes.push(b' ');
    fs::write(p.0.join("baseline.json"), &bytes).unwrap();
    let failed = p.check(&["--baseline", "baseline.json"], 2);
    assert!(failed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("limit"));
    fs::write(p.0.join("baseline.json"), &original).unwrap();
    assert_eq!(
        p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 0)["baseline"]["matched"],
        1
    );
}

#[test]
fn v04_10_baselines_preserve_token_equivalent_whitespace_and_multiline_calls() {
    let p = Project::new("token-fingerprint");
    p.write("main.py", MARKED);
    assert!(p
        .run(&[
            "baseline",
            "update",
            "--no-config",
            "--output",
            "baseline.json",
            "--reason",
            "reviewed site"
        ])
        .status
        .success());
    for source in [
        MARKED.replace("fetch()", "fetch ( )"),
        MARKED.replace("    fetch()", "    fetch(\n    ) # same call"),
        format!("# moved\n\n{MARKED}"),
    ] {
        p.write("main.py", &source);
        let r = p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 0);
        assert_eq!(r["baseline"]["matched"], 1, "{r}");
    }
}

#[test]
fn v04_10_11_collision_changed_evidence_and_semantic_revisions_cannot_hide_findings() {
    use codepact::{AnalysisSession, CheckResult, SourceFile};
    let p = Project::new("baseline-evidence");
    p.write("main.py", MARKED);
    let files = [SourceFile::new("main.py", "main", MARKED)];
    let result = CheckResult::from_inspection(
        AnalysisSession::default().check_with_must_use(&files, "3.11"),
        "3.11",
    );
    let config = codepact::adoption::resolve_config(
        &p.0,
        &codepact::adoption::Overrides {
            no_config: true,
            fail_on: Some("warning".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let baseline = p.0.join("baseline.json");
    codepact::adoption::write_baseline(&baseline, &result, &files, &config, false, "reviewed site")
        .unwrap();
    let mut collision = result.clone();
    let mut second = collision.diagnostics[0].clone();
    second.message =
        "Different display prose cannot disambiguate identical semantic evidence".into();
    collision.diagnostics.push(second);
    let ambiguous =
        codepact::adoption::build_report(collision.clone(), &files, &config, Some(&baseline), None)
            .unwrap();
    assert_eq!(ambiguous.baseline.matched, 0);
    assert_eq!(ambiguous.actionable_diagnostics, 2);
    assert_eq!(ambiguous.exit_code, 1);
    let skipped = codepact::adoption::write_baseline(
        &p.0.join("ambiguous.json"),
        &collision,
        &files,
        &config,
        false,
        "collision control",
    )
    .unwrap();
    assert_eq!(skipped.entries, 0);
    assert_eq!(skipped.skipped_ambiguous, 2);
    for field in ["certainty", "trust", "completion"] {
        let mut changed = result.clone();
        let d = &mut changed.diagnostics[0];
        match field {
            "certainty" => d.certainty = "possible".into(),
            "trust" => d.trust = "declared-unverified".into(),
            _ => d.completion = Some("raise".into()),
        }
        let r = codepact::adoption::build_report(changed, &files, &config, Some(&baseline), None)
            .unwrap();
        assert_eq!(r.baseline.matched, 0, "{field}");
        assert_eq!(r.exit_code, 1, "{field}");
    }
    let original = fs::read(&baseline).unwrap();
    for (field, value) in [
        ("target_python_version", json!("3.12")),
        ("model_catalog_version", json!(99)),
        ("rule_semantic_version", json!(99)),
        ("engine_version", json!("0.4.0-other")),
    ] {
        let mut document: Value = serde_json::from_slice(&original).unwrap();
        document[field] = value;
        fs::write(&baseline, serde_json::to_vec(&document).unwrap()).unwrap();
        let r = p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 1);
        assert_eq!(r["baseline"]["compatible"], false, "{field}");
        assert_eq!(r["baseline"]["matched"], 0);
    }
    fs::write(&baseline, &original).unwrap();
    fs::rename(p.0.join("main.py"), p.0.join("renamed.py")).unwrap();
    let renamed = p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 1);
    assert_eq!(renamed["baseline"]["matched"], 0);
    assert_eq!(
        findings(&renamed, "TP3001")[0]["primary"]["file"],
        "renamed.py"
    );
}

#[test]
fn v04_03_excluded_dependencies_remain_boundaries_and_operands_cannot_restore_them() {
    let p = Project::new("excluded-dependency");
    p.write("provider.py", MARKED);
    p.write("app.py", "from provider import fetch\ndef run(): fetch()\n");
    p.write(
        "pyproject.toml",
        "[tool.codepact]\nconfig-version=1\ninclude=['app.py']\nexclude=['provider.py']\n",
    );
    let output = p.run(&["check", "--json", "--mode", "strict"]);
    assert_eq!(output.status.code(), Some(1));
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r["selected_files"], json!(["app.py"]));
    assert_eq!(r["coverage"]["status"], "incomplete");
    assert!(findings(&r, "TP3001").is_empty());
    assert!(!findings(&r, "TP9001").is_empty());
    let explicit = p.run(&["check", "--json", "provider.py"]);
    assert_eq!(explicit.status.code(), Some(2));
}

#[test]
fn v04_14_lookalike_discard_keeps_actual_source_effects() {
    let p = Project::new("lookalike-discard");
    p.write("main.py","from codepact import must_use, resource\nfrom codepact.rust import Owned\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n@must_use\ndef acquire() -> Owned[Connection]: return Connection()\ndef discard(value: Owned[Connection]): value.close()\ndef run():\n    conn = acquire()\n    discard(conn)\n");
    let r = p.check(&["--fail-on", "warning"], 0);
    assert!(
        findings(&r, "TP1002").is_empty(),
        "the ordinary function actually closes its argument: {r}"
    );
    assert!(findings(&r, "TP3001").is_empty());
}

#[test]
fn v04_03_physical_duplicates_do_not_exhaust_the_exact_selected_file_limit() {
    let p = Project::new("physical-file-limit");
    for i in 0..codepact::DEFAULT_MAX_INPUT_FILES {
        p.write(&format!("f{i:05}.py"), "pass\n");
    }
    let absolute = p.0.join("f00000.py");
    let r = p.check(&[absolute.to_str().unwrap(), "./f00000.py"], 0);
    assert_eq!(
        r["selected_files"].as_array().unwrap().len(),
        codepact::DEFAULT_MAX_INPUT_FILES
    );
    p.write("overflow.py", "pass\n");
    assert_eq!(p.check(&[], 2)["execution_status"], "failed");
    fs::remove_file(p.0.join("overflow.py")).unwrap();
    assert_eq!(
        p.check(&[], 0)["selected_files"].as_array().unwrap().len(),
        codepact::DEFAULT_MAX_INPUT_FILES
    );
}

#[test]
fn v04_10_distinct_ast_sites_on_one_line_have_independent_baseline_identities() {
    let p = Project::new("same-line-sites");
    let source="from codepact import must_use\n@must_use\ndef fetch(value: int) -> int: return value\ndef run():\n    fetch(1); fetch(2)\n";
    p.write("main.py", source);
    let generated = p.run(&[
        "baseline",
        "update",
        "--no-config",
        "--output",
        "baseline.json",
        "--reason",
        "two distinct calls",
        "--json",
    ]);
    assert!(generated.status.success());
    let summary: Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert_eq!(summary["entries"], 2);
    p.write("main.py", &source.replace("fetch(1);", "fetch(3);"));
    let changed = p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 1);
    assert_eq!(changed["baseline"]["matched"], 1);
    assert_eq!(changed["actionable_diagnostics"], 1);
    p.write("main.py", &source.replace("fetch(2)", "fetch(1)"));
    let ambiguous = p.check(&["--baseline", "baseline.json", "--fail-on", "warning"], 1);
    assert_eq!(ambiguous["baseline"]["matched"], 0);
    assert_eq!(ambiguous["actionable_diagnostics"], 2);
}

#[cfg(unix)]
#[test]
fn v04_19_broken_stderr_and_both_streams_return_execution_failure_without_panicking() {
    use std::process::Stdio;
    let p = Project::new("broken-stderr");
    p.write("main.py", MARKED);
    for both in [false, true] {
        let mut child = Command::new(binary())
            .current_dir(&p.0)
            .args([
                "check",
                "--no-config",
                "--output-format",
                "concise",
                "--fail-on",
                "warning",
            ])
            .stdout(if both { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stderr.take());
        if both {
            drop(child.stdout.take());
        }
        assert_eq!(child.wait().unwrap().code(), Some(2));
    }
}

#[test]
fn v04_05_24_legacy_and_cancelled_checks_cannot_claim_unexecuted_mustuse_scope() {
    use codepact::{AnalysisSession, CheckResult, SourceFile};
    let p = Project::new("analysis-scope");
    let files = [SourceFile::new("main.py", "main", MARKED)];
    let mut config = codepact::adoption::resolve_config(
        &p.0,
        &codepact::adoption::Overrides {
            no_config: true,
            ..Default::default()
        },
    )
    .unwrap();
    let legacy = AnalysisSession::default().check_report(&files, "3.11");
    assert!(legacy.diagnostics.iter().all(|d| d.code != "TP3001"));
    assert!(
        codepact::adoption::build_report(legacy.clone(), &files, &config, None, None)
            .unwrap_err()
            .contains("not analyzed")
    );
    let destination = p.0.join("baseline.json");
    assert!(codepact::adoption::write_baseline(
        &destination,
        &legacy,
        &files,
        &config,
        false,
        "unchecked scope"
    )
    .is_err());
    assert!(!destination.exists());
    config.enabled_rules.remove("TP3001");
    let inspection_only = CheckResult::from_inspection(codepact::inspect(&files, "3.11"), "3.11");
    assert!(
        codepact::adoption::build_report(inspection_only, &files, &config, None, None)
            .unwrap_err()
            .contains("not executed")
    );
    let legacy_report =
        codepact::adoption::build_report(legacy, &files, &config, None, None).unwrap();
    assert_eq!(legacy_report.exit_code, 0);
    config.enabled_rules.insert("TP3001".into());
    let mut current = CheckResult::from_inspection(
        AnalysisSession::default().check_with_must_use(&files, "3.11"),
        "3.11",
    );
    assert_eq!(current.assurance, codepact::ADOPTION_ASSURANCE);
    let checked =
        codepact::adoption::build_report(current.clone(), &files, &config, None, None).unwrap();
    assert_ne!(
        checked.semantic_fingerprint,
        legacy_report.semantic_fingerprint
    );
    current.diagnostics[0].code = "TP9002".into();
    assert!(
        codepact::adoption::build_report(current.clone(), &files, &config, None, None)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(codepact::adoption::write_baseline(
        &destination,
        &current,
        &files,
        &config,
        false,
        "cancelled scope"
    )
    .is_err());
    assert!(!destination.exists());
}

#[test]
fn v04_08_possible_policy_is_independent_of_coverage_and_severity() {
    use codepact::{AnalysisSession, CheckResult, SourceFile};
    let p = Project::new("possible-policy-component");
    let files=[SourceFile::new("main.py","main","from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 1\ndef run():\n    c=C()\n    c.close()\n    c.status()\n")];
    let mut facts = CheckResult::from_inspection(
        AnalysisSession::default().check_with_must_use(&files, "3.11"),
        "3.11",
    );
    assert_eq!(facts.coverage.status, "complete-for-supported-subset");
    assert_eq!(facts.diagnostics.len(), 1);
    assert_eq!(facts.diagnostics[0].code, "TP1001");
    // Isolate the policy function from semantic classification. The native
    // possible-state controls above independently test the flow engine.
    facts.diagnostics[0].certainty = "possible".into();
    let mut config = codepact::adoption::resolve_config(
        &p.0,
        &codepact::adoption::Overrides {
            no_config: true,
            ..Default::default()
        },
    )
    .unwrap();
    for mode in ["advisory", "strict"] {
        for threshold in ["error", "warning"] {
            for severity in ["error", "warning", "info"] {
                config.mode = mode.into();
                config.fail_on = threshold.into();
                config.severity.insert("TP1001".into(), severity.into());
                let report =
                    codepact::adoption::build_report(facts.clone(), &files, &config, None, None)
                        .unwrap();
                let expected = mode == "strict"
                    || severity == "error"
                    || severity == "warning" && threshold == "warning";
                assert_eq!(
                    report.exit_code,
                    u8::from(expected),
                    "{mode}/{threshold}/{severity}"
                );
                assert_eq!(report.diagnostics[0].diagnostic.certainty, "possible");
                assert_eq!(report.coverage.status, "complete-for-supported-subset");
            }
        }
    }
}

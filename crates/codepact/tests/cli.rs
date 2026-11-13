use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

#[test]
fn version_flags_print_the_package_version() {
    let expected = concat!("codepact ", env!("CARGO_PKG_VERSION"), "\n");
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_codepact"))
            .arg(flag)
            .output()
            .expect("CLI starts");

        assert!(output.status.success(), "{flag}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{flag}");
        assert!(output.stderr.is_empty(), "{flag}: {output:?}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_path_argument_returns_a_json_input_error_without_panicking() {
    use std::os::unix::ffi::OsStringExt;

    for args in [
        vec!["check", "--json"],
        vec!["check", "--output-format", "json"],
        vec!["inspect-contracts"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codepact"));
        command.args(&args);
        command.arg(std::ffi::OsString::from_vec(b"bad_\xff.py".to_vec()));
        let output = command.output().expect("CLI starts");

        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stderr.is_empty(), "{args:?}");
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "{args:?}: {error}: {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            });
        assert_eq!(report["execution_status"], "failed", "{args:?}");
        assert_eq!(
            report["error"]["message"],
            format!("argument {} is not valid UTF-8", args.len() + 1),
            "{args:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn json_like_option_value_does_not_select_json_error_output() {
    use std::os::unix::ffi::OsStringExt;

    let output = Command::new(env!("CARGO_BIN_EXE_codepact"))
        .args(["check", "--output-format", "text", "--root", "--json"])
        .arg(std::ffi::OsString::from_vec(b"bad_\xff.py".to_vec()))
        .output()
        .expect("CLI starts");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "codepact: argument 6 is not valid UTF-8"
    );
}

#[cfg(unix)]
#[test]
fn version_flags_report_a_closed_stdout_pipe() {
    for flag in ["--version", "-V"] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);

        let output = Command::new(env!("CARGO_BIN_EXE_codepact"))
            .arg(flag)
            .stdout(writer)
            .output()
            .expect("CLI starts");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{flag}: {stderr}");
        assert!(
            stderr.starts_with("codepact: cannot write output: "),
            "{stderr}"
        );
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
    }
}

fn fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!("codepact-cli-{}", std::process::id()))
}

fn run(root: &PathBuf, paths: &[&str]) -> Output {
    run_for_version(root, paths, "3.11")
}

fn run_for_version(root: &PathBuf, paths: &[&str], version: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(root)
        .arg("inspect-contracts")
        .arg("--python-version")
        .arg(version)
        .arg("--root")
        .arg(root)
        .args(paths)
        .output()
        .expect("CLI starts")
}

fn run_check(root: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(root)
        .args(["check", "--root"])
        .arg(root)
        .arg("--json")
        .output()
        .expect("CLI starts")
}

fn run_check_text(root: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(root)
        .args(["check", "--root"])
        .arg(root)
        .output()
        .expect("CLI starts")
}

fn assert_adoption_report_matches_check_result(
    report: &serde_json::Value,
    expected: &serde_json::Value,
) {
    assert_eq!(report["assurance"], codepact::ADOPTION_ASSURANCE);
    assert_eq!(expected["assurance"], codepact::LIFECYCLE_ASSURANCE);
    assert_eq!(report["check_schema_version"], 2);
    assert_eq!(expected["check_schema_version"], 1);
    for field in [
        "target_python_version",
        "model_catalog_version",
        "available_models",
        "contracts",
        "coverage",
    ] {
        assert_eq!(report[field], expected[field], "canonical field {field}");
    }
    let fields = [
        "code",
        "severity",
        "message",
        "primary",
        "related",
        "certainty",
        "completion",
        "trust",
        "coverage_reason",
        "repair",
    ];
    let actual = report["diagnostics"].as_array().unwrap();
    let mut remaining = expected["diagnostics"].as_array().unwrap().clone();
    assert_eq!(actual.len(), remaining.len(), "canonical diagnostic count");
    for diagnostic in actual {
        let Some(position) = remaining.iter().position(|candidate| {
            fields
                .iter()
                .all(|field| diagnostic[*field] == candidate[*field])
        }) else {
            panic!("adoption report contains a noncanonical diagnostic: {diagnostic:#?}");
        };
        remaining.remove(position);
    }
    assert!(remaining.is_empty(), "canonical diagnostics were omitted");
}

#[test]
fn coverage_diagnostics_describe_the_limitation_without_a_release_number() {
    let root = fixture_root().with_extension("future-annotation-coverage");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("sample.py"),
        "from __future__ import annotations\n",
    )
    .unwrap();

    let check = |options: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_codepact"))
            .current_dir(&root)
            .args(["check", "--no-config", "--mode", "strict"])
            .args(options)
            .output()
            .expect("CLI starts")
    };
    let text = check(&[]);
    assert_eq!(text.status.code(), Some(1));
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.lines().any(|line| {
            line == "sample.py:1:1: warning TP9001: future annotations are not resolved"
        }),
        "{text}"
    );

    let json = check(&["--json"]);
    assert_eq!(json.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(report["coverage"]["status"], "incomplete");
    assert_eq!(report["diagnostics"][0]["code"], "TP9001");
    assert_eq!(
        report["diagnostics"][0]["message"],
        "future annotations are not resolved"
    );
    assert_eq!(
        report["coverage"]["reasons"][0],
        "sample.py:1: future annotations are not resolved"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn phase03_text_and_json_reports_agree_on_cleanup_diagnostics() {
    let root = fixture_root().with_extension("phase03-report-parity");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("main.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef leak():\n    resource = C()\n",
    )
    .unwrap();

    let text = run_check_text(&root);
    assert_eq!(text.status.code(), Some(1));
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("error TP1002"), "{text}");
    assert!(text.contains("completion: fallthrough"), "{text}");
    assert!(text.contains("certainty: definite"), "{text}");
    assert!(text.contains("trust: declared-unverified"), "{text}");

    let json = run_check(&root);
    assert_eq!(json.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let finding = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "TP1002")
        .unwrap();
    assert_eq!(finding["completion"], "fallthrough");
    assert_eq!(finding["certainty"], "definite");
    assert_eq!(finding["trust"], "declared-unverified");
    assert_eq!(report["check_schema_version"], 2);
    assert_eq!(
        report["coverage"]["status"],
        "complete-for-supported-subset"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn phase03_interfaces_agree_for_possible_and_unknown_cleanup_results() {
    let root = fixture_root().with_extension("phase03-nondefinite-report-parity");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    for (name, source, expected_code, expected_certainty) in [
        (
            "possible",
            "from codepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = C()\n    await resource.aclose()\n",
            "TP1002",
            "possible",
        ),
        (
            "unknown",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    opaque_effect(resource)\n",
            "TP9001",
            "unknown",
        ),
        (
            "finalizer",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def __del__(self): self.close()\ndef cleanup():\n    resource = C()\n",
            "TP9001",
            "unknown",
        ),
        (
            "async-context-cancellation-suppression",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    async with UnknownManager():\n        await resource.pause()\n        resource.close()\n",
            "TP9001",
            "unknown",
        ),
    ] {
        fs::write(root.join("main.py"), source).unwrap();

        let text = run_check_text(&root);
        assert_eq!(text.status.code(), Some(0), "{name}");
        let text = String::from_utf8_lossy(&text.stdout);
        assert!(text.contains(expected_code), "{name}: {text}");
        assert!(
            text.contains(&format!("certainty: {expected_certainty}")),
            "{name}: {text}"
        );
        assert!(text.contains("analysis incomplete:"), "{name}: {text}");

        let json = run_check(&root);
        assert_eq!(json.status.code(), Some(0), "{name}");
        let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(report["coverage"]["status"], "incomplete", "{name}");
        assert!(report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic["code"] == expected_code
                    && diagnostic["certainty"] == expected_certainty
            }),
            "{name}: {report:#?}"
        );

        let rust_result = codepact::check_report(
            &[codepact::SourceFile::new("main.py", "main", source)],
            "3.11",
        );
        let rust_result = serde_json::to_value(rust_result).unwrap();
        assert_eq!(report["coverage"], rust_result["coverage"], "{name}");
        assert_eq!(report["contracts"], rust_result["contracts"], "{name}");
        assert_eq!(
            report["available_models"], rust_result["available_models"],
            "{name}"
        );
        assert_adoption_report_matches_check_result(&report, &rust_result);
    }

    let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(&root)
        .args(["check", "--root"])
        .arg(&root)
        .arg("--invalid-option")
        .output()
        .expect("CLI starts");
    assert_eq!(invalid.status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn phase03_module_qualified_contextlib_model_is_reported_in_text_and_json() {
    let root = fixture_root().with_extension("phase03-contextlib-model");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let source = "import contextlib as ctx\nfrom codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with ctx.closing(resource):\n        pass\n";
    fs::write(root.join("main.py"), source).unwrap();

    let text = run_check_text(&root);
    assert_eq!(text.status.code(), Some(0));
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(!text.contains("TP1002"), "{text}");
    assert!(text.is_empty(), "{text}");

    let json = run_check(&root);
    assert_eq!(json.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(
        report["coverage"]["status"],
        "complete-for-supported-subset"
    );
    assert_eq!(report["model_catalog_version"], 2);
    assert!(report["available_models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| {
            model["id"] == "TPM-CTX-001" && model["source_revisions"].as_array().unwrap().len() == 3
        }));
    let rust_result = codepact::check_report(
        &[codepact::SourceFile::new("main.py", "main", source)],
        "3.11",
    );
    assert_adoption_report_matches_check_result(
        &report,
        &serde_json::to_value(rust_result).unwrap(),
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_reports_one_use_after_close_at_the_call_site() {
    let root = fixture_root().with_extension("lifecycle");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let source = "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass Connection:\n    def status(self): return 'ready'\n    def close(self): pass\ndef inspect(conn: Borrowed[Connection]):\n    return conn.status()\nconn = Connection()\ninspect(conn)\nconn.close()\ninspect(conn)\n";
    fs::write(root.join("connection.py"), source).unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = fs::read_to_string(root.join("connection.py")).unwrap();
    let api = codepact::check(
        &[codepact::SourceFile::new(
            "connection.py",
            "connection",
            source.clone(),
        )],
        "3.11",
    );
    let report = codepact::CheckResult::from_inspection(api, "3.11");
    assert_adoption_report_matches_check_result(&result, &serde_json::to_value(report).unwrap());
    let diagnostics = result["diagnostics"].as_array().unwrap();
    let findings: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{diagnostics:#?}");
    assert_eq!(findings[0]["certainty"], "definite");
    assert_eq!(findings[0]["primary"]["line"], 12);
    assert_eq!(findings[0]["related"][0]["line"], 11);
    assert_eq!(
        result["coverage"]["status"], "complete-for-supported-subset",
        "{result:#?}"
    );

    assert_eq!(result["assurance"], codepact::ADOPTION_ASSURANCE);
    let valid_source = source.strip_suffix("inspect(conn)\n").unwrap();
    let valid = codepact::check(
        &[codepact::SourceFile::new(
            "connection.py",
            "connection",
            valid_source,
        )],
        "3.11",
    );
    assert!(valid.diagnostics.is_empty(), "{valid:#?}");
    assert_eq!(valid.coverage.status, "complete-for-supported-subset");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn constructor_assignment_shadows_the_resource_class_binding() {
    let root = fixture_root().with_extension("constructor-shadow");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("main.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def status(self): pass\n    def close(self): pass\nC = C()\nvalue = C()\nvalue.status()\n",
    )
    .unwrap();

    let result: serde_json::Value = serde_json::from_slice(&run_check(&root).stdout).unwrap();
    assert_ne!(
        result["coverage"]["status"],
        "complete-for-supported-subset"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn class_body_executable_effects_mark_coverage_incomplete() {
    let root = fixture_root().with_extension("class-body-effect");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("main.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def status(self): pass\n    def close(self): pass\nc = C()\nclass Config:\n    c.close()\nc.status()\n",
    )
    .unwrap();

    let result: serde_json::Value = serde_json::from_slice(&run_check(&root).stdout).unwrap();
    assert_ne!(
        result["coverage"]["status"],
        "complete-for-supported-subset"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn raise_stops_normal_flow_and_does_not_report_unreachable_use() {
    let root = fixture_root().with_extension("raise-flow");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("main.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def status(self): pass\n    def close(self): pass\nc = C()\nraise RuntimeError()\nc.close()\nc.status()\n",
    )
    .unwrap();

    let result: serde_json::Value = serde_json::from_slice(&run_check(&root).stdout).unwrap();
    assert_eq!(
        result["coverage"]["status"], "incomplete",
        "module completion is not interpreter shutdown after an initialization raise"
    );
    assert!(!result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP1001"));
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("module-scope resource lifetime"))
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn helper_constructor_resolution_uses_the_provider_class() {
    let root = fixture_root().with_extension("provider-constructor");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("provider.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def __init__(self, other): other.close()\n    def close(self): pass\ndef wrap(value: C):\n    temp = C(value)\n",
    )
    .unwrap();
    fs::write(
        root.join("caller.py"),
        "from codepact import resource\nfrom provider import wrap\n@resource(closed_by='close')\nclass C:\n    def __init__(self): pass\n    def status(self): pass\n    def close(self): pass\nc = C()\nwrap(c)\nc.status()\n",
    )
    .unwrap();

    let result: serde_json::Value = serde_json::from_slice(&run_check(&root).stdout).unwrap();
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TP1001"),
        "{result:#?}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_branch_join_is_possible_and_borrowed_close_is_rejected() {
    let root = fixture_root().with_extension("lifecycle-uncertainty");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("connection.py"),
        "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass Connection:\n    def status(self): return 'ready'\n    def close(self): pass\ndef inspect(conn: Borrowed[Connection]):\n    conn.close()\nconn = Connection()\nif flag:\n    conn.close()\nconn.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP2001"));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TP1001"
                && diagnostic["certainty"] == "possible"),
        "{diagnostics:#?}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_alias_rebinding_and_uncertain_joins_preserve_object_state() {
    let root = fixture_root().with_extension("lifecycle-alias-joins");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("aliases.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'
same = Connection()
alias = same
alias.close()
same.status()
original = Connection()
detached = original
detached = Connection()
detached.close()
original.status()
left = Connection()
right = Connection()
if flag:
    selected = left
else:
    selected = right
selected.close()
left.status()
right.status()
shared = Connection()
if flag:
    selected_shared = shared
else:
    selected_shared = shared
selected_shared.close()
shared.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    let certainties: Vec<_> = findings
        .iter()
        .filter_map(|diagnostic| diagnostic["certainty"].as_str())
        .collect();
    assert_eq!(
        certainties,
        ["definite", "possible", "possible", "definite"],
        "{:#?}",
        result["diagnostics"]
    );
    assert_eq!(findings[0]["related"].as_array().unwrap().len(), 1);
    assert_eq!(findings[3]["related"].as_array().unwrap().len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_exposes_unbound_aliases_and_alias_budget_exhaustion() {
    let root = fixture_root().with_extension("lifecycle-alias-budgets");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let mut source = String::from(
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\nshared = Connection()\nif maybe_defined:\n    maybe = shared\nalias = maybe\nalias.close()\nshared.status()\nselected = Connection()\nselected.close()\n",
    );
    for index in 0..(64 + 1) {
        source.push_str(&format!(
            "if choose_{index}:\n    selected = Connection()\n    selected.close()\n"
        ));
    }
    source.push_str("selected.status()\n");
    fs::write(root.join("aliases.py"), source).unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "TP9001"
            && diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("may be unbound on one branch"))
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "TP9001"
            && diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("alias target budget exceeded"))
    }));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn close_through_an_aliased_helper_reports_the_borrow_origin() {
    let root = fixture_root().with_extension("lifecycle-borrow-helper");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("borrowed.py"),
        r#"from codepact import resource
from codepact.rust import Borrowed
@resource(closed_by='close')
class Connection:
    def close(self): pass
def close_alias(value):
    alias = value
    alias.close()
def inspect(value: Borrowed[Connection]):
    close_alias(value)
"#,
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP2001")
        .collect();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["primary"]["line"], 8);
    assert_eq!(findings[0]["related"].as_array().unwrap().len(), 1);
    assert_eq!(findings[0]["related"][0]["line"], 9);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_follows_acyclic_named_reexports_without_losing_resource_identity() {
    let root = fixture_root().with_extension("lifecycle-reexports");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("models.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'
"#,
    )
    .unwrap();
    fs::write(
        root.join("facade.py"),
        "from models import Connection as OriginalConnection\n",
    )
    .unwrap();
    fs::write(
        root.join("api.py"),
        "from facade import OriginalConnection as Connection\n",
    )
    .unwrap();
    fs::write(
        root.join("client.py"),
        r#"from api import Connection as Conn
connection = Conn()
connection.close()
connection.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    let findings: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{diagnostics:#?}");
    assert_eq!(findings[0]["certainty"], "definite");
    assert_eq!(findings[0]["primary"]["file"], "client.py");
    assert_eq!(
        result["coverage"]["status"], "complete-for-supported-subset",
        "{result:#?}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_preserves_call_effects_and_reports_unknown_qualified_calls() {
    let root = fixture_root().with_extension("lifecycle-call-effects");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("calls.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\ndef ignore(value): pass\nconnection = Connection()\nshutdown(connection)\nconnection.status()\nignore(connection.close())\nconnection.status()\nexternal.mutate(connection)\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic["code"] == "TP1001")
            .count(),
        2,
        "{diagnostics:#?}"
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "TP9001"
            && diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("unresolved method receiver"))
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recursive_summaries_stop_with_an_incomplete_boundary() {
    let root = fixture_root().with_extension("lifecycle-recursion");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("recursive.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef recurse(value):\n    recurse(value)\n    recurse(value)\nconnection = Connection()\nrecurse(connection)\n",
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["coverage"]["status"], "incomplete");
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("recursive function summary"))
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_applies_a_reexported_helper_and_retains_its_provider_location() {
    let root = fixture_root().with_extension("lifecycle-helper-reexports");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("resources.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
    )
    .unwrap();
    fs::write(
        root.join("helpers.py"),
        "def close_connection(value):\n    value.close()\n",
    )
    .unwrap();
    fs::write(
        root.join("reexports.py"),
        "from helpers import close_connection as close_connection\n",
    )
    .unwrap();
    fs::write(
        root.join("client.py"),
        "from resources import Connection\nfrom reexports import close_connection\nvalue = Connection()\nclose_connection(value)\nvalue.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["primary"]["file"], "client.py");
    assert_eq!(findings[0]["related"][0]["file"], "helpers.py");

    let sources: Vec<_> = ["resources.py", "helpers.py", "reexports.py", "client.py"]
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(root.join(path)).unwrap();
            codepact::SourceFile::new(path, path.trim_end_matches(".py"), source)
        })
        .collect();
    assert_adoption_report_matches_check_result(
        &result,
        &serde_json::to_value(codepact::CheckResult::from_inspection(
            codepact::check(&sources, "3.11"),
            "3.11",
        ))
        .unwrap(),
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lifecycle_metadata_forms_share_flow_semantics() {
    let root = fixture_root().with_extension("lifecycle-metadata-parity");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("metadata.py"),
        r#"from typing import Annotated
from codepact import resource
from codepact.contracts import Resource, SharedBorrow
from codepact.rust import Borrowed
@resource(closed_by='close')
class Decorated:
    def close(self): pass
    def status(self): return 'ready'
class Explicit:
    def close(self): pass
    def status(self): return 'ready'
ExplicitResource = Annotated[Explicit, Resource(closed_by='close')]
def read_decorated(value: Borrowed[Decorated]):
    value.status()
def read_explicit(value: Annotated[Explicit, Resource(closed_by='close'), SharedBorrow()]):
    value.status()
decorated = Decorated()
read_decorated(decorated)
decorated.close()
read_decorated(decorated)
explicit = ExplicitResource()
read_explicit(explicit)
explicit.close()
read_explicit(explicit)
"#,
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 2, "{:#?}", result["diagnostics"]);
    assert!(findings
        .iter()
        .all(|diagnostic| diagnostic["certainty"] == "definite"));
    assert!(findings
        .iter()
        .all(|diagnostic| diagnostic["related"].as_array().unwrap().len() == 1));
    assert!(!result["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|contract| {
            contract["display_name"] == "Explicit" && !contract["lifecycle"].is_null()
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn helper_effects_preserve_alias_order_returns_and_branch_certainty() {
    let root = fixture_root().with_extension("lifecycle-helper-order");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("helpers.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'
def close_then_read(first, second):
    first.close()
    second.status()
def close_and_return(value):
    value.close()
    return
    value.status()
def maybe_close(value):
    if should_close:
        value.close()
same = Connection()
close_then_read(same, same)
returned = Connection()
close_and_return(returned)
returned.status()
conditional = Connection()
maybe_close(conditional)
conditional.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 3, "{:#?}", result["diagnostics"]);
    let certainties: Vec<_> = findings
        .iter()
        .map(|diagnostic| diagnostic["certainty"].as_str().unwrap())
        .collect();
    assert_eq!(certainties, ["definite", "definite", "possible"]);
    assert_eq!(findings[0]["primary"]["line"], 17);
    assert_eq!(findings[1]["primary"]["line"], 20);
    assert_eq!(findings[2]["primary"]["line"], 23);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_effects_escape_and_unsupported_flow_keep_results_incomplete() {
    let root = fixture_root().with_extension("lifecycle-unknown-boundaries");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("unknown.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'
connection = Connection()
unknown_helper(connection)
unknown_zero_argument_call()
connection.status()
other = Connection()
try:
    other.close()
finally:
    other.status()
other.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["coverage"]["status"], "incomplete");
    let findings = result["diagnostics"].as_array().unwrap();
    assert!(findings.iter().any(|diagnostic| {
        diagnostic["code"] == "TP1001"
            && diagnostic["certainty"] == "definite"
            && diagnostic["primary"]["line"] == 14
    }));
    assert!(findings.iter().any(|diagnostic| {
        diagnostic["code"] == "TP1001"
            && diagnostic["certainty"] == "definite"
            && diagnostic["primary"]["line"] == 15
    }));
    assert!(findings.iter().any(|diagnostic| {
        diagnostic["code"] == "TP9001"
            && diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("unknown call effect"))
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn repeat_close_is_an_unknown_boundary_without_a_double_close_finding() {
    let root = fixture_root().with_extension("lifecycle-repeat-close");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("repeat.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\nconnection = Connection()\nconnection.close()\nconnection.close()\nconnection.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["primary"]["line"], 9);
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"] == "repeat-close behavior is not modeled"
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn possible_findings_exit_zero_while_invalid_project_input_exits_two() {
    let root = fixture_root().with_extension("lifecycle-possible-exit");
    let invalid_root = fixture_root().with_extension("lifecycle-empty-project");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&invalid_root);
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&invalid_root).unwrap();
    fs::write(
        root.join("possible.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\nconnection = Connection()\nif maybe:\n    connection.close()\nconnection.status()\n",
    )
    .unwrap();

    let possible = run_check(&root);
    assert_eq!(possible.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&possible.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["certainty"], "possible");

    let invalid = run_check(&invalid_root);
    assert_eq!(invalid.status.code(), Some(2));
    let invalid_output = format!(
        "{}{}",
        String::from_utf8_lossy(&invalid.stdout),
        String::from_utf8_lossy(&invalid.stderr)
    );
    assert!(
        invalid_output.contains("no Python files"),
        "{invalid_output}"
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(invalid_root).unwrap();
}

#[test]
fn loops_and_async_bodies_are_checked_while_unknown_regions_stay_incomplete() {
    let root = fixture_root().with_extension("lifecycle-unsupported-regions");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("unsupported.py"),
        r#"from codepact import resource
from codepact.rust import Borrowed
@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'
def loop_use(value: Borrowed[Connection]):
    for _ in range(1):
        value.close()
    value.status()
def with_use(value: Borrowed[Connection]):
    with open('unused'):
        value.close()
    value.status()
async def async_use(value: Borrowed[Connection]):
    value.close()
def closure_use(value: Borrowed[Connection]):
    def nested():
        value.close()
    nested()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["coverage"]["status"], "incomplete");
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"] == "context-manager cleanup has no qualified model"
        }));
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"
            && diagnostic["message"] == "unsupported flow weakens tracked resources"));
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP2001"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_discovery_rejects_module_collisions_and_skips_symlink_escapes() {
    let root = fixture_root().with_extension("lifecycle-discovery");
    let outside = fixture_root().with_extension("lifecycle-outside");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(root.join("pkg")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("pkg.py"), "value = 1\n").unwrap();
    fs::write(root.join("pkg/__init__.py"), "value = 2\n").unwrap();
    fs::write(
        outside.join("escaped.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nc = C()\nc.close()\nc.status()\n",
    )
    .unwrap();
    let collision = run_check(&root);
    assert_eq!(collision.status.code(), Some(2));
    let collision_output = format!(
        "{}{}",
        String::from_utf8_lossy(&collision.stdout),
        String::from_utf8_lossy(&collision.stderr)
    );
    assert!(
        collision_output.contains("multiple files map to module pkg"),
        "{collision_output}"
    );

    fs::remove_file(root.join("pkg.py")).unwrap();
    fs::remove_dir_all(root.join("pkg")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("escaped.py"), root.join("escaped.py")).unwrap();
        let symlink_result = run_check(&root);
        assert_eq!(symlink_result.status.code(), Some(2));
        let symlink_output = format!(
            "{}{}",
            String::from_utf8_lossy(&symlink_result.stdout),
            String::from_utf8_lossy(&symlink_result.stderr)
        );
        assert!(
            symlink_output.contains("no Python files"),
            "{symlink_output}"
        );
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn project_check_is_stable_across_file_creation_order_and_never_executes_source() {
    let root = fixture_root().with_extension("lifecycle-determinism");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("executed");
    let provider = format!(
        "open({:?}, 'w').close()\nfrom codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
        marker.to_string_lossy()
    );
    let consumer = "from provider import Connection\nconnection = Connection()\nconnection.close()\nconnection.status()\n";
    fs::write(root.join("provider.py"), &provider).unwrap();
    fs::write(root.join("consumer.py"), consumer).unwrap();
    let first = run_check(&root);
    assert_eq!(first.status.code(), Some(1));
    assert!(!marker.exists());

    fs::remove_file(root.join("provider.py")).unwrap();
    fs::remove_file(root.join("consumer.py")).unwrap();
    fs::write(root.join("consumer.py"), consumer).unwrap();
    fs::write(root.join("provider.py"), provider).unwrap();
    let reordered = run_check(&root);
    let repeated = run_check(&root);
    assert_eq!(reordered.status, first.status);
    assert_eq!(reordered.stdout, first.stdout);
    assert_eq!(repeated.stdout, first.stdout);
    assert!(!marker.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_import_cycles_return_deterministic_incomplete_checks() {
    let root = fixture_root().with_extension("lifecycle-import-cycle");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("models.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
    )
    .unwrap();
    fs::write(root.join("left.py"), "from right import Connection\n").unwrap();
    fs::write(root.join("right.py"), "from left import Connection\n").unwrap();
    fs::write(
        root.join("client.py"),
        "from left import Connection\nconnection = Connection()\nconnection.close()\nconnection.status()\n",
    )
    .unwrap();

    let first = run_check(&root);
    let second = run_check(&root);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.stdout, first.stdout);
    let result: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["coverage"]["status"], "incomplete");
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                || diagnostic["code"]
                    .as_str()
                    .is_some_and(|code| code.starts_with("TP4"))
        }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn logical_module_identity_is_stable_for_relative_and_absolute_paths() {
    let root = fixture_root();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("pkg")).unwrap();
    fs::write(
        root.join("pkg/models.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection: pass\n",
    )
    .unwrap();
    fs::write(
        root.join("pkg/client.py"),
        "from pkg.models import Connection\nfrom codepact.rust import Borrowed\ndef use(value: Borrowed[Connection]): pass\n",
    )
    .unwrap();

    let relative = run(&root, &["pkg/client.py", "pkg/models.py"]);
    let absolute = run(
        &root,
        &[
            root.join("pkg/models.py").to_str().unwrap(),
            root.join("pkg/client.py").to_str().unwrap(),
        ],
    );
    assert!(
        relative.status.success(),
        "{}",
        String::from_utf8_lossy(&relative.stderr)
    );
    assert!(
        absolute.status.success(),
        "{}",
        String::from_utf8_lossy(&absolute.stderr)
    );
    assert_eq!(relative.stdout, absolute.stdout);
    let inspection: serde_json::Value = serde_json::from_slice(&relative.stdout).unwrap();
    assert!(inspection["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|contract| {
            contract["base_symbol"] == "pkg.models.Connection"
                && contract["display_name"] == "use.value"
        }));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unicode_module_file_names_are_valid_python_identifiers() {
    let root = fixture_root().with_extension("unicode");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("模型.py"), "class C: pass\n").unwrap();

    let output = run(&root, &["模型.py"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unicode_module_names_follow_the_selected_python_target() {
    let root = fixture_root().with_extension("unicode-version");
    fs::create_dir_all(&root).unwrap();
    let kawi = "\u{11f02}.py";
    let cyrillic = "\u{1c89}.py";
    fs::write(root.join(kawi), "class C: pass\n").unwrap();
    fs::write(root.join(cyrillic), "class C: pass\n").unwrap();
    for version in ["3.11", "3.12", "3.13"] {
        let output = run_for_version(&root, &[kawi], version);
        assert_eq!(
            output.status.code(),
            Some(if version == "3.11" { 2 } else { 0 })
        );
        let output = run_for_version(&root, &[cyrillic], version);
        assert_eq!(output.status.code(), Some(2));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn completeness_regressions_have_the_documented_exit_status() {
    let root = fixture_root().with_extension("completeness");
    fs::create_dir_all(&root).unwrap();
    let prefix = "from codepact import rust\nclass C: pass\n";
    for (index, (body, expected_status)) in [
        ("def use(value: rust.Owned[C]) -> None: pass\n", 0),
        ("def use(value: list[rust.Borrowed[C]]): pass\n", 1),
        (
            "class Change:\n    global C\n    C = int\ndef use(value: rust.Borrowed[C]): pass\n",
            1,
        ),
    ]
    .iter()
    .enumerate()
    {
        let name = format!("case_{index}.py");
        fs::write(root.join(&name), format!("{prefix}{body}")).unwrap();
        for version in ["3.11", "3.12", "3.13"] {
            let result = run_for_version(&root, &[&name], version);
            assert_eq!(
                result.status.code(),
                Some(*expected_status),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lifecycle_method_aliases_use_base_identity_and_uncertain_receivers_join_effects() {
    let root = fixture_root().with_extension("lifecycle-method-views");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("view.py"),
        "from typing import Annotated\nfrom codepact.contracts import Resource\nclass C:\n    def close(self): pass\n    def status(self): pass\nV = Annotated[C, Resource(closed_by='close')]\nc = V()\nc.status()\n",
    )
    .unwrap();
    let output = run_check(&root);
    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["coverage"]["status"], "incomplete",
        "open module resources remain outside the function cleanup scope"
    );
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("module-scope resource lifetime"))
        }));

    fs::write(
        root.join("view.py"),
        "from typing import Annotated\nfrom codepact.contracts import Resource\nclass C:\n    def close(self): pass\n    def status(self): pass\nV = Annotated[C, Resource(closed_by='close')]\nc = V()\nc.close()\nc.status()\n",
    )
    .unwrap();
    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|diagnostic| diagnostic["code"] == "TP1001")
            .count(),
        1,
        "{:#?}",
        result["diagnostics"]
    );

    fs::write(
        root.join("view.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n    def shutdown(self): self.close()\na = C()\nb = C()\nif flag:\n    x = a\nelse:\n    x = b\nx.shutdown()\na.status()\nb.status()\n",
    )
    .unwrap();
    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 2, "{:#?}", result["diagnostics"]);
    assert!(findings
        .iter()
        .all(|finding| finding["certainty"] == "possible"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn assignment_evaluates_nested_effects_once_and_function_rebinding_drops_old_body() {
    let root = fixture_root().with_extension("lifecycle-rebinding");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("main.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\ndef shutdown(value):\n    value.close()\ndef noop(value): pass\nshutdown = noop\nc = C()\nresult = noop(c.close())\nshutdown(c)\nc.status()\n",
    )
    .unwrap();
    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| { diagnostic["message"] == "repeat-close behavior is not modeled" }));
    let findings = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["primary"]["line"], 13);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn helper_resolution_uses_provider_scope_and_borrowed_preconditions() {
    let root = fixture_root().with_extension("lifecycle-provider-scope");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("provider.py"),
        r#"from codepact import resource
from codepact.rust import Borrowed
@resource(closed_by='close')
class C:
    def close(self): pass
    def status(self): pass
def finish(value):
    value.close()
def shutdown(value):
    finish(value)
def inspect(value: Borrowed[C]):
    pass
"#,
    )
    .unwrap();
    fs::write(
        root.join("client.py"),
        r#"from codepact import resource
from provider import C, inspect, shutdown
def finish(value):
    pass
value = C()
shutdown(value)
value.status()
value.close()
inspect(value)
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 2, "{:#?}", result["diagnostics"]);
    assert!(findings
        .iter()
        .all(|finding| finding["certainty"] == "definite"));
    assert_eq!(findings[0]["primary"]["file"], "client.py");
    assert_eq!(findings[1]["primary"]["file"], "client.py");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn constructor_close_body_and_early_return_effects_reach_callers() {
    let root = fixture_root().with_extension("lifecycle-body-effects");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("effects.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class B:
    def close(self): pass
    def status(self): pass
@resource(closed_by='close')
class A:
    def __init__(self, target):
        target.close()
    def close(self, target):
        target.close()
    def status(self): pass
@resource(closed_by='close')
class Maybe:
    def close(self): pass
    def status(self): pass
    def maybe_close(self, flag):
        if flag:
            self.close()
            return
first = B()
a = A(first)
second = B()
a.close(second)
a.status()
first.status()
second.status()
maybe = Maybe()
maybe.maybe_close(flag)
maybe.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 4, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["certainty"], "definite");
    assert_eq!(findings[1]["certainty"], "definite");
    assert_eq!(findings[2]["certainty"], "definite");
    assert_eq!(findings[3]["certainty"], "possible");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn flow_work_budget_stops_exponential_helper_expansion() {
    let root = fixture_root().with_extension("lifecycle-flow-budget");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let mut source = String::from("def f0():\n    pass\n");
    for level in 1..=6 {
        source.push_str(&format!("def f{level}():\n"));
        for _ in 0..10 {
            source.push_str(&format!("    f{}()\n", level - 1));
        }
    }
    source.push_str("f6()\n");
    fs::write(root.join("budget.py"), source).unwrap();

    let output = run_check(&root);
    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic["code"] == "TP9001"
                    && diagnostic["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("flow work budget exceeded"))
            }),
        "{:#?}",
        result["diagnostics"]
    );
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn assignment_rhs_keeps_the_previous_binding_until_its_effects_finish() {
    let root = fixture_root().with_extension("lifecycle-assignment-order");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("assignment.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class C:
    def close(self): pass
    def status(self): return 'ready'
@resource(closed_by='close')
class Wrapper:
    def __init__(self, other): other.close()
    def close(self): pass
    def status(self): return 'ready'
def shutdown(value: C): value.close()
a = C()
alias = a
a = Wrapper(a)
alias.status()
a.status()
b = C()
alias_b = b
b = b.close()
alias_b.status()
c = C()
alias_c = c
c = shutdown(c)
alias_c.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 3, "{:#?}", result["diagnostics"]);
    assert!(findings
        .iter()
        .all(|diagnostic| diagnostic["certainty"] == "definite"));
    assert_eq!(result["coverage"]["status"], "incomplete", "{result:#?}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_call_argument_expressions_create_a_boundary() {
    let root = fixture_root().with_extension("lifecycle-call-argument-expression");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("argument.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef noop(value): pass\nc = C()\nnoop(c.close() or None)\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn function_defaults_with_resource_effects_create_a_boundary() {
    let root = fixture_root().with_extension("lifecycle-function-default-effect");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("default.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nc = C()\ndef f(value=c.close()): pass\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn class_method_defaults_with_resource_effects_create_a_boundary() {
    let root = fixture_root().with_extension("lifecycle-method-default-effect");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("default.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nc = C()\nclass Holder:\n    def __init__(self, value=c.close()): pass\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn function_parameters_shadow_resource_class_names() {
    let root = fixture_root().with_extension("lifecycle-shadowed-class-parameter");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("shadow.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef use(C):\n    value = C()\n    value.close()\n    value.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP1001" && diagnostic["certainty"] == "definite"
        }));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn annotated_method_parameters_are_visible_as_an_incomplete_boundary() {
    let root = fixture_root().with_extension("lifecycle-annotated-method-parameter");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("method.py"),
        "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def consume(self, other: Borrowed['C']): other.close()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("method parameter annotations are not modeled")
                })
        }));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn annotated_close_method_still_enforces_borrowed_close_rule() {
    let root = fixture_root().with_extension("lifecycle-annotated-close-borrowed");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("method.py"),
        "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self, force: bool = False): pass\ndef bad(value: Borrowed[C]): value.close()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| { diagnostic["code"] == "TP2001" }));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn escaped_resource_state_reaches_unknown_calls_in_callees() {
    let root = fixture_root().with_extension("lifecycle-escaped-callee");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("escaped.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef f(value): external()\nc = C()\nunknown(c)\nc.close()\nf(c)\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert!(findings
        .iter()
        .any(|diagnostic| diagnostic["certainty"] == "possible"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn escaped_resource_state_reaches_zero_argument_callees() {
    let root = fixture_root().with_extension("lifecycle-escaped-zero-argument-callee");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("escaped.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef f(): external()\nc = C()\nunknown(c)\nc.close()\nf()\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert!(findings
        .iter()
        .any(|diagnostic| diagnostic["certainty"] == "possible"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn assignment_of_a_resource_returning_close_method_keeps_identity_and_flags_unowned_escape() {
    let root = fixture_root().with_extension("lifecycle-close-return");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("close_return.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): return self\n    def status(self): return 'ready'\nc = C()\nalias = c\nc = c.close()\nalias.status()\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "TP1001" && diagnostic["certainty"] == "definite"
        }),
        "{diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("resource returned without an Owned contract")
                })
        }),
        "{diagnostics:#?}"
    );
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn close_body_runs_before_the_designated_close_transition() {
    let root = fixture_root().with_extension("lifecycle-close-body-order");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("close_body.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def flush(self): pass\n    def close(self): self.flush()\n    def status(self): return 'ready'\nc = C()\nc.close()\nc.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{:#?}", result["diagnostics"]);
    assert_eq!(findings[0]["primary"]["line"], 9);
    assert_eq!(findings[0]["related"][0]["line"], 8);
    assert_eq!(
        result["coverage"]["status"],
        "complete-for-supported-subset"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn decorated_methods_report_a_boundary_without_assuming_instance_binding() {
    let root = fixture_root().with_extension("lifecycle-decorated-method");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("decorated.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class C:
    def close(self): pass
    def status(self): return 'ready'
    @staticmethod
    def shutdown(target): target.close()
a = C()
b = C()
a.close()
a.shutdown(b)
b.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP1001"));
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("decorated method binding is not modeled")
                })
        }),
        "{diagnostics:#?}"
    );
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn decorated_init_reports_boundary_without_assuming_receiver_binding() {
    let root = fixture_root().with_extension("lifecycle-decorated-init");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("decorated_init.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class R:
    def close(self): pass
    def status(self): return 'ready'
@resource(closed_by='close')
class C:
    @staticmethod
    def __init__(other): other.close()
    def close(self): pass
    def status(self): return 'ready'
other = R()
value = C(other)
other.status()
value.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("decorated method binding is not modeled")
                })
        }),
        "{diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TP1001"),
        "{diagnostics:#?}"
    );
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn custom_new_effects_are_reported_as_unknown() {
    let root = fixture_root().with_extension("lifecycle-custom-new");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("custom_new.py"),
        r#"from codepact import resource
@resource(closed_by='close')
class C:
    def __new__(cls, other):
        other.close()
        return object.__new__(cls)
    def close(self): pass
    def status(self): return 'ready'
other = C(None)
value = C(other)
other.status()
"#,
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP1001"));
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "TP9001"
                && diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("custom __new__ allocation effects are not modeled")
                })
        }),
        "{diagnostics:#?}"
    );
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inline_annotated_borrowed_resource_rejects_close() {
    let root = fixture_root().with_extension("lifecycle-inline-borrowed");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("borrowed.py"),
        "from typing import Annotated\nfrom codepact.contracts import Resource, SharedBorrow\nclass C:\n    def close(self): pass\ndef consume(value: Annotated[C, Resource(closed_by='close'), SharedBorrow()]):\n    value.close()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TP2001"),
        "{:#?}",
        result
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_method_effect_weakens_tracked_resource_arguments() {
    let root = fixture_root().with_extension("lifecycle-unknown-method-argument");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("unknown_method.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\na = C()\nb = C()\nb.close()\na.external(b)\nb.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    let use_after_close: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(use_after_close.len(), 1, "{diagnostics:#?}");
    assert_eq!(use_after_close[0]["certainty"], "possible");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_method_effect_is_applied_when_receiver_is_closed() {
    let root = fixture_root().with_extension("lifecycle-unknown-method-closed-receiver");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("unknown_method.py"),
        "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\na = C()\nb = C()\na.close()\nb.close()\na.external(b)\nb.status()\n",
    )
    .unwrap();

    let output = run_check(&root);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = result["diagnostics"].as_array().unwrap();
    let use_after_close: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP1001")
        .collect();
    assert_eq!(use_after_close.len(), 2, "{diagnostics:#?}");
    assert_eq!(use_after_close[0]["certainty"], "definite");
    assert_eq!(use_after_close[1]["certainty"], "possible");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic["code"] == "TP9001"));
    assert_eq!(result["coverage"]["status"], "incomplete");
    fs::remove_dir_all(root).unwrap();
}

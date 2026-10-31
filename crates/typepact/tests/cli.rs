use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

fn fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!("typepact-cli-{}", std::process::id()))
}

fn run(root: &PathBuf, paths: &[&str]) -> Output {
    run_for_version(root, paths, "3.11")
}

fn run_for_version(root: &PathBuf, paths: &[&str], version: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typepact"))
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
    Command::new(env!("CARGO_BIN_EXE_typepact"))
        .current_dir(root)
        .args(["check", "--root"])
        .arg(root)
        .arg("--json")
        .output()
        .expect("CLI starts")
}

#[test]
fn check_reports_one_use_after_close_at_the_call_site() {
    let root = fixture_root().with_extension("lifecycle");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("connection.py"),
        "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass Connection:\n    def status(self): return 'ready'\n    def close(self): pass\ndef inspect(conn: Borrowed[Connection]):\n    return conn.status()\nconn = Connection()\ninspect(conn)\nconn.close()\ninspect(conn)\n",
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
    let source = fs::read_to_string(root.join("connection.py")).unwrap();
    let api = typepact::check(
        &[typepact::SourceFile::new(
            "connection.py",
            "connection",
            source,
        )],
        "3.11",
    );
    assert_eq!(result, serde_json::to_value(api).unwrap());
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
        result["assurance"],
        "lifecycle-checking-for-supported-normal-flow"
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
        "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass Connection:\n    def status(self): return 'ready'\n    def close(self): pass\ndef inspect(conn: Borrowed[Connection]):\n    conn.close()\nconn = Connection()\nif flag:\n    conn.close()\nconn.status()\n",
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
        r#"from typepact import resource
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
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\nshared = Connection()\nif maybe_defined:\n    maybe = shared\nalias = maybe\nalias.close()\nshared.status()\nselected = Connection()\nselected.close()\n",
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
        r#"from typepact import resource
from typepact.rust import Borrowed
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
        r#"from typepact import resource
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
        result["coverage"]["status"],
        "complete-for-supported-subset"
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
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\ndef ignore(value): pass\nconnection = Connection()\nshutdown(connection)\nconnection.status()\nignore(connection.close())\nconnection.status()\nexternal.mutate(connection)\n",
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
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef recurse(value):\n    recurse(value)\n    recurse(value)\nconnection = Connection()\nrecurse(connection)\n",
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
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
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
            typepact::SourceFile::new(path, path.trim_end_matches(".py"), source)
        })
        .collect();
    assert_eq!(
        result,
        serde_json::to_value(typepact::check(&sources, "3.11")).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn logical_module_identity_is_stable_for_relative_and_absolute_paths() {
    let root = fixture_root();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("pkg")).unwrap();
    fs::write(
        root.join("pkg/models.py"),
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection: pass\n",
    )
    .unwrap();
    fs::write(
        root.join("pkg/client.py"),
        "from pkg.models import Connection\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Connection]): pass\n",
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
    let prefix = "from typepact import rust\nclass C: pass\n";
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

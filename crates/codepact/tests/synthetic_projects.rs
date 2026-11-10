//! Project-sized synthetic controls. These never count as a real-project pilot.

use codepact::{check_with_must_use, AnalysisSession, SourceFile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const TARGETS: [&str; 3] = ["3.11", "3.12", "3.13"];
const VIOLATIONS: [&str; 5] = ["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u8,
    provenance: String,
    real_project_pilot_credit: bool,
    oracle: String,
    oracle_revision: u8,
    oracle_revisions: Vec<String>,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    id: String,
    title: String,
    kind: String,
    expected_coverage: Option<String>,
    #[serde(default)]
    cases: Vec<Case>,
    #[serde(default)]
    required_diagnostics: Vec<ExpectedDiagnostic>,
    #[serde(default)]
    required_coverage_text: Vec<String>,
    #[serde(default)]
    allowed_boundary_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    file: String,
    #[serde(rename = "function")]
    function_name: String,
    code: String,
    certainty: String,
    rationale: String,
    repair: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedDiagnostic {
    code: String,
    certainty: String,
}

struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("codepact-synthetic-{}-{name}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/projects")
}

fn manifest() -> Manifest {
    serde_json::from_slice(&fs::read(corpus_root().join("manifest.json")).unwrap()).unwrap()
}

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink(), "unexpected fixture symlink: {path:?}");
            if kind.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn install_snapshot(source: &Path, destination: &Path) {
    for (name, bytes) in files(source) {
        let path = destination.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

fn run_cli(root: &Path, target: &str, extra: &[&str]) -> Output {
    let before = files(root);
    let output = Command::new(env!("CARGO_BIN_EXE_codepact"))
        .current_dir(root)
        .args(["check", "--json", "--python-version", target])
        .args(extra)
        .output()
        .expect("synthetic project CLI starts");
    assert_eq!(
        files(root),
        before,
        "checking must not execute or write source"
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn report(output: &Output) -> Value {
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid CLI JSON: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(report["execution_status"], "completed", "{report:#}");
    assert_eq!(report["check_schema_version"], 2);
    assert_eq!(report["details_truncated"], false);
    assert_eq!(
        report["exit_code"].as_i64(),
        output.status.code().map(i64::from)
    );
    report
}

fn save_report(project: &Project, target: &str, variant: &str, output: &Output) {
    if let Some(directory) = std::env::var_os("CODEPACT_SYNTHETIC_RESULTS_DIR") {
        let directory = PathBuf::from(directory).join(&project.id).join(target);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{variant}.json")), &output.stdout).unwrap();
    }
}

fn containing_function(root: &Path, file: &str, line: usize) -> String {
    fs::read_to_string(root.join("src").join(file))
        .unwrap()
        .lines()
        .take(line)
        .filter_map(|source| {
            source
                .strip_prefix("async def ")
                .or_else(|| source.strip_prefix("def "))
                .and_then(|name| name.split_once('('))
                .map(|(name, _)| name.to_string())
        })
        .last()
        .unwrap_or_else(|| "<module>".into())
}

fn assert_oracle(project: &Project, report: &Value, root: &Path, buggy: bool) {
    assert_eq!(
        report["coverage"]["status"].as_str(),
        project.expected_coverage.as_deref(),
        "{}: {report:#}",
        project.id
    );
    let mut actual = Vec::new();
    for diagnostic in report["diagnostics"].as_array().unwrap() {
        let code = diagnostic["code"].as_str().unwrap();
        if code == "TP9001" {
            assert_eq!(project.expected_coverage.as_deref(), Some("incomplete"));
            assert_eq!(diagnostic["certainty"], "unknown");
            let message = diagnostic["message"].as_str().unwrap();
            assert!(
                project
                    .allowed_boundary_reasons
                    .iter()
                    .any(|text| message.contains(text)),
                "unexpected boundary: {diagnostic:#}"
            );
            continue;
        }
        assert!(
            VIOLATIONS.contains(&code),
            "unexpected diagnostic: {diagnostic:#}"
        );
        let file = diagnostic["primary"]["file"].as_str().unwrap();
        let line = diagnostic["primary"]["line"].as_u64().unwrap() as usize;
        actual.push((
            file.to_string(),
            containing_function(root, file, line),
            code.to_string(),
            diagnostic["certainty"].as_str().unwrap().to_string(),
        ));
        assert_eq!(diagnostic["disposition"], "active");
    }
    actual.sort();
    let mut expected = if buggy {
        project
            .cases
            .iter()
            .map(|case| {
                (
                    case.file.clone(),
                    case.function_name.clone(),
                    case.code.clone(),
                    case.certainty.clone(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    expected.sort();
    assert_eq!(actual, expected, "{}: {report:#}", project.id);
    let reasons = report["coverage"]["reasons"].to_string();
    for text in &project.required_coverage_text {
        assert!(reasons.contains(text), "{}: {reasons}", project.id);
    }
}

fn source_files(root: &Path) -> Vec<SourceFile> {
    files(&root.join("src"))
        .into_iter()
        .filter(|(path, _)| path.ends_with(".py"))
        .map(|(path, source)| {
            let module = path.trim_end_matches(".py").replace('/', ".");
            SourceFile::new(path, module, String::from_utf8(source).unwrap())
        })
        .collect()
}

#[test]
fn synthetic_manifest_declares_independent_oracles_and_no_pilot_credit() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.provenance, "synthetic");
    assert!(!manifest.real_project_pilot_credit);
    assert!(!manifest.oracle.is_empty());
    assert_eq!(manifest.oracle_revision, 3);
    assert!(!manifest.oracle_revisions.is_empty());
    let mut ids = BTreeSet::new();
    let mut total_cases = 0;
    let mut rules = BTreeSet::new();
    for project in manifest.projects {
        assert!(ids.insert(project.id.clone()), "duplicate project id");
        assert!(!project.title.is_empty());
        assert!(project
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if project.kind == "repair" {
            let buggy = files(&corpus_root().join(&project.id).join("buggy"));
            let fixed = files(&corpus_root().join(&project.id).join("fixed"));
            assert_eq!(
                buggy.keys().collect::<Vec<_>>(),
                fixed.keys().collect::<Vec<_>>()
            );
            assert_eq!(buggy["pyproject.toml"], fixed["pyproject.toml"]);
            for (path, source) in &buggy {
                if path != "src/main.py" {
                    assert_eq!(source, &fixed[path], "only main.py should be repaired");
                }
            }
            assert_ne!(buggy, fixed, "a repair must change source");
            let source =
                fs::read_to_string(corpus_root().join(&project.id).join("buggy/src/main.py"))
                    .unwrap();
            for case in project.cases {
                assert!(!case.rationale.is_empty() && !case.repair.is_empty());
                assert!(source.contains(&format!("def {}(", case.function_name)));
                assert_eq!(case.certainty, "definite");
                assert!(VIOLATIONS.contains(&case.code.as_str()));
                rules.insert(case.code);
                total_cases += 1;
            }
        } else {
            assert_eq!(project.kind, "boundary");
            assert!(!project.required_diagnostics.is_empty());
            assert!(corpus_root()
                .join(&project.id)
                .join("boundary/src")
                .is_dir());
        }
    }
    assert_eq!(ids.len(), 12);
    assert_eq!(total_cases, 33);
    assert_eq!(rules, VIOLATIONS.map(String::from).into_iter().collect());
}

#[test]
fn synthetic_projects_match_bug_and_repair_oracles_on_all_targets() {
    for project in manifest()
        .projects
        .into_iter()
        .filter(|p| p.kind == "repair")
    {
        for target in TARGETS {
            let workspace = Workspace::new(&format!("{}-{target}", project.id));
            let fixtures = corpus_root().join(&project.id);
            install_snapshot(&fixtures.join("buggy"), &workspace.0);
            let buggy_output = run_cli(&workspace.0, target, &[]);
            save_report(&project, target, "buggy", &buggy_output);
            assert_eq!(buggy_output.status.code(), Some(1), "{}", project.id);
            let buggy = report(&buggy_output);
            assert_oracle(&project, &buggy, &workspace.0, true);

            install_snapshot(&fixtures.join("fixed"), &workspace.0);
            let fixed_output = run_cli(&workspace.0, target, &[]);
            save_report(&project, target, "fixed", &fixed_output);
            assert_eq!(fixed_output.status.code(), Some(0), "{}", project.id);
            let fixed = report(&fixed_output);
            assert_oracle(&project, &fixed, &workspace.0, false);
            assert_eq!(buggy["policy_fingerprint"], fixed["policy_fingerprint"]);
            assert_eq!(buggy["model_fingerprint"], fixed["model_fingerprint"]);
            assert_eq!(buggy["selected_files"], fixed["selected_files"]);
            let repeated = run_cli(&workspace.0, target, &[]);
            assert_eq!(
                repeated.stdout, fixed_output.stdout,
                "output must be deterministic"
            );
            println!(
                "{} / {target}: {} expected violations, source repair clean",
                project.id,
                project.cases.len()
            );
        }
    }
}

#[test]
fn synthetic_source_repairs_have_fresh_reused_and_input_order_parity() {
    for project in manifest()
        .projects
        .into_iter()
        .filter(|p| p.kind == "repair")
    {
        let fixtures = corpus_root().join(&project.id);
        let buggy = source_files(&fixtures.join("buggy"));
        let fixed = source_files(&fixtures.join("fixed"));
        for target in TARGETS {
            let mut reused = AnalysisSession::default();
            let fresh_buggy = check_with_must_use(&buggy, target);
            assert_eq!(reused.check_with_must_use(&buggy, target), fresh_buggy);
            let fresh_fixed = check_with_must_use(&fixed, target);
            assert_eq!(reused.check_with_must_use(&fixed, target), fresh_fixed);
            assert_eq!(reused.check_with_must_use(&fixed, target), fresh_fixed);
            let reversed = fixed.iter().rev().cloned().collect::<Vec<_>>();
            assert_eq!(check_with_must_use(&reversed, target), fresh_fixed);
            assert_eq!(reused.check_with_must_use(&buggy, target), fresh_buggy);
        }
    }
}

fn canonical_diagnostics(report: &Value) -> Vec<Value> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            json!({"code":d["code"], "primary":d["primary"], "related":d["related"],
            "certainty":d["certainty"], "trust":d["trust"], "completion":d["completion"],
            "fingerprint":d["fingerprint"]})
        })
        .collect()
}

#[test]
fn synthetic_unknown_boundaries_cannot_be_interpreted_as_verified_clean() {
    for project in manifest()
        .projects
        .into_iter()
        .filter(|p| p.kind == "boundary")
    {
        for target in TARGETS {
            let workspace = Workspace::new(&format!("{}-{target}", project.id));
            install_snapshot(
                &corpus_root().join(&project.id).join("boundary"),
                &workspace.0,
            );
            let advisory_output = run_cli(&workspace.0, target, &["--mode", "advisory"]);
            save_report(&project, target, "boundary-advisory", &advisory_output);
            let advisory = report(&advisory_output);
            let strict_output = run_cli(&workspace.0, target, &["--mode", "strict"]);
            save_report(&project, target, "boundary-strict", &strict_output);
            assert_eq!(strict_output.status.code(), Some(1));
            let strict = report(&strict_output);
            assert_eq!(
                canonical_diagnostics(&advisory),
                canonical_diagnostics(&strict)
            );
            for report in [&advisory, &strict] {
                assert_eq!(report["coverage"]["status"], "incomplete", "{report:#}");
                let diagnostics = report["diagnostics"].as_array().unwrap();
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| VIOLATIONS.contains(&d["code"].as_str().unwrap())
                            && d["certainty"] == "definite"),
                    "{}: {report:#}",
                    project.id
                );
                for expected in &project.required_diagnostics {
                    assert!(
                        diagnostics
                            .iter()
                            .any(|d| d["code"] == expected.code
                                && d["certainty"] == expected.certainty),
                        "{}: {report:#}",
                        project.id
                    );
                }
                let reasons = report["coverage"]["reasons"].to_string();
                for text in &project.required_coverage_text {
                    assert!(reasons.contains(text), "{}: {reasons}", project.id);
                }
            }
            println!(
                "{} / {target}: incomplete in both modes; strict refuses clean status",
                project.id
            );
        }
    }
}

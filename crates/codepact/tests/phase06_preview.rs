//! Synthetic valid, bug, repair, and boundary controls for the Phase 0.6 preview.

use codepact::{AnalysisSession, Ownership, SourceFile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PREVIEW: &str = "phase06_preview";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u8,
    provenance: String,
    real_project_pilot_credit: bool,
    oracle: String,
    oracle_revision: u8,
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Feature {
    id: String,
    title: String,
    targets: Vec<String>,
    bug: BugOracle,
    #[serde(default)]
    bug_boundary_targets: Vec<String>,
    #[serde(default)]
    bug_boundary_reasons: Vec<String>,
    boundary_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BugOracle {
    code: String,
    function: String,
    certainty: String,
    trust: String,
    primary_text: String,
    related_text: String,
}

struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "codepact-phase06-preview-{}-{name}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/projects")
        .join(PREVIEW)
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
    let snapshot = files(source);
    // A repair or control is a complete snapshot. Leaving files from the
    // previous variant would silently analyze a different project.
    if destination.exists() {
        fs::remove_dir_all(destination).unwrap();
    }
    fs::create_dir_all(destination).unwrap();
    for (name, bytes) in &snapshot {
        let path = destination.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    assert_eq!(files(destination), snapshot);
}

fn binary() -> PathBuf {
    std::env::var_os("CODEPACT_QUALIFICATION_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codepact")))
}

fn run(root: &Path, target: &str, mode: &str) -> Output {
    let before = files(root);
    let output = Command::new(binary())
        .current_dir(root)
        .args([
            "check",
            "--json",
            "--python-version",
            target,
            "--mode",
            mode,
        ])
        .output()
        .expect("preview CLI starts");
    assert_eq!(
        files(root),
        before,
        "checking a synthetic project must not execute or write its source"
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
    assert_eq!(report["check_schema_version"], 1);
    assert_eq!(report["details_truncated"], false);
    assert_eq!(
        report["exit_code"].as_i64(),
        output.status.code().map(i64::from)
    );
    report
}

fn save_report(feature: &Feature, target: &str, variant: &str, output: &Output) {
    let Some(directory) = std::env::var_os("CODEPACT_PHASE06_PREVIEW_RESULTS_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory).join(&feature.id).join(target);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(format!("{variant}.json")), &output.stdout).unwrap();
}

fn line(root: &Path, span: &Value) -> String {
    let file = span["file"].as_str().unwrap();
    let number = span["line"].as_u64().unwrap() as usize;
    let reported_path = root.join(file);
    let source_path = if reported_path.is_file() {
        reported_path
    } else {
        root.join("src").join(file)
    };
    fs::read_to_string(source_path)
        .unwrap()
        .lines()
        .nth(number - 1)
        .unwrap()
        .trim()
        .to_owned()
}

fn containing_function(root: &Path, line_number: usize) -> String {
    fs::read_to_string(root.join("src/main.py"))
        .unwrap()
        .lines()
        .take(line_number)
        .filter_map(|source| {
            source
                .strip_prefix("async def ")
                .or_else(|| source.strip_prefix("def "))
                .and_then(|name| name.split_once('('))
                .map(|(name, _)| name.to_owned())
        })
        .last()
        .unwrap_or_else(|| "<module>".into())
}

fn assert_clean(report: &Value, project: &str) {
    assert_eq!(
        report["coverage"]["status"], "complete-for-supported-subset",
        "{project}: {report:#}"
    );
    assert!(
        report["diagnostics"].as_array().unwrap().is_empty(),
        "{project}: {report:#}"
    );
}

fn assert_bug(feature: &Feature, target: &str, report: &Value, root: &Path) {
    let expected_bug_boundary = feature
        .bug_boundary_targets
        .iter()
        .any(|boundary_target| boundary_target == target);
    assert_eq!(
        report["coverage"]["status"],
        if expected_bug_boundary {
            "incomplete"
        } else {
            "complete-for-supported-subset"
        },
        "{}: {report:#}",
        feature.id
    );
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(
        diagnostics.len(),
        if expected_bug_boundary { 2 } else { 1 },
        "{}: {report:#}",
        feature.id
    );
    let finding = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == feature.bug.code)
        .expect("independently specified bug finding");
    assert_eq!(finding["code"], feature.bug.code, "{report:#}");
    assert_eq!(finding["certainty"], feature.bug.certainty, "{report:#}");
    assert_eq!(finding["trust"], feature.bug.trust, "{report:#}");
    assert_eq!(finding["completion"], Value::Null, "{report:#}");
    assert_eq!(finding["disposition"], "active");
    assert!(finding["primary"]["file"]
        .as_str()
        .unwrap()
        .ends_with("main.py"));
    let primary_line = line(root, &finding["primary"]);
    assert_eq!(primary_line, feature.bug.primary_text, "{report:#}");
    let primary_number = finding["primary"]["line"].as_u64().unwrap() as usize;
    assert_eq!(
        containing_function(root, primary_number),
        feature.bug.function,
        "{report:#}"
    );
    let related = finding["related"].as_array().unwrap();
    assert!(
        !related.is_empty(),
        "related origin is required: {report:#}"
    );
    assert!(
        related
            .iter()
            .any(|span| line(root, span).contains(&feature.bug.related_text)),
        "expected related source {}: {report:#}",
        feature.bug.related_text
    );
    let boundaries = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "TP9001")
        .collect::<Vec<_>>();
    assert_eq!(boundaries.len(), usize::from(expected_bug_boundary));
    if expected_bug_boundary {
        assert_eq!(boundaries[0]["certainty"], "unknown");
        let reason = boundaries[0]["message"].as_str().unwrap();
        assert!(
            feature
                .bug_boundary_reasons
                .iter()
                .any(|expected| reason.contains(expected)),
            "{}: {reason}",
            feature.id
        );
    }
}

fn canonical_diagnostics(report: &Value) -> Vec<Value> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| {
            json!({
                "code": diagnostic["code"],
                "primary": diagnostic["primary"],
                "related": diagnostic["related"],
                "certainty": diagnostic["certainty"],
                "trust": diagnostic["trust"],
                "completion": diagnostic["completion"],
                "fingerprint": diagnostic["fingerprint"]
            })
        })
        .collect()
}

#[test]
fn phase06_preview_manifest_has_independent_four_way_oracles() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.provenance, "synthetic");
    assert!(!manifest.real_project_pilot_credit);
    assert_eq!(manifest.oracle_revision, 2);
    assert!(!manifest.oracle.is_empty());
    assert_eq!(manifest.features.len(), 4);

    let mut ids = std::collections::BTreeSet::new();
    for feature in manifest.features {
        assert!(ids.insert(feature.id.clone()), "duplicate feature id");
        assert!(!feature.title.is_empty());
        assert!(!feature.targets.is_empty());
        assert!(feature
            .targets
            .iter()
            .all(|target| { ["3.11", "3.12", "3.13"].contains(&target.as_str()) }));
        assert!(!feature.boundary_reasons.is_empty());
        assert!(feature.bug.code == "TP2101" || feature.bug.code == "TP2102");
        for variant in ["buggy", "fixed", "control", "boundary"] {
            let root = corpus_root().join(&feature.id).join(variant);
            assert!(root.join("pyproject.toml").is_file(), "{root:?}");
            assert!(root.join("src/main.py").is_file(), "{root:?}");
        }
        let buggy = files(&corpus_root().join(&feature.id).join("buggy"));
        let fixed = files(&corpus_root().join(&feature.id).join("fixed"));
        assert_ne!(buggy, fixed, "a repair changes source: {}", feature.id);
        assert_eq!(
            buggy.keys().collect::<Vec<_>>(),
            fixed.keys().collect::<Vec<_>>()
        );
        assert_eq!(buggy["pyproject.toml"], fixed["pyproject.toml"]);
        assert!(String::from_utf8(buggy["src/main.py"].clone())
            .unwrap()
            .contains(&feature.bug.primary_text));
    }
    assert_eq!(ids.len(), 4);
}

#[test]
fn phase06_preview_matches_bug_repair_valid_and_boundary_oracles() {
    for feature in manifest().features {
        for target in &feature.targets {
            let fixture = corpus_root().join(&feature.id);
            let workspace = Workspace::new(&format!("{}-{target}", feature.id));

            install_snapshot(&fixture.join("buggy"), &workspace.0);
            let buggy_output = run(&workspace.0, target, "strict");
            save_report(&feature, target, "buggy", &buggy_output);
            assert_eq!(buggy_output.status.code(), Some(1), "{}", feature.id);
            let buggy = report(&buggy_output);
            assert_bug(&feature, target, &buggy, &workspace.0);
            assert_eq!(
                run(&workspace.0, target, "strict").stdout,
                buggy_output.stdout,
                "{} / {target}: seeded-bug report is deterministic",
                feature.id
            );

            install_snapshot(&fixture.join("fixed"), &workspace.0);
            let fixed_output = run(&workspace.0, target, "strict");
            save_report(&feature, target, "fixed", &fixed_output);
            assert_eq!(fixed_output.status.code(), Some(0), "{}", feature.id);
            let fixed = report(&fixed_output);
            assert_clean(&fixed, &feature.id);
            assert_eq!(
                run(&workspace.0, target, "strict").stdout,
                fixed_output.stdout,
                "{} / {target}: repair report is deterministic",
                feature.id
            );

            install_snapshot(&fixture.join("control"), &workspace.0);
            let control_output = run(&workspace.0, target, "strict");
            save_report(&feature, target, "control", &control_output);
            assert_eq!(control_output.status.code(), Some(0), "{}", feature.id);
            let control = report(&control_output);
            assert_clean(&control, &feature.id);
            assert_eq!(
                run(&workspace.0, target, "strict").stdout,
                control_output.stdout,
                "{} / {target}: valid control report is deterministic",
                feature.id
            );

            install_snapshot(&fixture.join("boundary"), &workspace.0);
            let advisory_output = run(&workspace.0, target, "advisory");
            save_report(&feature, target, "boundary-advisory", &advisory_output);
            assert_eq!(advisory_output.status.code(), Some(0), "{}", feature.id);
            let advisory = report(&advisory_output);
            assert_eq!(
                run(&workspace.0, target, "advisory").stdout,
                advisory_output.stdout,
                "{} / {target}: advisory boundary report is deterministic",
                feature.id
            );
            let strict_output = run(&workspace.0, target, "strict");
            save_report(&feature, target, "boundary-strict", &strict_output);
            assert_eq!(strict_output.status.code(), Some(1), "{}", feature.id);
            let strict = report(&strict_output);
            assert_eq!(
                run(&workspace.0, target, "strict").stdout,
                strict_output.stdout,
                "{} / {target}: strict boundary report is deterministic",
                feature.id
            );
            assert_eq!(
                canonical_diagnostics(&advisory),
                canonical_diagnostics(&strict)
            );

            for boundary in [&advisory, &strict] {
                assert_eq!(boundary["coverage"]["status"], "incomplete", "{boundary:#}");
                let diagnostics = boundary["diagnostics"].as_array().unwrap();
                assert!(diagnostics.iter().any(|diagnostic| {
                    diagnostic["code"] == "TP9001" && diagnostic["certainty"] == "unknown"
                }));
                assert!(!diagnostics.iter().any(|diagnostic| {
                    matches!(diagnostic["code"].as_str(), Some("TP2101" | "TP2102"))
                        && diagnostic["certainty"] == "definite"
                }));
                let reasons = boundary["coverage"]["reasons"].to_string();
                assert!(
                    feature
                        .boundary_reasons
                        .iter()
                        .any(|expected| reasons.contains(expected)),
                    "{}: {reasons}",
                    feature.id
                );
            }

            for field in ["policy_fingerprint", "model_fingerprint", "selected_files"] {
                assert_eq!(buggy[field], fixed[field], "{}: {field}", feature.id);
            }
        }
    }
}

fn source_files(root: &Path) -> Vec<SourceFile> {
    files(&root.join("src"))
        .into_iter()
        .filter(|(path, _)| path.ends_with(".py"))
        .map(|(path, source)| {
            let module = path
                .strip_suffix(".py")
                .unwrap()
                .trim_end_matches("/__init__")
                .replace('/', ".");
            SourceFile::new(path, module, String::from_utf8(source).unwrap())
        })
        .collect()
}

#[test]
fn phase06_preview_source_repairs_match_fresh_and_reused_analysis() {
    for feature in manifest().features {
        for target in &feature.targets {
            let fixture = corpus_root().join(&feature.id);
            let buggy = source_files(&fixture.join("buggy"));
            let fixed = source_files(&fixture.join("fixed"));
            let control = source_files(&fixture.join("control"));
            let mut session = AnalysisSession::default();

            let reused_buggy = session.check_with_ownership(
                &buggy,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            let fresh_buggy = codepact::check_with_ownership(
                &buggy,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            assert_eq!(reused_buggy, fresh_buggy, "{} / {target}", feature.id);

            let reused_fixed = session.check_with_ownership(
                &fixed,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            let fresh_fixed = codepact::check_with_ownership(
                &fixed,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            assert_eq!(reused_fixed, fresh_fixed, "{} / {target}", feature.id);
            assert_eq!(
                reused_fixed,
                session.check_with_ownership(
                    &fixed,
                    target,
                    Ownership::AdvancedAssumeDefaultTaskFactory
                )
            );

            let reused_control = session.check_with_ownership(
                &control,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            assert_eq!(
                reused_control,
                codepact::check_with_ownership(
                    &control,
                    target,
                    Ownership::AdvancedAssumeDefaultTaskFactory
                ),
                "{} / {target}",
                feature.id
            );
        }
    }
}

#[test]
fn phase06_preview_multifile_contracts_preserve_oracles_across_every_input_order() {
    let feature = manifest()
        .features
        .into_iter()
        .find(|feature| feature.id == "typestate")
        .unwrap();
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for target in &feature.targets {
        let mut session = AnalysisSession::default();
        for variant in ["buggy", "fixed", "buggy"] {
            let inputs = source_files(&corpus_root().join(&feature.id).join(variant));
            assert_eq!(
                inputs.len(),
                3,
                "the order oracle requires a provider, re-export, and consumer"
            );
            let modules = inputs
                .iter()
                .map(|file| file.module.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                modules,
                std::collections::BTreeSet::from(["facade", "main", "transport"])
            );
            let expected = codepact::check_with_ownership(
                &inputs,
                target,
                Ownership::AdvancedAssumeDefaultTaskFactory,
            );
            assert_eq!(
                expected.coverage.status, "complete-for-supported-subset",
                "{variant} / {target}: {expected:#?}"
            );
            assert!(expected.coverage.reasons.is_empty(), "{expected:#?}");
            if variant == "buggy" {
                assert_eq!(expected.diagnostics.len(), 1, "{expected:#?}");
                let diagnostic = &expected.diagnostics[0];
                assert_eq!(diagnostic.code, feature.bug.code);
                assert_eq!(diagnostic.certainty, feature.bug.certainty);
                assert_eq!(diagnostic.trust, feature.bug.trust);
                assert_eq!(diagnostic.completion, None);
                assert_eq!(diagnostic.coverage_reason, None);
                assert_eq!(diagnostic.primary.file, "main.py");
                assert_eq!(diagnostic.primary.line, 9);
                assert!(
                    diagnostic
                        .related
                        .iter()
                        .any(|site| { site.file == "transport.py" && site.line == 13 }),
                    "provider requirement must survive the re-export: {expected:#?}"
                );
            } else {
                assert!(expected.diagnostics.is_empty(), "{expected:#?}");
            }
            for order in orders {
                let permuted = order.map(|index| inputs[index].clone());
                let fresh = codepact::check_with_ownership(
                    &permuted,
                    target,
                    Ownership::AdvancedAssumeDefaultTaskFactory,
                );
                let reused = session.check_with_ownership(
                    &permuted,
                    target,
                    Ownership::AdvancedAssumeDefaultTaskFactory,
                );
                assert_eq!(fresh, expected, "{variant} / {target} / {order:?}");
                assert_eq!(reused, expected, "{variant} / {target} / {order:?}");
            }
        }
    }
}

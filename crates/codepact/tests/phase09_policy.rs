use codepact::adoption::{build_report, resolve_config, write_baseline, Overrides};
use codepact::{check_with_ownership, SourceFile};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TemporaryProject(PathBuf);

impl TemporaryProject {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "codepact-0.9-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const POLICY: &str = r#"[tool.codepact]
config-version = 1
mode = 'strict'
experimental-ownership = true
[[tool.codepact.declaration-policy]]
module = 'subject'
requirement = 'resource-access'
mode = 'preview'
[[tool.codepact.declaration-policy]]
module = 'subject'
function = 'make'
requirement = 'owned-return'
mode = 'enforce'
[[tool.codepact.declaration-policy]]
module = 'subject'
function = 'fetch'
requirement = 'must-use'
mode = 'enforce'
[[tool.codepact.declaration-policy]]
module = 'subject'
function = 'already_marked'
requirement = 'must-use'
mode = 'enforce'
"#;

const SOURCE: &str = r#"from codepact import resource
from codepact.rust import Borrowed, MustUse, Owned

@resource(closed_by='close')
class R:
    def close(self): pass

def parameter(resource: R): return 1

def make() -> R:
    return R()

def fetch() -> int: return 1

def already_marked() -> MustUse[int]: return 2
"#;

#[test]
fn declaration_policy_preview_enforcement_repair_and_strict_baseline() {
    let root = TemporaryProject::new();
    let config_path = root.path().join("pyproject.toml");
    fs::write(&config_path, POLICY).unwrap();
    let resolve = || resolve_config(root.path(), &Overrides::default()).unwrap();
    let config = resolve();
    let files = [SourceFile::new("subject.py", "subject", SOURCE)];
    let run = |files: &[SourceFile]| {
        check_with_ownership(files, &config.python_version, config.ownership)
    };
    let report = build_report(run(&files), &files, &config, None, None).unwrap();
    let declarations = report
        .diagnostics
        .iter()
        .filter(|finding| finding.diagnostic.code.starts_with("TP41"))
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 3, "{report:#?}");
    assert!(
        !declarations
            .iter()
            .find(|finding| finding.diagnostic.code == "TP4101")
            .unwrap()
            .actionable
    );
    assert!(declarations
        .iter()
        .filter(|finding| finding.diagnostic.code != "TP4101")
        .all(|finding| finding.actionable));

    let baseline = root.path().join("baseline.json");
    write_baseline(
        &baseline,
        &run(&files),
        &files,
        &config,
        false,
        "accept existing declarations",
    )
    .unwrap();
    let suppressed = build_report(run(&files), &files, &config, Some(&baseline), None).unwrap();
    assert_eq!(suppressed.baseline.compatible, Some(true));
    assert!(suppressed
        .diagnostics
        .iter()
        .filter(|finding| finding.diagnostic.code.starts_with("TP41"))
        .all(|finding| finding.disposition == "suppressed-baseline"));
    assert!(suppressed
        .policy_failures
        .iter()
        .any(|failure| failure == "analysis coverage is incomplete"));

    fs::write(
        &config_path,
        POLICY.replacen("mode = 'preview'", "mode = 'enforce'", 1),
    )
    .unwrap();
    let changed = build_report(run(&files), &files, &resolve(), Some(&baseline), None).unwrap();
    assert_eq!(changed.baseline.compatible, Some(false));
    assert_ne!(changed.snapshot_id, suppressed.snapshot_id);

    let fixed = SOURCE
        .replace("resource: R", "resource: Borrowed[R]")
        .replace("make() -> R", "make() -> Owned[R]")
        .replace("fetch() -> int", "fetch() -> MustUse[int]");
    let files = [SourceFile::new("subject.py", "subject", fixed)];
    let fixed = build_report(run(&files), &files, &config, None, None).unwrap();
    assert!(
        fixed
            .diagnostics
            .iter()
            .all(|finding| !finding.diagnostic.code.starts_with("TP41")),
        "{fixed:#?}"
    );
}

#[test]
fn duplicate_declaration_policy_selector_is_invalid() {
    let root = TemporaryProject::new();
    fs::write(root.path().join("pyproject.toml"), format!("{POLICY}\n[[tool.codepact.declaration-policy]]\nmodule='subject'\nrequirement='resource-access'\nmode='enforce'\n")).unwrap();
    let error = resolve_config(root.path(), &Overrides::default()).unwrap_err();
    assert!(error.contains("duplicate declaration-policy"), "{error}");
}

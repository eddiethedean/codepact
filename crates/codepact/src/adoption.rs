use crate::{CheckDiagnostic, CheckResult, SourceFile, Span};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::PythonVersion;
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::Ranged;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const ADOPTION_SCHEMA_VERSION: u8 = 2;
pub const BASELINE_SCHEMA_VERSION: u8 = 1;
pub const FINGERPRINT_ALGORITHM_VERSION: u8 = 3;
pub const RULE_SEMANTIC_VERSION: u8 = 1;
pub const DEFAULT_RULES: &[&str] = &["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"];
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

fn read_configuration(reader: impl Read) -> Result<String, String> {
    let mut raw = String::new();
    reader
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|error| format!("cannot read UTF-8 configuration: {error}"))?;
    if raw.len() as u64 > MAX_CONFIG_BYTES {
        return Err("configuration exceeds the 64 KiB limit".into());
    }
    Ok(raw)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(rename = "config-version")]
    pub config_version: u8,
    pub root: Option<PathBuf>,
    #[serde(rename = "python-version")]
    pub python_version: Option<String>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(rename = "enabled-rules")]
    pub enabled_rules: Option<Vec<String>>,
    pub mode: Option<String>,
    #[serde(rename = "fail-on")]
    pub fail_on: Option<String>,
    pub baseline: Option<PathBuf>,
    #[serde(default)]
    pub severity: BTreeMap<String, String>,
}

fn default_include() -> Vec<String> {
    vec!["**/*.py".into()]
}

#[derive(Clone, Debug)]
pub struct EffectiveConfig {
    pub source: Option<PathBuf>,
    pub root: PathBuf,
    pub python_version: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub enabled_rules: BTreeSet<String>,
    pub mode: String,
    pub fail_on: String,
    pub baseline: Option<PathBuf>,
    pub severity: BTreeMap<String, String>,
    pub settings_origins: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub root: Option<PathBuf>,
    pub python_version: Option<String>,
    pub mode: Option<String>,
    pub fail_on: Option<String>,
    pub rules: Option<Vec<String>>,
    pub baseline: Option<PathBuf>,
    pub severity: Option<BTreeMap<String, String>>,
    pub no_config: bool,
    pub config: Option<PathBuf>,
}

fn normalize_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

pub fn reject_symlink_components(path: &Path, kind: &str) -> Result<(), String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot determine current directory: {e}"))?
            .join(path)
    };
    let mut component_path = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                component_path.push(component.as_os_str());
                continue;
            }
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => {
                component_path.pop();
                continue;
            }
            std::path::Component::Normal(_) => component_path.push(component.as_os_str()),
        }
        match fs::symlink_metadata(&component_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                // macOS exposes these system paths through stable aliases.
                // Rust's temp_dir commonly returns /var/folders, so rejecting
                // the /var -> private/var compatibility link would reject
                // every ordinary temporary project before examining any
                // project-controlled path component.
                let approved_macos_alias = cfg!(target_os = "macos")
                    && ((component_path == Path::new("/var")
                        && fs::read_link(&component_path)
                            .is_ok_and(|target| target == Path::new("private/var")))
                        || (component_path == Path::new("/tmp")
                            && fs::read_link(&component_path)
                                .is_ok_and(|target| target == Path::new("private/tmp"))));
                if approved_macos_alias {
                    continue;
                }
                return Err(format!(
                    "{kind} path contains a symlink component: {}",
                    component_path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot inspect {kind} path {}: {error}",
                    component_path.display()
                ));
            }
        }
    }
    Ok(())
}

pub fn resolve_config(cwd: &Path, overrides: &Overrides) -> Result<EffectiveConfig, String> {
    if overrides.config.is_some() && overrides.no_config {
        return Err("--config conflicts with --no-config".into());
    }
    let search_from = overrides
        .root
        .as_deref()
        .map(|p| normalize_path(cwd, p))
        .unwrap_or_else(|| cwd.to_path_buf());
    let config_path = if let Some(path) = &overrides.config {
        Some(normalize_path(cwd, path))
    } else if overrides.no_config {
        None
    } else {
        let mut at = search_from.clone();
        loop {
            let candidate = at.join("pyproject.toml");
            if candidate.is_file() {
                break Some(candidate);
            }
            if !at.pop() {
                break None;
            }
        }
    };
    let config_path = if let Some(path) = config_path {
        reject_symlink_components(&path, "configuration")?;
        if !path.is_file() {
            return Err(format!(
                "configuration file does not exist: {}",
                path.display()
            ));
        }
        let resolved = fs::canonicalize(&path).map_err(|error| {
            format!(
                "cannot resolve configuration file {}: {error}",
                path.display()
            )
        })?;
        Some(resolved)
    } else {
        None
    };
    let mut config = None;
    if let Some(path) = &config_path {
        let file = fs::File::open(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err("configuration exceeds the 64 KiB limit".into());
        }
        // Bound the read itself as well as the metadata preflight: a file can
        // grow after its length is checked.
        let raw = read_configuration(file)?;
        let value: toml::Value =
            toml::from_str(&raw).map_err(|e| format!("invalid TOML configuration: {e}"))?;
        if let Some(table) = value.get("tool").and_then(|v| v.get("codepact")) {
            let parsed: ProjectConfig =
                table.clone().try_into().map_err(|e: toml::de::Error| {
                    format!("invalid [tool.codepact] configuration: {e}")
                })?;
            validate_config(&parsed)?;
            config = Some(parsed);
        }
    }
    let config_dir = config_path.as_deref().and_then(Path::parent).unwrap_or(cwd);
    let root = overrides
        .root
        .as_deref()
        .map(|p| normalize_path(cwd, p))
        .or_else(|| {
            config
                .as_ref()
                .and_then(|c| c.root.as_deref().map(|p| normalize_path(config_dir, p)))
        })
        .unwrap_or_else(|| {
            if config.is_some() {
                config_dir.to_path_buf()
            } else {
                cwd.to_path_buf()
            }
        });
    let c = config;
    let mut rules: BTreeSet<String> = c
        .as_ref()
        .and_then(|c| c.enabled_rules.clone())
        .unwrap_or_else(|| DEFAULT_RULES.iter().map(|r| r.to_string()).collect())
        .into_iter()
        .collect();
    if let Some(override_rules) = &overrides.rules {
        let mut seen = BTreeSet::new();
        for rule in override_rules {
            if !seen.insert(rule) {
                return Err(format!("duplicate rule identifier {rule}"));
            }
        }
        rules = override_rules.iter().cloned().collect();
    }
    for r in &rules {
        validate_rule(r)?;
    }
    let mut severity = c.as_ref().map(|c| c.severity.clone()).unwrap_or_default();
    if let Some(overrides) = &overrides.severity {
        severity.extend(overrides.clone());
    }
    for (rule, value) in &severity {
        validate_rule(rule)?;
        if !severity_overridable(rule) {
            return Err(format!(
                "severity for mandatory rule {rule} cannot be overridden"
            ));
        }
        if !matches!(value.as_str(), "error" | "warning" | "info") {
            return Err(format!("invalid severity for {rule}: {value}"));
        }
    }
    if severity.keys().any(|r| !rules.contains(r)) {
        return Err("severity overrides may only name enabled rules".into());
    }
    let python_version = overrides
        .python_version
        .clone()
        .or_else(|| c.as_ref().and_then(|c| c.python_version.clone()))
        .unwrap_or_else(|| "3.11".into());
    if !matches!(python_version.as_str(), "3.11" | "3.12" | "3.13") {
        return Err(format!("unsupported Python target version {python_version}; supported targets are 3.11, 3.12 and 3.13"));
    }
    let mode = overrides
        .mode
        .clone()
        .or_else(|| c.as_ref().and_then(|c| c.mode.clone()))
        .unwrap_or_else(|| "advisory".into());
    let fail_on = overrides
        .fail_on
        .clone()
        .or_else(|| c.as_ref().and_then(|c| c.fail_on.clone()))
        .unwrap_or_else(|| "error".into());
    if !matches!(mode.as_str(), "advisory" | "strict") {
        return Err("mode must be advisory or strict".into());
    }
    if !matches!(fail_on.as_str(), "error" | "warning") {
        return Err("fail-on must be error or warning".into());
    }
    let baseline = overrides.baseline.clone().or_else(|| {
        c.as_ref()
            .and_then(|c| c.baseline.as_ref().map(|p| normalize_path(config_dir, p)))
    });
    let include = c
        .as_ref()
        .and_then(|c| c.include.clone())
        .unwrap_or_else(default_include);
    let exclude = c
        .as_ref()
        .and_then(|c| c.exclude.clone())
        .unwrap_or_default();
    for pattern in include.iter().chain(exclude.iter()) {
        validate_pattern(pattern)?;
    }
    let mut settings_origins = BTreeMap::new();
    let origin = |cli: bool, configured: bool| {
        if cli {
            "command-line"
        } else if configured {
            "pyproject.toml"
        } else {
            "default"
        }
    };
    settings_origins.insert(
        "root".into(),
        if overrides.root.is_some() {
            "command-line".into()
        } else if c.as_ref().is_some_and(|config| config.root.is_some()) {
            "pyproject.toml".into()
        } else if c.is_some() {
            "pyproject-directory".into()
        } else {
            "current-directory".into()
        },
    );
    settings_origins.insert(
        "python-version".into(),
        origin(
            overrides.python_version.is_some(),
            c.as_ref()
                .is_some_and(|config| config.python_version.is_some()),
        )
        .into(),
    );
    settings_origins.insert(
        "mode".into(),
        origin(
            overrides.mode.is_some(),
            c.as_ref().is_some_and(|config| config.mode.is_some()),
        )
        .into(),
    );
    settings_origins.insert(
        "fail-on".into(),
        origin(
            overrides.fail_on.is_some(),
            c.as_ref().is_some_and(|config| config.fail_on.is_some()),
        )
        .into(),
    );
    settings_origins.insert(
        "enabled-rules".into(),
        origin(
            overrides.rules.is_some(),
            c.as_ref()
                .is_some_and(|config| config.enabled_rules.is_some()),
        )
        .into(),
    );
    settings_origins.insert(
        "include".into(),
        if c.as_ref().is_some_and(|config| config.include.is_some()) {
            "pyproject.toml".into()
        } else {
            "default".into()
        },
    );
    settings_origins.insert(
        "exclude".into(),
        if c.as_ref().is_some_and(|config| config.exclude.is_some()) {
            "pyproject.toml".into()
        } else {
            "default".into()
        },
    );
    settings_origins.insert(
        "baseline".into(),
        origin(
            overrides.baseline.is_some(),
            c.as_ref().is_some_and(|config| config.baseline.is_some()),
        )
        .into(),
    );
    for rule in severity.keys() {
        settings_origins.insert(
            format!("severity.{rule}"),
            if overrides
                .severity
                .as_ref()
                .is_some_and(|items| items.contains_key(rule))
            {
                "command-line".into()
            } else {
                "pyproject.toml".into()
            },
        );
    }
    Ok(EffectiveConfig {
        source: config_path,
        root,
        python_version,
        include,
        exclude,
        enabled_rules: rules,
        mode,
        fail_on,
        baseline,
        severity,
        settings_origins,
    })
}

fn validate_config(c: &ProjectConfig) -> Result<(), String> {
    if c.config_version != 1 {
        return Err(format!(
            "unsupported config-version {}; expected 1",
            c.config_version
        ));
    }
    for p in c.include.iter().flatten().chain(c.exclude.iter().flatten()) {
        validate_pattern(p)?;
    }
    if let Some(rules) = &c.enabled_rules {
        let mut seen = BTreeSet::new();
        for r in rules {
            validate_rule(r)?;
            if !seen.insert(r) {
                return Err(format!("duplicate rule identifier {r}"));
            }
        }
    }
    if let Some(v) = &c.python_version {
        if !matches!(v.as_str(), "3.11" | "3.12" | "3.13") {
            return Err(format!("unsupported Python target version {v}"));
        }
    }
    if let Some(v) = &c.mode {
        if !matches!(v.as_str(), "advisory" | "strict") {
            return Err("mode must be advisory or strict".into());
        }
    }
    if let Some(v) = &c.fail_on {
        if !matches!(v.as_str(), "error" | "warning") {
            return Err("fail-on must be error or warning".into());
        }
    }
    for (r, v) in &c.severity {
        validate_rule(r)?;
        if !severity_overridable(r) {
            return Err(format!(
                "severity for mandatory rule {r} cannot be overridden"
            ));
        }
        if !matches!(v.as_str(), "error" | "warning" | "info") {
            return Err(format!("invalid severity for {r}: {v}"));
        }
    }
    Ok(())
}
fn severity_overridable(rule: &str) -> bool {
    matches!(rule, "TP1001" | "TP1002" | "TP1003" | "TP2001" | "TP3001")
}
fn validate_rule(r: &str) -> Result<(), String> {
    if matches!(
        r,
        "TP1001"
            | "TP1002"
            | "TP1003"
            | "TP2001"
            | "TP3001"
            | "TP4001"
            | "TP4002"
            | "TP9001"
            | "TP9002"
    ) {
        Ok(())
    } else {
        Err(format!("unknown rule identifier {r}"))
    }
}
fn validate_pattern(p: &str) -> Result<(), String> {
    if p.is_empty()
        || p.starts_with('/')
        || p.contains('\\')
        || p.split('/').any(|part| {
            part.is_empty() || part == ".." || part == "." || (part.contains("**") && part != "**")
        })
    {
        Err(format!("invalid root-relative path pattern {p:?}"))
    } else {
        Ok(())
    }
}

fn glob_match(pattern: &str, path: &str) -> bool {
    fn component(p: &str, s: &str) -> bool {
        let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
        let pattern = p.chars().collect::<Vec<_>>();
        let value = s.chars().collect::<Vec<_>>();
        while j < value.len() {
            if i < pattern.len()
                && (pattern[i] == '*' || pattern[i] == '?' || pattern[i] == value[j])
            {
                if pattern[i] == '*' {
                    star = Some(i);
                    mark = j;
                    i += 1;
                } else {
                    i += 1;
                    j += 1;
                }
            } else if let Some(si) = star {
                i = si + 1;
                mark += 1;
                j = mark;
            } else {
                return false;
            }
        }
        while i < pattern.len() && pattern[i] == '*' {
            i += 1;
        }
        i == pattern.len()
    }
    let patterns = pattern.split('/').collect::<Vec<_>>();
    let segments = path.split('/').collect::<Vec<_>>();
    let mut matched = vec![false; segments.len() + 1];
    matched[0] = true;
    for part in patterns {
        let mut next = vec![false; segments.len() + 1];
        if part == "**" {
            next[0] = matched[0];
            for index in 1..=segments.len() {
                // `**` either consumes no path components or extends the match
                // already formed by consuming the preceding component.
                next[index] = matched[index] || next[index - 1];
            }
        } else {
            for index in 1..=segments.len() {
                next[index] = matched[index - 1] && component(part, segments[index - 1]);
            }
        }
        matched = next;
    }
    matched[segments.len()]
}
pub fn included(config: &EffectiveConfig, root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let rel = rel.to_string_lossy().replace('\\', "/");
    let skipped = rel.split('/').any(|p| {
        matches!(
            p,
            ".git" | ".venv" | "venv" | "target" | "build" | "dist" | "__pycache__"
        )
    });
    !skipped
        && config.include.iter().any(|p| glob_match(p, &rel))
        && !config.exclude.iter().any(|p| glob_match(p, &rel))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdoptionDiagnostic {
    #[serde(flatten)]
    pub diagnostic: CheckDiagnostic,
    pub fingerprint: String,
    pub base_severity: String,
    pub effective_severity: String,
    pub disposition: String,
    pub disposition_reason: Option<String>,
    pub explanation_id: String,
    pub actionable: bool,
    pub message_omitted_bytes: usize,
    pub repair_omitted_bytes: usize,
    pub related_sites_omitted: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdoptionReport {
    pub execution_status: String,
    pub engine_version: String,
    pub report_schema_version: u8,
    pub check_schema_version: u8,
    pub target_python_version: String,
    pub model_catalog_version: u8,
    pub available_models: Vec<crate::CheckModelMetadata>,
    pub assurance: String,
    pub contracts: Vec<crate::Contract>,
    pub coverage: crate::Coverage,
    pub diagnostics: Vec<AdoptionDiagnostic>,
    pub counts: BTreeMap<String, usize>,
    pub counts_by_certainty: BTreeMap<String, usize>,
    pub counts_by_disposition: BTreeMap<String, usize>,
    pub total_diagnostics: usize,
    pub actionable_diagnostics: usize,
    pub returned_diagnostics: usize,
    pub audit_diagnostics: usize,
    pub policy_failures: Vec<String>,
    pub policy_warnings: Vec<String>,
    pub unused_suppressions: Vec<String>,
    pub details_truncated: bool,
    pub snapshot_algorithm_version: u8,
    pub snapshot_id: String,
    pub semantic_fingerprint: String,
    pub policy_fingerprint: String,
    pub model_fingerprint: String,
    pub snapshot_manifest: Vec<SnapshotFile>,
    pub project_root: String,
    pub logical_root: String,
    pub config_file: Option<String>,
    pub selected_files: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub selected_rules: Vec<String>,
    pub disabled_rules: Vec<String>,
    pub mode: String,
    pub fail_on: String,
    pub settings_origins: BTreeMap<String, String>,
    pub baseline: BaselineSummary,
    pub exit_code: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub logical_path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BaselineSummary {
    pub configured: bool,
    pub compatible: Option<bool>,
    pub entries: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub update_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselineUpdateSummary {
    pub baseline_schema_version: u8,
    pub output_path: String,
    pub entries: usize,
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub skipped_ambiguous: usize,
    pub snapshot_id: String,
    pub coverage_status: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    fingerprint_algorithm_version: u8,
    baseline_schema_version: u8,
    engine_version: String,
    rule_semantic_version: u8,
    target_python_version: String,
    model_catalog_version: u8,
    snapshot_id: String,
    source_manifest: Vec<SnapshotFile>,
    logical_root: String,
    include: Vec<String>,
    exclude: Vec<String>,
    selected_rules: Vec<String>,
    coverage_status: String,
    coverage_reasons: Vec<String>,
    update_reason: String,
    entries: Vec<BaselineEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct BaselineEntry {
    fingerprint: String,
    code: String,
    logical_path: String,
    certainty: String,
    trust: String,
    completion: Option<String>,
    explanation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BaselineKey {
    fingerprint: String,
    code: String,
    logical_path: String,
    certainty: String,
    trust: String,
    completion: Option<String>,
}

impl From<&BaselineEntry> for BaselineKey {
    fn from(entry: &BaselineEntry) -> Self {
        Self {
            fingerprint: entry.fingerprint.clone(),
            code: entry.code.clone(),
            logical_path: entry.logical_path.clone(),
            certainty: entry.certainty.clone(),
            trust: entry.trust.clone(),
            completion: entry.completion.clone(),
        }
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_logical_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn validate_baseline_document(baseline: &Baseline) -> Result<(), String> {
    if baseline.baseline_schema_version != BASELINE_SCHEMA_VERSION {
        return Err("unsupported baseline schema version".into());
    }
    if baseline.fingerprint_algorithm_version != FINGERPRINT_ALGORITHM_VERSION {
        return Err("unsupported baseline fingerprint algorithm version".into());
    }
    if baseline.engine_version.is_empty()
        || baseline.engine_version.len() > 64
        || baseline.target_python_version.is_empty()
        || baseline.target_python_version.len() > 16
        || baseline.update_reason.trim().is_empty()
        || baseline.update_reason.len() > 8 * 1024
        || !valid_sha256(&baseline.snapshot_id)
    {
        return Err("baseline has invalid identity or update-reason fields".into());
    }
    if !matches!(
        baseline.coverage_status.as_str(),
        "complete-for-supported-subset" | "incomplete"
    ) {
        return Err("baseline has an invalid coverage status".into());
    }
    if baseline.logical_root != "."
        || baseline.source_manifest.len() > crate::DEFAULT_MAX_INPUT_FILES
    {
        return Err("baseline has an invalid logical root or source manifest size".into());
    }
    for pattern in baseline.include.iter().chain(&baseline.exclude) {
        validate_pattern(pattern)?;
    }
    let mut paths = BTreeSet::new();
    for entry in &baseline.source_manifest {
        if !valid_logical_path(&entry.logical_path)
            || !valid_sha256(&entry.sha256)
            || !paths.insert(entry.logical_path.as_str())
        {
            return Err("baseline source manifest has an invalid or duplicate entry".into());
        }
    }
    let mut selected_rules = BTreeSet::new();
    for rule in &baseline.selected_rules {
        validate_rule(rule)?;
        if !selected_rules.insert(rule) {
            return Err("baseline has duplicate selected rules".into());
        }
    }
    if baseline.entries.len() > 100_000 {
        return Err("baseline exceeds the 100,000 entry limit".into());
    }
    let mut keys = BTreeSet::new();
    for entry in &baseline.entries {
        validate_rule(&entry.code)?;
        if !eligible_code(&entry.code)
            || !valid_sha256(&entry.fingerprint)
            || !valid_logical_path(&entry.logical_path)
            || !matches!(
                entry.certainty.as_str(),
                "definite" | "possible" | "unknown"
            )
            || entry.trust.is_empty()
            || entry.trust.len() > 128
            || entry.explanation.is_empty()
            || entry.explanation.len() > 8 * 1024
            || entry.completion.as_deref().is_some_and(|completion| {
                !matches!(
                    completion,
                    "fallthrough" | "return" | "raise" | "cancelled" | "break" | "continue"
                )
            })
        {
            return Err("baseline contains an invalid finding entry".into());
        }
        if !keys.insert(BaselineKey::from(entry)) {
            return Err("baseline contains duplicate entries".into());
        }
    }
    Ok(())
}

const MAX_BASELINE_BYTES: u64 = 16 * 1024 * 1024;

fn load_baseline_file(path: &Path, description: &str) -> Result<(Baseline, String), String> {
    reject_symlink_components(path, description)?;
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {description} {}: {error}", path.display()))?;
    if !path_metadata.file_type().is_file() {
        return Err(format!("{description} must be a regular file"));
    }
    let file = fs::File::open(path)
        .map_err(|error| format!("cannot read {description} {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot inspect {description} {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{description} must be a regular file"));
    }
    if metadata.len() > MAX_BASELINE_BYTES {
        return Err(format!("{description} exceeds the 16 MiB limit"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BASELINE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {description}: {error}"))?;
    if bytes.len() as u64 > MAX_BASELINE_BYTES {
        return Err(format!("{description} exceeds the 16 MiB limit"));
    }
    let loaded: Baseline = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {description}: {error}"))?;
    validate_baseline_document(&loaded).map_err(|error| {
        if description == "baseline" {
            error
        } else {
            format!("{description}: {error}")
        }
    })?;
    Ok((loaded, hash(&bytes)))
}

pub fn validate_baseline_file(path: &Path) -> Result<(), String> {
    load_baseline_file(path, "baseline").map(|_| ())
}

fn hash(data: &[u8]) -> String {
    let d = Sha256::digest(data);
    d.iter().map(|b| format!("{b:02x}")).collect()
}
fn code_before_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let (mut i, mut quote, mut triple, mut escaped) = (0usize, None::<u8>, false, false);
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if c == b'\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if triple && i + 2 < b.len() && b[i] == q && b[i + 1] == q && b[i + 2] == q {
                quote = None;
                triple = false;
                i += 3;
                continue;
            }
            if !triple && c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'#' {
            return &line[..i];
        }
        if c == b'\'' || c == b'"' {
            quote = Some(c);
            triple = i + 2 < b.len() && b[i + 1] == c && b[i + 2] == c;
            if triple {
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    line
}
fn normalized_code(line: &str) -> String {
    let source = code_before_comment(line);
    let mut out = String::with_capacity(source.len());
    let mut quote = None;
    let mut escaped = false;
    let mut pending_space = false;
    for character in source.trim().chars() {
        if let Some(delimiter) = quote {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
        } else if character == '\'' || character == '"' {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            quote = Some(character);
            out.push(character);
        } else if character.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        }
    }
    out
}

fn enclosing_declaration(source: &str, line: usize) -> String {
    let mut enclosing = Vec::<(usize, String)>::new();
    for candidate in source.lines().take(line) {
        let code = code_before_comment(candidate);
        let trimmed = code.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = code
            .chars()
            .take_while(|character| character.is_whitespace())
            .fold(0usize, |column, character| match character {
                '\t' => column + (8 - column % 8),
                _ => column + 1,
            });
        while enclosing
            .last()
            .is_some_and(|(previous_indent, _)| *previous_indent >= indent)
        {
            enclosing.pop();
        }
        let is_declaration = trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.starts_with("class ");
        if is_declaration {
            enclosing.push((indent, normalized_code(trimmed)));
        }
    }
    if enclosing.is_empty() {
        "<module>".into()
    } else {
        enclosing
            .into_iter()
            .map(|(_, declaration)| declaration)
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn normalized_span(source: &str, start: usize, end: usize) -> String {
    source
        .lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start.saturating_sub(1)).max(1))
        .map(normalized_code)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn source_comments(source: &str) -> Vec<(usize, &str)> {
    let mut comments = Vec::new();
    // Use the same Python lexer family as source analysis. In particular, this
    // keeps quotes inside PEP 701 f-string expressions from ending the outer
    // string and turning string contents into apparent suppression comments.
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY313),
    );
    let mut line = 1usize;
    let mut offset = 0usize;
    for token in parsed.tokens() {
        if token.kind() != TokenKind::Comment {
            continue;
        }
        let start = token.start().to_usize();
        let end = token.end().to_usize();
        line += source[offset..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        comments.push((line, &source[start..end]));
        offset = end;
    }
    comments
}

fn fingerprint(diagnostic: &CheckDiagnostic, contents: &BTreeMap<&str, &str>) -> String {
    let source = contents
        .get(diagnostic.primary.file.as_str())
        .copied()
        .unwrap_or("");
    let related = diagnostic
        .related
        .iter()
        .map(|span| {
            let related_source = contents.get(span.file.as_str()).copied().unwrap_or("");
            format!(
                "{}:{}:{}",
                span.file,
                enclosing_declaration(related_source, span.line),
                normalized_span(related_source, span.line, span.end_line)
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("|");
    let encoding = [
        format!("fingerprint-v{}", FINGERPRINT_ALGORITHM_VERSION),
        RULE_SEMANTIC_VERSION.to_string(),
        diagnostic.code.clone(),
        diagnostic.primary.file.clone(),
        enclosing_declaration(source, diagnostic.primary.line),
        normalized_span(source, diagnostic.primary.line, diagnostic.primary.end_line),
        diagnostic.certainty.clone(),
        diagnostic.trust.clone(),
        diagnostic.completion.clone().unwrap_or_default(),
        related,
    ]
    .join("\0");
    hash(encoding.as_bytes())
}
fn eligible(d: &CheckDiagnostic) -> bool {
    matches!(
        d.code.as_str(),
        "TP1001" | "TP1002" | "TP1003" | "TP2001" | "TP3001"
    )
}

fn bound_diagnostic_text(text: &mut String, maximum_bytes: usize) -> usize {
    if text.len() <= maximum_bytes {
        return 0;
    }
    let original_length = text.len();
    let ellipsis_length = '…'.len_utf8();
    let mut end = if maximum_bytes >= ellipsis_length {
        maximum_bytes - ellipsis_length
    } else {
        maximum_bytes
    };
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    if maximum_bytes >= ellipsis_length {
        text.push('…');
    }
    original_length.saturating_sub(end)
}

struct LimitedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for LimitedVecWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other(
                "serialized document exceeds its byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_pretty_limited<T: Serialize>(
    value: &T,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut writer = LimitedVecWriter {
        bytes: Vec::with_capacity(8192),
        limit,
        exceeded: false,
    };
    serde_json::to_writer_pretty(&mut writer, value).map_err(|error| {
        if writer.exceeded {
            format!("{label} exceeds the {} byte limit", writer.limit)
        } else {
            format!("cannot serialize {label}: {error}")
        }
    })?;
    Ok(writer.bytes)
}

pub fn serialize_report(report: &AdoptionReport) -> Result<String, String> {
    String::from_utf8(serialize_pretty_limited(
        report,
        32 * 1024 * 1024,
        "checker report",
    )?)
    .map_err(|error| format!("checker report is not UTF-8: {error}"))
}

fn parse_suppression(line: &str) -> Result<Option<(Vec<String>, String)>, String> {
    let comment = code_before_comment(line);
    let Some(hash) = line[comment.len()..]
        .find('#')
        .map(|i| &line[comment.len() + i + 1..])
    else {
        return Ok(None);
    };
    let directive = hash.trim_start();
    let Some(directive) = directive.strip_prefix("codepact: ignore[") else {
        return if directive.starts_with("codepact:") {
            Err("unsupported CodePact comment; use `codepact: ignore[CODE] -- reason`".into())
        } else {
            Ok(None)
        };
    };
    let Some(close) = directive.find(']') else {
        return Err("malformed CodePact suppression; expected a closing ]".into());
    };
    let codes = directive[..close]
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if codes.is_empty() || codes.iter().any(String::is_empty) {
        return Err("CodePact suppression requires at least one rule code".into());
    }
    let rest = directive[close + 1..].trim();
    let Some(reason) = rest.strip_prefix("--").map(str::trim) else {
        return Err("CodePact suppression requires `-- reason`".into());
    };
    if reason.is_empty() {
        return Err("CodePact suppression reason must not be empty".into());
    }
    let mut seen = BTreeSet::new();
    for code in &codes {
        validate_rule(code).map_err(|_| format!("unknown suppression rule {code}"))?;
        if !eligible_code(code) {
            return Err(format!("rule {code} cannot be suppressed"));
        }
        if !seen.insert(code) {
            return Err(format!("duplicate suppression rule {code}"));
        }
    }
    Ok(Some((codes, reason.to_string())))
}
fn eligible_code(code: &str) -> bool {
    matches!(code, "TP1001" | "TP1002" | "TP1003" | "TP2001" | "TP3001")
}

fn suppression_at(source: &str, target_line: usize, code: &str) -> Result<Option<String>, String> {
    let Some((_, comment)) = source_comments(source)
        .into_iter()
        .find(|(line, _)| *line == target_line)
    else {
        return Ok(None);
    };
    Ok(parse_suppression(comment)?.and_then(|(codes, reason)| {
        codes
            .iter()
            .any(|candidate| candidate == code)
            .then_some(reason)
    }))
}

pub fn build_report(
    mut result: CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    baseline_path: Option<&Path>,
    max: Option<usize>,
) -> Result<AdoptionReport, String> {
    let mut baseline_content_hash = String::new();
    let baseline = if let Some(path) = baseline_path {
        let (loaded, content_hash) = load_baseline_file(path, "baseline")?;
        baseline_content_hash = content_hash;
        let unique = loaded
            .entries
            .iter()
            .map(BaselineKey::from)
            .collect::<BTreeSet<_>>();
        let compatible = loaded.target_python_version == result.target_python_version
            && loaded.model_catalog_version == result.model_catalog_version
            && loaded.rule_semantic_version == RULE_SEMANTIC_VERSION
            && loaded.engine_version == env!("CARGO_PKG_VERSION");
        Some((unique, compatible, loaded.update_reason))
    } else {
        None
    };

    let mut contents = BTreeMap::new();
    for file in files {
        contents.insert(file.path.as_str(), file.source.as_str());
    }
    let mut snapshot_manifest = files
        .iter()
        .map(|file| SnapshotFile {
            logical_path: file.path.clone(),
            sha256: hash(file.source.as_bytes()),
        })
        .collect::<Vec<_>>();
    snapshot_manifest.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
    let semantic_fingerprint = hash(
        serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            result.check_schema_version,
            config.python_version.as_str(),
            result.model_catalog_version,
            &snapshot_manifest,
        ))
        .map_err(|error| format!("cannot encode semantic fingerprint: {error}"))?
        .as_slice(),
    );
    let model_fingerprint = hash(
        serde_json::to_vec(&(result.model_catalog_version, &result.available_models))
            .map_err(|error| format!("cannot encode model fingerprint: {error}"))?
            .as_slice(),
    );
    let suppression_policy = files
        .iter()
        .flat_map(|file| {
            source_comments(&file.source)
                .into_iter()
                .filter(|(_, comment)| {
                    comment
                        .strip_prefix('#')
                        .is_some_and(|text| text.trim_start().starts_with("codepact:"))
                })
                .map(move |(line, comment)| format!("{}:{line}:{comment}", file.path))
        })
        .collect::<Vec<_>>();
    let policy_fingerprint = hash(
        serde_json::to_vec(&(
            &config.enabled_rules,
            &config.severity,
            config.mode.as_str(),
            config.fail_on.as_str(),
            &config.include,
            &config.exclude,
            &baseline_content_hash,
            &suppression_policy,
        ))
        .map_err(|error| format!("cannot encode policy fingerprint: {error}"))?
        .as_slice(),
    );
    let snapshot_id = hash(
        format!(
            "{}\0{}\0{}",
            semantic_fingerprint, policy_fingerprint, model_fingerprint
        )
        .as_bytes(),
    );

    let mut candidates = BTreeMap::<String, usize>::new();
    for diagnostic in &result.diagnostics {
        *candidates
            .entry(fingerprint(diagnostic, &contents))
            .or_default() += 1;
    }

    let mut policy_failures = Vec::<String>::new();
    let mut policy_warnings = Vec::<String>::new();
    let mut suppression_records = BTreeMap::<(String, usize), (Vec<String>, String)>::new();
    for file in files {
        for (line_number, comment) in source_comments(&file.source) {
            if !comment
                .strip_prefix('#')
                .is_some_and(|text| text.trim_start().starts_with("codepact:"))
            {
                continue;
            }
            match parse_suppression(comment) {
                Err(message) => {
                    policy_failures.push(format!("TP4002 {}:{line_number}: {message}", file.path));
                    result.diagnostics.push(suppression_diagnostic(
                        &file.path,
                        line_number,
                        1,
                        message,
                    ));
                }
                Ok(Some((codes, reason))) => {
                    suppression_records.insert((file.path.clone(), line_number), (codes, reason));
                }
                Ok(None) => {}
            }
        }
    }
    let mut unused_suppressions = Vec::new();
    for ((path, line), (codes, _)) in &suppression_records {
        if !result.diagnostics.iter().any(|diagnostic| {
            diagnostic.primary.file == *path
                && diagnostic.primary.line == *line
                && codes.iter().any(|code| code == &diagnostic.code)
        }) {
            unused_suppressions.push(format!("{path}:{line}"));
        }
    }
    unused_suppressions.sort();

    let mut counts = BTreeMap::new();
    let mut counts_by_certainty = BTreeMap::new();
    let mut counts_by_disposition = BTreeMap::new();
    let mut matched_entries = BTreeSet::new();
    let mut actionable_count = 0usize;
    let mut raw = Vec::new();
    for mut diagnostic in result.diagnostics {
        let fp = fingerprint(&diagnostic, &contents);
        let is_eligible = eligible(&diagnostic);
        let mut disposition = "active".to_string();
        let mut reason = None;
        if is_eligible && !config.enabled_rules.contains(&diagnostic.code) {
            disposition = "rule-disabled".into();
            reason = Some("rule disabled by effective configuration".into());
        }
        if is_eligible && disposition == "active" {
            if let Some((codes, suppression_reason)) =
                suppression_records.get(&(diagnostic.primary.file.clone(), diagnostic.primary.line))
            {
                if codes.iter().any(|code| code == &diagnostic.code) {
                    disposition = "suppressed-inline".into();
                    reason = Some(suppression_reason.clone());
                }
            }
        }
        let baseline_entry = BaselineEntry {
            fingerprint: fp.clone(),
            code: diagnostic.code.clone(),
            logical_path: diagnostic.primary.file.clone(),
            certainty: diagnostic.certainty.clone(),
            trust: diagnostic.trust.clone(),
            completion: diagnostic.completion.clone(),
            explanation: diagnostic.message.clone(),
        };
        let baseline_key = BaselineKey {
            fingerprint: baseline_entry.fingerprint.clone(),
            code: baseline_entry.code.clone(),
            logical_path: baseline_entry.logical_path.clone(),
            certainty: baseline_entry.certainty.clone(),
            trust: baseline_entry.trust.clone(),
            completion: baseline_entry.completion.clone(),
        };
        if is_eligible
            && disposition == "active"
            && baseline.as_ref().is_some_and(|(entries, compatible, _)| {
                *compatible && candidates.get(&fp) == Some(&1) && entries.contains(&baseline_key)
            })
        {
            disposition = "suppressed-baseline".into();
            reason = Some("matching finding in configured baseline".into());
            matched_entries.insert(baseline_key);
        }
        let base = if diagnostic.code == "TP3001" {
            "warning".to_string()
        } else {
            diagnostic.severity.clone()
        };
        let effective = config
            .severity
            .get(&diagnostic.code)
            .map(String::as_str)
            .unwrap_or(&base)
            .to_string();
        let threshold = if config.fail_on == "error" {
            effective == "error"
        } else {
            matches!(effective.as_str(), "error" | "warning")
        };
        let actionable = disposition == "active"
            && ((threshold && diagnostic.certainty != "unknown")
                || (config.mode == "strict" && diagnostic.certainty == "possible")
                || (!is_eligible && diagnostic.severity == "error"));
        if actionable {
            actionable_count += 1;
            policy_failures.push(format!("{} {}", diagnostic.code, diagnostic.message));
        }
        let related_sites_omitted = diagnostic.related.len().saturating_sub(32);
        diagnostic.related.sort_by(|a, b| {
            (&a.file, a.line, a.column, a.end_line, a.end_column).cmp(&(
                &b.file,
                b.line,
                b.column,
                b.end_line,
                b.end_column,
            ))
        });
        if related_sites_omitted > 0 {
            diagnostic.related.truncate(32);
        }
        let mut explanation_budget = 8 * 1024;
        let message_omitted_bytes =
            bound_diagnostic_text(&mut diagnostic.message, explanation_budget);
        explanation_budget = explanation_budget.saturating_sub(diagnostic.message.len());
        let repair_omitted_bytes = diagnostic.repair.as_mut().map_or(0, |repair| {
            bound_diagnostic_text(repair, explanation_budget)
        });
        let explanation_id = format!("{}.R001", diagnostic.code);
        *counts.entry(diagnostic.code.clone()).or_insert(0) += 1;
        *counts_by_certainty
            .entry(diagnostic.certainty.clone())
            .or_insert(0) += 1;
        *counts_by_disposition
            .entry(disposition.clone())
            .or_insert(0) += 1;
        raw.push(AdoptionDiagnostic {
            diagnostic,
            fingerprint: fp,
            base_severity: base,
            effective_severity: effective,
            disposition,
            disposition_reason: reason,
            explanation_id,
            actionable,
            message_omitted_bytes,
            repair_omitted_bytes,
            related_sites_omitted,
        });
    }
    if config.mode == "strict" && result.coverage.status != "complete-for-supported-subset" {
        policy_failures.push("analysis coverage is incomplete".into());
    }

    let baseline_summary =
        baseline
            .as_ref()
            .map_or_else(BaselineSummary::default, |(entries, compatible, reason)| {
                let unmatched = entries.len().saturating_sub(matched_entries.len());
                if !compatible {
                    policy_warnings
                        .push("baseline is stale or incompatible; entries were not applied".into());
                } else if unmatched > 0 {
                    policy_warnings.push(format!(
                        "{unmatched} baseline entries no longer match current findings"
                    ));
                }
                BaselineSummary {
                    configured: true,
                    compatible: Some(*compatible),
                    entries: entries.len(),
                    matched: matched_entries.len(),
                    unmatched,
                    update_reason: Some(reason.clone()),
                }
            });

    raw.sort_by(|a, b| {
        let location_order = (
            &a.diagnostic.primary.file,
            a.diagnostic.primary.line,
            a.diagnostic.primary.column,
            a.diagnostic.primary.end_line,
            a.diagnostic.primary.end_column,
            &a.diagnostic.code,
            &a.diagnostic.completion,
            &a.fingerprint,
        )
            .cmp(&(
                &b.diagnostic.primary.file,
                b.diagnostic.primary.line,
                b.diagnostic.primary.column,
                b.diagnostic.primary.end_line,
                b.diagnostic.primary.end_column,
                &b.diagnostic.code,
                &b.diagnostic.completion,
                &b.fingerprint,
            ));
        if max.is_some() {
            let rank = |item: &AdoptionDiagnostic| {
                if item.actionable {
                    0
                } else {
                    match item.disposition.as_str() {
                        "active" => 1,
                        "rule-disabled" => 2,
                        _ => 3,
                    }
                }
            };
            rank(a).cmp(&rank(b)).then(location_order)
        } else {
            location_order
        }
    });
    let max = max.unwrap_or(usize::MAX);
    if max == 0 {
        return Err("--max-diagnostics must be positive".into());
    }
    let total_diagnostics = raw.len();
    let truncated = total_diagnostics > max;
    let diagnostics = raw.into_iter().take(max).collect::<Vec<_>>();
    let details_truncated = truncated
        || diagnostics.iter().any(|diagnostic| {
            diagnostic.related_sites_omitted > 0
                || diagnostic.message_omitted_bytes > 0
                || diagnostic.repair_omitted_bytes > 0
        });
    let disabled_rules = ["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"]
        .into_iter()
        .filter(|rule| !config.enabled_rules.contains(*rule))
        .map(str::to_owned)
        .collect();
    let exit_code = if policy_failures.is_empty() { 0 } else { 1 };
    let project_root = config
        .root
        .canonicalize()
        .unwrap_or_else(|_| config.root.clone());
    let report = AdoptionReport {
        execution_status: "completed".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        report_schema_version: ADOPTION_SCHEMA_VERSION,
        check_schema_version: ADOPTION_SCHEMA_VERSION,
        target_python_version: result.target_python_version,
        model_catalog_version: result.model_catalog_version,
        available_models: result.available_models,
        assurance: result.assurance,
        contracts: result.contracts,
        coverage: result.coverage,
        diagnostics,
        counts,
        counts_by_certainty,
        counts_by_disposition,
        total_diagnostics,
        actionable_diagnostics: actionable_count,
        returned_diagnostics: max.min(total_diagnostics),
        audit_diagnostics: total_diagnostics.saturating_sub(actionable_count),
        policy_failures,
        policy_warnings,
        unused_suppressions,
        details_truncated,
        snapshot_algorithm_version: 1,
        snapshot_id,
        semantic_fingerprint,
        policy_fingerprint,
        model_fingerprint,
        snapshot_manifest: snapshot_manifest.clone(),
        project_root: project_root.to_string_lossy().into_owned(),
        logical_root: ".".into(),
        config_file: config
            .source
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        selected_files: snapshot_manifest
            .iter()
            .map(|entry| entry.logical_path.clone())
            .collect(),
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        selected_rules: config.enabled_rules.iter().cloned().collect(),
        disabled_rules,
        mode: config.mode.clone(),
        fail_on: config.fail_on.clone(),
        settings_origins: config.settings_origins.clone(),
        baseline: baseline_summary,
        exit_code,
    };
    Ok(report)
}

pub fn write_baseline(
    path: &Path,
    result: &CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    replace: bool,
    reason: &str,
) -> Result<BaselineUpdateSummary, String> {
    if reason.trim().is_empty() {
        return Err("baseline update requires a nonempty reason".into());
    }
    if reason.len() > 8 * 1024 {
        return Err("baseline update reason exceeds the 8 KiB limit".into());
    }
    for source in files {
        for (line, comment) in source_comments(&source.source) {
            if comment
                .strip_prefix('#')
                .is_some_and(|text| text.trim_start().starts_with("codepact:"))
            {
                if let Err(message) = parse_suppression(comment) {
                    return Err(format!(
                        "invalid suppression at {}:{line}: {message}",
                        source.path
                    ));
                }
            }
        }
    }
    reject_symlink_components(path, "baseline destination")?;
    if path.exists() && !replace {
        return Err("baseline destination exists; pass --replace to replace it".into());
    }
    if path.exists() && !path.is_file() {
        return Err("baseline destination must be a regular file".into());
    }
    let mut previous_entries = BTreeSet::new();
    if path.exists() {
        let (previous, _) = load_baseline_file(path, "existing baseline")?;
        previous_entries.extend(previous.entries);
    }
    let content: BTreeMap<&str, &str> = files
        .iter()
        .map(|f| (f.path.as_str(), f.source.as_str()))
        .collect();
    let mut candidate_counts = BTreeMap::<String, usize>::new();
    for diagnostic in &result.diagnostics {
        if !eligible(diagnostic) || !config.enabled_rules.contains(&diagnostic.code) {
            continue;
        }
        *candidate_counts
            .entry(fingerprint(diagnostic, &content))
            .or_default() += 1;
    }
    let mut entries = BTreeSet::new();
    let mut skipped_ambiguous = 0;
    for d in &result.diagnostics {
        if !eligible(d) || !config.enabled_rules.contains(&d.code) {
            continue;
        }
        let source = content.get(d.primary.file.as_str()).copied().unwrap_or("");
        if suppression_at(source, d.primary.line, &d.code)?.is_some() {
            continue;
        }
        let finding_fingerprint = fingerprint(d, &content);
        if candidate_counts.get(&finding_fingerprint) != Some(&1) {
            skipped_ambiguous += 1;
            continue;
        }
        entries.insert(BaselineEntry {
            fingerprint: finding_fingerprint,
            code: d.code.clone(),
            logical_path: d.primary.file.clone(),
            certainty: d.certainty.clone(),
            trust: d.trust.clone(),
            completion: d.completion.clone(),
            explanation: d.message.clone(),
        });
    }
    if entries.len() > 100_000 {
        return Err("generated baseline exceeds the 100,000 entry limit".into());
    }
    let mut source_manifest = files
        .iter()
        .map(|file| SnapshotFile {
            logical_path: file.path.clone(),
            sha256: hash(file.source.as_bytes()),
        })
        .collect::<Vec<_>>();
    source_manifest.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
    let snapshot_id = hash(
        serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            result.target_python_version.as_str(),
            result.model_catalog_version,
            &source_manifest,
            &config.enabled_rules,
            &config.severity,
            config.mode.as_str(),
            config.fail_on.as_str(),
        ))
        .map_err(|error| format!("cannot encode baseline snapshot: {error}"))?
        .as_slice(),
    );
    let b = Baseline {
        fingerprint_algorithm_version: FINGERPRINT_ALGORITHM_VERSION,
        baseline_schema_version: BASELINE_SCHEMA_VERSION,
        engine_version: env!("CARGO_PKG_VERSION").into(),
        rule_semantic_version: RULE_SEMANTIC_VERSION,
        target_python_version: result.target_python_version.clone(),
        model_catalog_version: result.model_catalog_version,
        snapshot_id: snapshot_id.clone(),
        source_manifest: source_manifest.clone(),
        logical_root: ".".into(),
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        selected_rules: config.enabled_rules.iter().cloned().collect(),
        coverage_status: result.coverage.status.clone(),
        coverage_reasons: result.coverage.reasons.clone(),
        update_reason: reason.trim().to_string(),
        entries: entries.into_iter().collect(),
    };
    let data = serialize_pretty_limited(&b, 16 * 1024 * 1024, "generated baseline")?;
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("cannot create baseline directory: {e}"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("cannot create baseline temporary identity: {error}"))?
        .as_nanos();
    let temp = parent.join(format!(
        ".codepact-baseline-{}-{nonce}.tmp",
        std::process::id()
    ));
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("cannot create temporary baseline: {error}"))?;
        file.write_all(&data)
            .map_err(|error| format!("cannot write temporary baseline: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush temporary baseline: {error}"))?;
        if replace {
            fs::rename(&temp, path)
                .map_err(|error| format!("cannot atomically replace baseline: {error}"))?;
        } else {
            fs::hard_link(&temp, path).map_err(|error| {
                format!("cannot install baseline without replacing an existing file: {error}")
            })?;
            fs::remove_file(&temp)
                .map_err(|error| format!("cannot remove baseline temporary file: {error}"))?;
        }
        Ok::<(), String>(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result?;
    let unchanged = entries_intersection_count(&previous_entries, &b.entries);
    Ok(BaselineUpdateSummary {
        baseline_schema_version: BASELINE_SCHEMA_VERSION,
        output_path: path.to_string_lossy().into_owned(),
        entries: b.entries.len(),
        added: b
            .entries
            .iter()
            .filter(|entry| !previous_entries.contains(*entry))
            .count(),
        removed: previous_entries
            .iter()
            .filter(|entry| !b.entries.contains(*entry))
            .count(),
        unchanged,
        skipped_ambiguous,
        snapshot_id,
        coverage_status: result.coverage.status.clone(),
    })
}

fn entries_intersection_count(old: &BTreeSet<BaselineEntry>, new: &[BaselineEntry]) -> usize {
    new.iter().filter(|entry| old.contains(*entry)).count()
}

pub fn failure(message: String) -> serde_json::Value {
    serde_json::json!({
        "execution_status":"failed",
        "engine_version":env!("CARGO_PKG_VERSION"),
        "report_schema_version":ADOPTION_SCHEMA_VERSION,
        "check_schema_version":ADOPTION_SCHEMA_VERSION,
        "error":{"category":"configuration-or-execution","message":message}
    })
}

pub fn suppression_diagnostic(
    path: &str,
    line: usize,
    column: usize,
    message: String,
) -> CheckDiagnostic {
    CheckDiagnostic {
        code: "TP4002".into(),
        severity: "error".into(),
        message,
        primary: Span {
            file: path.into(),
            line,
            column,
            end_line: line,
            end_column: column + 1,
        },
        related: vec![],
        certainty: "definite".into(),
        completion: None,
        trust: "static-source".into(),
        coverage_reason: None,
        repair: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{glob_match, read_configuration, serialize_pretty_limited, MAX_CONFIG_BYTES};

    #[test]
    fn limited_json_serialization_accepts_exact_limit_and_rejects_overflow() {
        let exact = serialize_pretty_limited(&"x", 3, "test report").unwrap();
        assert_eq!(exact, br#""x""#);

        let error = serialize_pretty_limited(&"x", 2, "test report").unwrap_err();
        assert_eq!(error, "test report exceeds the 2 byte limit");
    }

    #[test]
    fn limited_json_serialization_reports_serializer_errors_separately() {
        struct FailsSerialization;

        impl serde::Serialize for FailsSerialization {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("injected serializer failure"))
            }
        }

        let error = serialize_pretty_limited(&FailsSerialization, 64, "test report").unwrap_err();
        assert!(
            error.starts_with("cannot serialize test report:"),
            "{error}"
        );
        assert!(error.contains("injected serializer failure"), "{error}");
    }

    #[test]
    fn configuration_read_accepts_the_byte_limit_and_caps_consumption_on_overflow() {
        let exact = vec![b' '; MAX_CONFIG_BYTES as usize];
        assert_eq!(
            read_configuration(exact.as_slice()).unwrap().len(),
            exact.len()
        );

        let mut oversized = std::io::Cursor::new(vec![b' '; 2 * MAX_CONFIG_BYTES as usize]);
        let error = read_configuration(&mut oversized).unwrap_err();
        assert!(error.contains("64 KiB limit"));
        assert_eq!(oversized.position(), MAX_CONFIG_BYTES + 1);
    }

    #[test]
    fn globstar_matching_handles_many_ambiguous_components() {
        let mut pattern = std::iter::repeat_n("**", 20).collect::<Vec<_>>().join("/");
        pattern.push_str("/missing.py");
        let path = (0..20)
            .map(|index| format!("d{index}"))
            .chain(std::iter::once("real.py".into()))
            .collect::<Vec<_>>()
            .join("/");

        assert!(!glob_match(&pattern, &path));
        assert!(glob_match("**/*.py", "src/main.py"));
        assert!(glob_match("**/*.py", "main.py"));
    }
}

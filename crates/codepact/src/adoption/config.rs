use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DEFAULT_RULES: &[&str] = &["TP1001", "TP1002", "TP1003", "TP2001", "TP3001"];
pub const OWNERSHIP_RULES: &[&str] = &["TP2002", "TP2003", "TP2004", "TP2005", "TP2006"];

pub(super) const MAX_CONFIG_BYTES: u64 = 64 * 1024;

pub(super) fn read_configuration(reader: impl Read) -> Result<String, String> {
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
    #[serde(rename = "experimental-ownership")]
    pub experimental_ownership: Option<bool>,
    #[serde(rename = "assume-default-task-factory")]
    pub assume_default_task_factory: Option<bool>,
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
    pub ownership: crate::Ownership,
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
    pub experimental_ownership: Option<bool>,
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
    let experimental_ownership = overrides
        .experimental_ownership
        .or_else(|| c.as_ref().and_then(|c| c.experimental_ownership))
        .unwrap_or(false);
    let assume_default_task_factory = c
        .as_ref()
        .and_then(|c| c.assume_default_task_factory)
        .unwrap_or(false);
    let ownership = match (experimental_ownership, assume_default_task_factory) {
        (true, true) => crate::Ownership::AssumeDefaultTaskFactory,
        (true, false) => crate::Ownership::Enabled,
        (false, _) => crate::Ownership::Disabled,
    };
    let mut rules: BTreeSet<String> = c
        .as_ref()
        .and_then(|c| c.enabled_rules.clone())
        .unwrap_or_else(|| scoped_rules(ownership).map(str::to_owned).collect())
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
        if !ownership.is_enabled() && OWNERSHIP_RULES.contains(&r.as_str()) {
            return Err(format!("rule {r} requires experimental-ownership"));
        }
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
    if overrides.experimental_ownership.is_some()
        || c.as_ref()
            .is_some_and(|c| c.experimental_ownership.is_some())
    {
        settings_origins.insert(
            "experimental-ownership".into(),
            origin(
                overrides.experimental_ownership.is_some(),
                c.as_ref()
                    .is_some_and(|c| c.experimental_ownership.is_some()),
            )
            .into(),
        );
    }
    if c.as_ref()
        .is_some_and(|c| c.assume_default_task_factory.is_some())
    {
        settings_origins.insert(
            "assume-default-task-factory".into(),
            "pyproject.toml".into(),
        );
    }
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
        ownership,
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
    eligible_code(rule)
}

pub(super) fn eligible_code(code: &str) -> bool {
    DEFAULT_RULES.contains(&code) || OWNERSHIP_RULES.contains(&code)
}

pub(super) fn validate_rule(r: &str) -> Result<(), String> {
    if eligible_code(r) || matches!(r, "TP4001" | "TP4002" | "TP9001" | "TP9002") {
        Ok(())
    } else {
        Err(format!("unknown rule identifier {r}"))
    }
}

pub(super) fn scoped_rules(ownership: crate::Ownership) -> impl Iterator<Item = &'static str> {
    DEFAULT_RULES.iter().copied().chain(
        OWNERSHIP_RULES
            .iter()
            .copied()
            .filter(move |_| ownership.is_enabled()),
    )
}
pub(super) fn validate_pattern(p: &str) -> Result<(), String> {
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

pub(super) fn glob_match(pattern: &str, path: &str) -> bool {
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

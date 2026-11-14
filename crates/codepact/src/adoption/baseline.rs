use super::*;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Baseline {
    pub(super) fingerprint_algorithm_version: u8,
    pub(super) baseline_schema_version: u8,
    pub(super) engine_version: String,
    pub(super) rule_semantic_version: u8,
    pub(super) target_python_version: String,
    pub(super) model_catalog_version: u8,
    #[serde(default, skip_serializing_if = "ownership_disabled")]
    pub(super) ownership: crate::Ownership,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) ownership_model_revision: Option<String>,
    pub(super) snapshot_id: String,
    pub(super) source_manifest: Vec<SnapshotFile>,
    pub(super) logical_root: String,
    pub(super) include: Vec<String>,
    pub(super) exclude: Vec<String>,
    pub(super) selected_rules: Vec<String>,
    pub(super) coverage_status: String,
    pub(super) coverage_reasons: Vec<String>,
    pub(super) update_reason: String,
    pub(super) entries: Vec<BaselineEntry>,
}

fn ownership_disabled(ownership: &crate::Ownership) -> bool {
    !ownership.is_enabled()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(super) struct BaselineEntry {
    pub(super) fingerprint: String,
    pub(super) code: String,
    pub(super) logical_path: String,
    pub(super) certainty: String,
    pub(super) trust: String,
    pub(super) completion: Option<String>,
    pub(super) explanation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BaselineKey {
    pub(super) fingerprint: String,
    pub(super) code: String,
    pub(super) logical_path: String,
    pub(super) certainty: String,
    pub(super) trust: String,
    pub(super) completion: Option<String>,
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

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_logical_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

pub(super) fn validate_baseline_document(baseline: &Baseline) -> Result<(), String> {
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

pub(super) const MAX_BASELINE_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn load_baseline_file(
    path: &Path,
    description: &str,
) -> Result<(Baseline, String), String> {
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

pub fn write_baseline(
    path: &Path,
    result: &CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    replace: bool,
    reason: &str,
) -> Result<BaselineUpdateSummary, String> {
    validate_analysis_scope(result, config)?;
    if reason.trim().is_empty() {
        return Err("baseline update requires a nonempty reason".into());
    }
    if reason.len() > 8 * 1024 {
        return Err("baseline update reason exceeds the 8 KiB limit".into());
    }
    let mut suppressions_by_path = BTreeMap::new();
    for source in files {
        let mut suppressions_by_line = BTreeMap::new();
        let mut previous_comment_line = None;
        for (line, comment) in source_comments(&source.source) {
            // Baseline matching uses the first comment for each computed line.
            let first_on_line = previous_comment_line.replace(line) != Some(line);
            if !comment
                .strip_prefix('#')
                .is_some_and(|text| text.trim_start().starts_with("codepact:"))
            {
                continue;
            }
            let suppression = parse_suppression(comment).map_err(|message| {
                format!("invalid suppression at {}:{line}: {message}", source.path)
            })?;
            if let Some((codes, _)) = suppression.filter(|_| first_on_line) {
                suppressions_by_line.insert(line, codes);
            }
        }
        suppressions_by_path.insert(source.path.as_str(), suppressions_by_line);
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
    let mut anchors = AnchorCache::default();
    let mut candidate_counts = BTreeMap::<String, usize>::new();
    for diagnostic in &result.diagnostics {
        if !eligible(diagnostic) || !config.enabled_rules.contains(&diagnostic.code) {
            continue;
        }
        *candidate_counts
            .entry(fingerprint(diagnostic, &content, &mut anchors))
            .or_default() += 1;
    }
    let mut entries = BTreeSet::new();
    let mut skipped_ambiguous = 0;
    for d in &result.diagnostics {
        if !eligible(d) || !config.enabled_rules.contains(&d.code) {
            continue;
        }
        if suppressions_by_path
            .get(d.primary.file.as_str())
            .and_then(|lines| lines.get(&d.primary.line))
            .is_some_and(|codes| codes.contains(&d.code))
        {
            continue;
        }
        if d.semantic_identity.is_empty() {
            return Err("cannot baseline a finding without resolved contract identity; rerun analysis before updating the baseline".into());
        }
        let finding_fingerprint = fingerprint(d, &content, &mut anchors);
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
    let mut module_manifest = files
        .iter()
        .map(|file| (file.path.as_str(), file.module.as_str()))
        .collect::<Vec<_>>();
    module_manifest.sort();
    let snapshot_id = hash(
        serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            result.target_python_version.as_str(),
            result.model_catalog_version,
            config.ownership,
            ownership_model_revision(result),
            &source_manifest,
            &module_manifest,
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
        ownership: config.ownership,
        ownership_model_revision: ownership_model_revision(result).map(str::to_owned),
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
    install_baseline_atomically(path, &data, replace, |_| Ok(()))?;
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

pub(super) fn install_baseline_atomically(
    path: &Path,
    data: &[u8],
    replace: bool,
    before_commit: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
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
        file.write_all(data)
            .map_err(|error| format!("cannot write temporary baseline: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush temporary baseline: {error}"))?;
        before_commit(&temp)?;
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
    write_result
}

fn entries_intersection_count(old: &BTreeSet<BaselineEntry>, new: &[BaselineEntry]) -> usize {
    new.iter().filter(|entry| old.contains(*entry)).count()
}

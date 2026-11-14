use crate::{CheckDiagnostic, CheckResult, SourceFile, Span};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;

mod baseline;
mod config;
mod fingerprint;
#[cfg(test)]
use baseline::install_baseline_atomically;
use baseline::{load_baseline_file, BaselineEntry, BaselineKey};
pub use baseline::{validate_baseline_file, write_baseline};
use config::{eligible_code, scoped_rules, validate_pattern, validate_rule};
#[cfg(test)]
use config::{glob_match, read_configuration, MAX_CONFIG_BYTES};
pub use config::{
    included, reject_symlink_components, resolve_config, EffectiveConfig, Overrides, ProjectConfig,
    DEFAULT_RULES, OWNERSHIP_RULES,
};
use fingerprint::{code_before_comment, fingerprint, hash, source_comments, AnchorCache};

pub const ADOPTION_SCHEMA_VERSION: u8 = 2;
pub const BASELINE_SCHEMA_VERSION: u8 = 1;
pub const FINGERPRINT_ALGORITHM_VERSION: u8 = 5;
pub const RULE_SEMANTIC_VERSION: u8 = 1;
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
fn eligible(d: &CheckDiagnostic) -> bool {
    eligible_code(&d.code)
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
fn ownership_model_revision(result: &CheckResult) -> Option<&str> {
    result
        .available_models
        .iter()
        .find(|model| model.id == "TPM-TASK-001")
        .and_then(|model| model.source_revisions.first())
        .map(String::as_str)
}

fn validate_analysis_scope(result: &CheckResult, config: &EffectiveConfig) -> Result<(), String> {
    if result.target_python_version != config.python_version {
        return Err("report Python target differs from the analyzed target".into());
    }
    if result.diagnostics.iter().any(|d| d.code == "TP9002") {
        return Err(
            "analysis was cancelled; no completed report or baseline can be published".into(),
        );
    }
    if !matches!(
        result.assurance.as_str(),
        crate::ADOPTION_ASSURANCE | crate::LIFECYCLE_ASSURANCE | crate::OWNERSHIP_ASSURANCE
    ) {
        return Err("lifecycle analysis was not executed; declaration inspection alone cannot produce a check report".into());
    }
    if config.enabled_rules.contains("TP3001") && result.assurance == crate::LIFECYCLE_ASSURANCE {
        return Err("must-use rules were requested but not analyzed; use check_with_must_use before building an adoption report or baseline".into());
    }
    if (result.assurance == crate::OWNERSHIP_ASSURANCE) != config.ownership.is_enabled()
        || ownership_model_revision(result)
            != (config.ownership == crate::Ownership::AssumeDefaultTaskFactory)
                .then(|| crate::model::task_group_revision(&config.python_version))
                .flatten()
    {
        return Err("report ownership scope differs from the analyzed scope".into());
    }
    Ok(())
}

pub fn build_report(
    mut result: CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    baseline_path: Option<&Path>,
    max: Option<usize>,
) -> Result<AdoptionReport, String> {
    validate_analysis_scope(&result, config)?;
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
            && loaded.ownership == config.ownership
            && loaded.ownership_model_revision.as_deref() == ownership_model_revision(&result)
            && loaded.engine_version == env!("CARGO_PKG_VERSION");
        Some((unique, compatible, loaded.update_reason))
    } else {
        None
    };

    let mut contents = BTreeMap::new();
    for file in files {
        contents.insert(file.path.as_str(), file.source.as_str());
    }
    let mut files_by_path = files.iter().collect::<Vec<_>>();
    files_by_path.sort_by(|a, b| (&a.path, &a.module).cmp(&(&b.path, &b.module)));
    let snapshot_manifest = files_by_path
        .iter()
        .map(|file| SnapshotFile {
            logical_path: file.path.clone(),
            sha256: hash(file.source.as_bytes()),
        })
        .collect::<Vec<_>>();
    let module_manifest = files_by_path
        .iter()
        .map(|file| (&file.path, &file.module))
        .collect::<Vec<_>>();
    let semantic_fingerprint = hash(
        serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            result.check_schema_version,
            RULE_SEMANTIC_VERSION,
            FINGERPRINT_ALGORITHM_VERSION,
            &result.assurance,
            config.python_version.as_str(),
            result.model_catalog_version,
            &snapshot_manifest,
            &module_manifest,
        ))
        .map_err(|error| format!("cannot encode semantic fingerprint: {error}"))?
        .as_slice(),
    );
    let model_fingerprint = hash(
        serde_json::to_vec(&(result.model_catalog_version, &result.available_models))
            .map_err(|error| format!("cannot encode model fingerprint: {error}"))?
            .as_slice(),
    );
    let suppression_policy = files_by_path
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

    let mut anchors = AnchorCache::default();
    let mut candidates = BTreeMap::<String, usize>::new();
    for diagnostic in &result.diagnostics {
        *candidates
            .entry(fingerprint(diagnostic, &contents, &mut anchors))
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
        let fp = fingerprint(&diagnostic, &contents, &mut anchors);
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
            && !diagnostic.semantic_identity.is_empty()
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
    let disabled_rules = scoped_rules(config.ownership)
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
        semantic_identity: Vec::new(),
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
    use super::{
        build_report, glob_match, read_configuration, resolve_config, serialize_pretty_limited,
        Overrides, MAX_CONFIG_BYTES,
    };

    #[test]
    fn semantic_snapshot_includes_module_identity_and_rejects_target_mismatch() {
        use crate::{check_with_ownership, AnalysisSession, SourceFile};

        let config = resolve_config(
            &std::env::current_dir().unwrap(),
            &Overrides {
                no_config: true,
                ..Default::default()
            },
        )
        .unwrap();
        let files = [SourceFile::new(
            "provider.py",
            "provider",
            "def f(): pass\n",
        )];
        let result = check_with_ownership(&files, &config.python_version, config.ownership);
        let report = build_report(result, &files, &config, None, None).unwrap();

        let renamed = [SourceFile::new("provider.py", "renamed", "def f(): pass\n")];
        let result = check_with_ownership(&renamed, &config.python_version, config.ownership);
        let renamed_report = build_report(result, &renamed, &config, None, None).unwrap();
        assert_ne!(report.snapshot_id, renamed_report.snapshot_id);

        let result =
            AnalysisSession::default().check_with_ownership(&files, "3.12", config.ownership);
        assert_eq!(
            build_report(result, &files, &config, None, None).unwrap_err(),
            "report Python target differs from the analyzed target"
        );
    }

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
    #[test]
    fn interrupted_baseline_commit_preserves_existing_bytes_and_cleans_temporary() {
        let root = std::env::temp_dir().join(format!(
            "codepact-atomic-interruption-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("baseline.json");
        std::fs::write(&destination, b"previous complete baseline").unwrap();
        let error = super::install_baseline_atomically(
            &destination,
            b"next complete baseline",
            true,
            |temporary| {
                assert_eq!(std::fs::read(temporary).unwrap(), b"next complete baseline");
                assert_eq!(
                    std::fs::read(&destination).unwrap(),
                    b"previous complete baseline"
                );
                Err("injected interruption before commit".into())
            },
        )
        .unwrap_err();
        assert!(error.contains("interruption"));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"previous complete baseline"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        super::install_baseline_atomically(&destination, b"next complete baseline", true, |_| {
            Ok(())
        })
        .unwrap();
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"next complete baseline"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

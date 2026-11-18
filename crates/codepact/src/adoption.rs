use crate::{CheckDiagnostic, CheckResult, SourceFile, Span};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

mod baseline;
mod config;
mod fingerprint;
mod policy;
mod render;
mod scope;
mod snapshot;
#[cfg(test)]
use baseline::install_baseline_atomically;
use baseline::load_baseline_file;
pub use baseline::{validate_baseline_file, write_baseline};
use config::scoped_rules;
#[cfg(test)]
use config::{glob_match, read_configuration, MAX_CONFIG_BYTES};
pub use config::{
    included, reject_symlink_components, resolve_config, DependenciesConfig, EffectiveConfig,
    Overrides, ProjectConfig, ADVANCED_RULES, DEFAULT_RULES, FINITE_MATCH_RULES, OWNERSHIP_RULES,
};
#[cfg(test)]
use render::serialize_pretty_limited;
pub use render::serialize_report;
use scope::validate_analysis_scope;

pub const ADOPTION_SCHEMA_VERSION: u8 = 3;
pub const FOUNDATION_ADOPTION_SCHEMA_VERSION: u8 = 4;
pub const BASELINE_SCHEMA_VERSION: u8 = 1;
pub const FINGERPRINT_ALGORITHM_VERSION: u8 = 5;
pub const RULE_SEMANTIC_VERSION: u8 = 4;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foundations: Option<crate::foundation::FoundationEvidence>,
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
    pub dependency_roots: Vec<String>,
    pub required_providers: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub selected_rules: Vec<String>,
    pub disabled_rules: Vec<String>,
    pub mode: String,
    pub fail_on: String,
    pub experimental_finite_matches: bool,
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
pub fn build_report(
    result: CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    baseline_path: Option<&Path>,
    max: Option<usize>,
) -> Result<AdoptionReport, String> {
    validate_analysis_scope(&result, config)?;
    let (baseline, baseline_content_hash) = if let Some(path) = baseline_path {
        let (loaded, content_hash) = load_baseline_file(path, "baseline")?;
        (
            Some(policy::BaselinePolicy::new(loaded, &result, config)),
            content_hash,
        )
    } else {
        (None, String::new())
    };
    let snapshot = snapshot::identify(&result, files, config, &baseline_content_hash)?;
    let selected_files = snapshot
        .manifest
        .iter()
        .map(|entry| entry.logical_path.clone())
        .collect();
    let policy = policy::apply(
        result.diagnostics,
        &result.coverage,
        files,
        config,
        baseline.as_ref(),
    );
    let details = render::present_details(policy.diagnostics, max)?;
    let disabled_rules = scoped_rules(config.ownership, config.finite_matches)
        .filter(|rule| !config.enabled_rules.contains(*rule))
        .map(str::to_owned)
        .collect();
    let exit_code = u8::from(!policy.failures.is_empty());
    let project_root = config
        .root
        .canonicalize()
        .unwrap_or_else(|_| config.root.clone());
    let has_foundations = result.foundations.is_some();
    let report = AdoptionReport {
        execution_status: "completed".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        report_schema_version: if has_foundations {
            FOUNDATION_ADOPTION_SCHEMA_VERSION
        } else {
            ADOPTION_SCHEMA_VERSION
        },
        check_schema_version: result.check_schema_version,
        target_python_version: result.target_python_version,
        model_catalog_version: result.model_catalog_version,
        available_models: result.available_models,
        foundations: result.foundations,
        assurance: result.assurance,
        contracts: result.contracts,
        coverage: result.coverage,
        returned_diagnostics: details.diagnostics.len(),
        diagnostics: details.diagnostics,
        counts: policy.counts,
        counts_by_certainty: policy.counts_by_certainty,
        counts_by_disposition: policy.counts_by_disposition,
        total_diagnostics: details.total_diagnostics,
        actionable_diagnostics: policy.actionable_count,
        audit_diagnostics: details
            .total_diagnostics
            .saturating_sub(policy.actionable_count),
        policy_failures: policy.failures,
        policy_warnings: policy.warnings,
        unused_suppressions: policy.unused_suppressions,
        details_truncated: details.truncated,
        snapshot_algorithm_version: if has_foundations { 2 } else { 1 },
        snapshot_id: snapshot.id,
        semantic_fingerprint: snapshot.semantic,
        policy_fingerprint: snapshot.policy,
        model_fingerprint: snapshot.model,
        snapshot_manifest: snapshot.manifest,
        project_root: project_root.to_string_lossy().into_owned(),
        logical_root: ".".into(),
        config_file: config
            .source
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        selected_files,
        dependency_roots: config
            .dependency_roots
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        required_providers: config.required_providers.clone(),
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        selected_rules: config.enabled_rules.iter().cloned().collect(),
        disabled_rules,
        mode: config.mode.clone(),
        fail_on: config.fail_on.clone(),
        experimental_finite_matches: config.finite_matches,
        settings_origins: config.settings_origins.clone(),
        baseline: policy.baseline,
        exit_code,
    };
    Ok(report)
}

pub fn failure(message: String) -> serde_json::Value {
    serde_json::json!({
        "execution_status":"failed",
        "engine_version":env!("CARGO_PKG_VERSION"),
        "report_schema_version":ADOPTION_SCHEMA_VERSION,
        "check_schema_version":crate::CHECK_SCHEMA_VERSION,
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
        provider_evidence: None,
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

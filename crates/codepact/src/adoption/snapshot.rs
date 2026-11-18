use super::{
    fingerprint::{hash, source_comments},
    EffectiveConfig, SnapshotFile, FINGERPRINT_ALGORITHM_VERSION, RULE_SEMANTIC_VERSION,
};
use crate::{CheckResult, SourceFile};

pub(super) struct SnapshotIdentity {
    pub(super) id: String,
    pub(super) semantic: String,
    pub(super) policy: String,
    pub(super) model: String,
    pub(super) manifest: Vec<SnapshotFile>,
}

pub(super) fn identify(
    result: &CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
    baseline_content_hash: &str,
) -> Result<SnapshotIdentity, String> {
    let mut files_by_path = files.iter().collect::<Vec<_>>();
    files_by_path.sort_by(|a, b| (&a.path, &a.module).cmp(&(&b.path, &b.module)));
    let mut snapshot_manifest = files_by_path
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
    if let Some(evidence) = &result.foundations {
        snapshot_manifest = evidence
            .scope
            .iter()
            .map(|file| SnapshotFile {
                logical_path: file.path.clone(),
                sha256: file.sha256.clone(),
            })
            .collect();
    }
    let semantic_fingerprint = hash(
        serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            result.check_schema_version,
            RULE_SEMANTIC_VERSION,
            FINGERPRINT_ALGORITHM_VERSION,
            &result.assurance,
            config.python_version.as_str(),
            result.model_catalog_version,
            config.finite_matches,
            &snapshot_manifest,
            &module_manifest,
        ))
        .map_err(|error| format!("cannot encode semantic fingerprint: {error}"))?
        .as_slice(),
    );
    let model_fingerprint = hash(
        if let Some(evidence) = &result.foundations {
            serde_json::to_vec(&(
                result.model_catalog_version,
                &result.available_models,
                &evidence.comparison,
                &evidence.runtime,
                &evidence.providers,
                &evidence.provider_issues,
                &evidence.lock_sha256,
                &evidence.project_models_sha256,
            ))
        } else {
            serde_json::to_vec(&(result.model_catalog_version, &result.available_models))
        }
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
            config.finite_matches,
            &config.include,
            &config.exclude,
            &config.required_providers,
            baseline_content_hash,
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

    Ok(SnapshotIdentity {
        id: snapshot_id,
        semantic: semantic_fingerprint,
        policy: policy_fingerprint,
        model: model_fingerprint,
        manifest: snapshot_manifest,
    })
}

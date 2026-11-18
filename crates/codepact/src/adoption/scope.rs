use super::EffectiveConfig;
use crate::CheckResult;

pub(super) fn ownership_model_revision(result: &CheckResult) -> Option<String> {
    let mut selected = result
        .available_models
        .iter()
        .filter(|model| matches!(model.id.as_str(), "TPM-TASK-001" | "TPM-EXECUTOR-001"))
        .filter_map(|model| {
            model
                .source_revisions
                .first()
                .map(|revision| (model.id.as_str(), revision.as_str()))
        })
        .collect::<Vec<_>>();
    format_selected_models(&mut selected)
}

fn expected_ownership_model_revision(config: &EffectiveConfig) -> Option<String> {
    let mut selected = Vec::new();
    if config.ownership.assumes_default_task_factory() {
        if let Some(revision) = crate::model::task_group_revision(&config.python_version) {
            selected.push(("TPM-TASK-001", revision));
        }
    }
    if config.ownership.includes_advanced_contracts() {
        if let Some(revision) = crate::model::thread_pool_executor_revision(&config.python_version)
        {
            selected.push(("TPM-EXECUTOR-001", revision));
        }
    }
    format_selected_models(&mut selected)
}

pub(super) fn validate_analysis_scope(
    result: &CheckResult,
    config: &EffectiveConfig,
) -> Result<(), String> {
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
        crate::ADOPTION_ASSURANCE
            | crate::LIFECYCLE_ASSURANCE
            | crate::OWNERSHIP_ASSURANCE
            | crate::ADVANCED_ASSURANCE
    ) {
        return Err("lifecycle analysis was not executed; declaration inspection alone cannot produce a check report".into());
    }
    if config.enabled_rules.contains("TP3001") && result.assurance == crate::LIFECYCLE_ASSURANCE {
        return Err("must-use rules were requested but not analyzed; use check_with_must_use before building an adoption report or baseline".into());
    }
    let ownership_enabled = matches!(
        result.assurance.as_str(),
        crate::OWNERSHIP_ASSURANCE | crate::ADVANCED_ASSURANCE
    );
    if ownership_enabled != config.ownership.is_enabled()
        || (result.assurance == crate::ADVANCED_ASSURANCE)
            != config.ownership.includes_advanced_contracts()
        || ownership_model_revision(result) != expected_ownership_model_revision(config)
    {
        return Err("report ownership scope differs from the analyzed scope".into());
    }
    Ok(())
}

fn format_selected_models(selected: &mut [(&str, &str)]) -> Option<String> {
    selected.sort_by_key(|(id, _)| *id);
    match selected {
        [] => None,
        [("TPM-TASK-001", revision)] => Some((*revision).to_owned()),
        [(id, revision)] => Some(format!("{id}={revision}")),
        selected => Some(
            selected
                .iter()
                .map(|(id, revision)| format!("{id}={revision}"))
                .collect::<Vec<_>>()
                .join(";"),
        ),
    }
}

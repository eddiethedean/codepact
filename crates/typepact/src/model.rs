use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CHECK_SCHEMA_VERSION: u8 = 1;
pub const CHECK_MODEL_CATALOG_VERSION: u8 = 2;

/// Lifecycle postconditions used by TypePact-owned effect summaries. These
/// records are internal model inputs; they do not add user annotation syntax.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum LifecyclePostcondition {
    Preserve,
    Open,
    Closed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RepeatClosePolicy {
    Unspecified,
    Idempotent,
    RequiresOpen,
}

/// A reviewed or fixture summary for one resource cleanup method.
///
/// `None` for an outcome means that the model establishes no such exit. A
/// concrete postcondition is applied only to that outcome, so an exceptional
/// or cancelled state cannot inherit the normal-close transition by accident.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ResourceEffectModel {
    pub id: String,
    pub qualified_symbol: String,
    pub close_method: String,
    pub source_revision: String,
    pub source_url: String,
    pub supported_python_versions: BTreeSet<String>,
    pub platform_assumptions: Vec<String>,
    pub argument_aliases: Vec<ModelAlias>,
    pub result_alias: Option<String>,
    pub duty_handoff: String,
    pub suppression: String,
    pub asynchronous: bool,
    pub normal: Option<LifecyclePostcondition>,
    pub exceptional: Option<LifecyclePostcondition>,
    pub cancelled: Option<LifecyclePostcondition>,
    pub repeat_close: RepeatClosePolicy,
    pub post_close_methods: BTreeSet<String>,
    pub trust: String,
    pub reviewer: String,
    pub valid_cases: Vec<String>,
    pub invalid_cases: Vec<String>,
    pub unknown_cases: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ModelAlias {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FlowModelCatalog {
    resources: Vec<ResourceEffectModel>,
}

impl Default for FlowModelCatalog {
    fn default() -> Self {
        let mut catalog = Self {
            resources: Vec::new(),
        };
        for model in builtin_resource_effect_models() {
            catalog
                .insert(model)
                .expect("built-in resource effect models must not conflict");
        }
        catalog
    }
}

fn builtin_resource_effect_models() -> Vec<ResourceEffectModel> {
    // `contextlib.closing` and `aclosing` have manager-specific summaries in
    // the flow engine. Resource-specific close summaries are added here only
    // when their source and outcome matrix have been reviewed.
    Vec::new()
}

impl FlowModelCatalog {
    /// Add a version-bounded model, rejecting overlapping records for the same
    /// resource and method so selection never depends on insertion order.
    pub(crate) fn insert(&mut self, model: ResourceEffectModel) -> Result<(), String> {
        if model.id.trim().is_empty()
            || model.qualified_symbol.trim().is_empty()
            || model.close_method.trim().is_empty()
            || model.source_revision.trim().is_empty()
            || model.source_url.trim().is_empty()
            || model.trust.trim().is_empty()
            || model.reviewer.trim().is_empty()
            || model.duty_handoff.trim().is_empty()
            || model.suppression.trim().is_empty()
            || !model.argument_aliases.is_empty()
            || model.result_alias.is_some()
            || model.duty_handoff != "none"
            || model.suppression != "not-a-context-manager"
            || model.supported_python_versions.is_empty()
            || model
                .supported_python_versions
                .iter()
                .any(|version| version.trim().is_empty())
            || model.platform_assumptions.is_empty()
            || model
                .platform_assumptions
                .iter()
                .any(|assumption| assumption.trim().is_empty())
            || model.valid_cases.is_empty()
            || model.valid_cases.iter().any(|case| case.trim().is_empty())
            || model.invalid_cases.is_empty()
            || model
                .invalid_cases
                .iter()
                .any(|case| case.trim().is_empty())
            || model.unknown_cases.is_empty()
            || model
                .unknown_cases
                .iter()
                .any(|case| case.trim().is_empty())
            || model
                .post_close_methods
                .iter()
                .any(|method| method.trim().is_empty())
            || model
                .argument_aliases
                .iter()
                .any(|alias| alias.source.trim().is_empty() || alias.target.trim().is_empty())
            || model
                .result_alias
                .as_ref()
                .is_some_and(|alias| alias.trim().is_empty())
            || (model.asynchronous && model.cancelled.is_none())
        {
            return Err("resource effect models require identity, provenance, review, version bounds, conformance cases and an explicit async cancellation outcome".into());
        }
        if self.resources.iter().any(|existing| {
            existing.qualified_symbol == model.qualified_symbol
                && existing.close_method == model.close_method
                && existing
                    .supported_python_versions
                    .iter()
                    .any(|version| model.supported_python_versions.contains(version))
        }) {
            return Err(format!(
                "model {} conflicts with an existing model for {}.{} on a supported Python version",
                model.id, model.qualified_symbol, model.close_method
            ));
        }
        self.resources.push(model);
        self.resources.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(())
    }

    pub(crate) fn resolve(
        &self,
        qualified_symbol: &str,
        close_method: &str,
        python_version: &str,
    ) -> Option<&ResourceEffectModel> {
        self.resources.iter().find(|model| {
            model.qualified_symbol == qualified_symbol
                && model.close_method == close_method
                && model.supported_python_versions.contains(python_version)
        })
    }

    pub(crate) fn for_resource_symbols(&self, symbols: &BTreeSet<String>) -> Self {
        Self {
            resources: self
                .resources
                .iter()
                .filter(|model| symbols.contains(&model.qualified_symbol))
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
mod flow_model_tests {
    use super::*;

    fn model(id: &str, versions: &[&str]) -> ResourceEffectModel {
        ResourceEffectModel {
            id: id.into(),
            qualified_symbol: "fixture.Connection".into(),
            close_method: "close".into(),
            source_revision: format!("{id}-source"),
            source_url: format!("fixture://{id}"),
            supported_python_versions: versions.iter().map(|version| (*version).into()).collect(),
            platform_assumptions: vec!["platform-independent fixture".into()],
            argument_aliases: Vec::new(),
            result_alias: None,
            duty_handoff: "none".into(),
            suppression: "not-a-context-manager".into(),
            asynchronous: false,
            normal: Some(LifecyclePostcondition::Closed),
            exceptional: None,
            cancelled: None,
            repeat_close: RepeatClosePolicy::Unspecified,
            post_close_methods: BTreeSet::new(),
            trust: "reviewed-fixture".into(),
            reviewer: "fixture-test".into(),
            valid_cases: vec!["normal close applies its postcondition".into()],
            invalid_cases: vec!["overlapping version bounds are rejected".into()],
            unknown_cases: vec!["unsupported Python version selects no model".into()],
        }
    }

    #[test]
    fn catalog_selection_is_version_exact_and_rejects_conflicts() {
        let mut catalog = FlowModelCatalog::default();
        catalog.insert(model("fixture-311", &["3.11"])).unwrap();
        catalog.insert(model("fixture-312", &["3.12"])).unwrap();
        assert_eq!(
            catalog
                .resolve("fixture.Connection", "close", "3.11")
                .map(|model| model.id.as_str()),
            Some("fixture-311")
        );
        assert_eq!(
            catalog
                .resolve("fixture.Connection", "close", "3.12")
                .map(|model| model.id.as_str()),
            Some("fixture-312")
        );
        assert!(catalog
            .resolve("fixture.Connection", "close", "3.13")
            .is_none());
        assert!(catalog.insert(model("conflict", &["3.12"])).is_err());

        let mut missing_async_outcome = model("async-close", &["3.11"]);
        missing_async_outcome.asynchronous = true;
        assert!(FlowModelCatalog::default()
            .insert(missing_async_outcome.clone())
            .is_err());
        missing_async_outcome.cancelled = Some(LifecyclePostcondition::Unknown);
        assert!(FlowModelCatalog::default()
            .insert(missing_async_outcome)
            .is_ok());

        let encoded = serde_json::to_string(&catalog).unwrap();
        let decoded: FlowModelCatalog = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, catalog);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Span {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub origin: String,
    pub trust: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contract {
    pub schema_version: u8,
    pub base_symbol: String,
    pub display_name: String,
    pub lifecycle: Option<Lifecycle>,
    pub access: String,
    pub source: Span,
    pub provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lifecycle {
    pub closed_by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub primary: Span,
    pub related: Vec<Span>,
    pub certainty: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Coverage {
    pub status: String,
    pub reasons: Vec<String>,
    pub source_bytes: usize,
    pub declarations: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inspection {
    pub schema_version: u8,
    pub assurance: String,
    pub contracts: Vec<Contract>,
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: Coverage,
}

/// Structured cleanup-check output. Its schema evolves independently of the
/// declaration-inspection envelope so check-specific fields do not change the
/// published inspection contract.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckResult {
    pub check_schema_version: u8,
    pub schema_version: u8,
    pub target_python_version: String,
    pub model_catalog_version: u8,
    pub available_models: Vec<CheckModelMetadata>,
    pub assurance: String,
    pub contracts: Vec<Contract>,
    pub diagnostics: Vec<CheckDiagnostic>,
    pub coverage: Coverage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckModelMetadata {
    pub id: String,
    pub qualified_symbol: String,
    pub source_revisions: Vec<String>,
    pub source_urls: Vec<String>,
    pub selection_rule: String,
    pub modeled_behavior: String,
    pub qualification: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub primary: Span,
    pub related: Vec<Span>,
    pub certainty: String,
    /// Modeled function completion associated with an exit obligation.
    pub completion: Option<String>,
    /// Trust basis for the source facts used by this finding.
    pub trust: String,
    /// Boundary reason affecting this finding, when one is known.
    pub coverage_reason: Option<String>,
    /// Suggested source-level action; this is guidance, not an auto-fix.
    pub repair: Option<String>,
}

impl CheckResult {
    pub fn from_inspection(inspection: Inspection, target_python_version: &str) -> Self {
        Self::from_inspection_with_models(
            inspection,
            target_python_version,
            &FlowModelCatalog::default(),
        )
    }

    pub(crate) fn from_inspection_with_models(
        inspection: Inspection,
        target_python_version: &str,
        flow_models: &FlowModelCatalog,
    ) -> Self {
        let diagnostics = inspection
            .diagnostics
            .into_iter()
            .map(|diagnostic| {
                let completion = if diagnostic.code == "TP1003" {
                    Some("return".into())
                } else {
                    [
                        "fallthrough",
                        "return",
                        "raise",
                        "cancelled",
                        "break",
                        "continue",
                    ]
                        .into_iter()
                        .find(|kind| {
                            diagnostic.message.ends_with(&format!(" on {kind}"))
                        })
                        .map(str::to_owned)
                };
                let coverage_reason = if diagnostic.code == "TP9001" {
                    Some(diagnostic.message.clone())
                } else {
                    inspection
                        .coverage
                        .reasons
                        .iter()
                        .find(|reason| {
                            reason.starts_with(&format!(
                                "{}:{}:",
                                diagnostic.primary.file, diagnostic.primary.line
                            ))
                        })
                        .cloned()
                };
                let trust = match diagnostic.code.as_str() {
                    "TP1001" | "TP1002" | "TP1003" | "TP2001" => "declared-unverified",
                    "TP9001" | "TP9002" => "unknown-boundary",
                    _ => "static-source",
                };
                let repair = match diagnostic.code.as_str() {
                    "TP1001" => Some(
                        "Keep the use before close, or guard it with a lifecycle check.".into(),
                    ),
                    "TP1002" => Some(
                        "Close the resource on every reported exit, or return it through a resolved Owned[T] boundary.".into(),
                    ),
                    "TP1003" if diagnostic.message.contains("Borrowed") => Some(
                        "Return a Borrowed value under a Borrowed contract, or return an open locally owned resource under Owned[T].".into(),
                    ),
                    "TP1003" => Some(
                        "Return an open resource from an Owned[T] function; do not close it before handoff.".into(),
                    ),
                    "TP2001" => Some(
                        "Leave cleanup to the owner or pass an explicitly owned resource.".into(),
                    ),
                    "TP9001" => Some(
                        "Resolve the reported model boundary or keep the affected lifecycle state unknown.".into(),
                    ),
                    _ => None,
                };
                CheckDiagnostic {
                    code: diagnostic.code,
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                    primary: diagnostic.primary,
                    related: diagnostic.related,
                    certainty: diagnostic.certainty,
                    completion,
                    trust: trust.into(),
                    coverage_reason,
                    repair,
                }
            })
            .collect();
        let available_models = vec![
            CheckModelMetadata {
                id: "TPM-CTX-001".into(),
                qualified_symbol: "contextlib.closing".into(),
                source_revisions: ["v3.11.0", "v3.12.0", "v3.13.0"]
                    .into_iter()
                    .map(|tag| format!("CPython {tag} Lib/contextlib.py"))
                    .collect(),
                source_urls: [
                    "https://github.com/python/cpython/blob/v3.11.0/Lib/contextlib.py",
                    "https://github.com/python/cpython/blob/v3.12.0/Lib/contextlib.py",
                    "https://github.com/python/cpython/blob/v3.13.0/Lib/contextlib.py",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                selection_rule: "Exact import identity only; applies when the selected target Python version is 3.11, 3.12, or 3.13.".into(),
                modeled_behavior: "On context exit, call the wrapped object's resolved close method. Do not suppress the body exception. Cleanup success or failure follows the wrapped TypePact close summary.".into(),
                qualification: "Source-reviewed against the listed CPython tags; controlled runtime cases and exact interpreter/source hashes are recorded in docs/planning/phases/0.3/contextlib-conformance.md; 0.3 release qualification pending.".into(),
            },
            CheckModelMetadata {
                id: "TPM-CTX-002".into(),
                qualified_symbol: "contextlib.aclosing".into(),
                source_revisions: ["v3.11.0", "v3.12.0", "v3.13.0"]
                    .into_iter()
                    .map(|tag| format!("CPython {tag} Lib/contextlib.py"))
                    .collect(),
                source_urls: [
                    "https://github.com/python/cpython/blob/v3.11.0/Lib/contextlib.py",
                    "https://github.com/python/cpython/blob/v3.12.0/Lib/contextlib.py",
                    "https://github.com/python/cpython/blob/v3.13.0/Lib/contextlib.py",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                selection_rule: "Exact import identity only; applies when the selected target Python version is 3.11, 3.12, or 3.13.".into(),
                modeled_behavior: "On async context exit, await the wrapped object's resolved aclose method. Do not suppress the body exception. Cleanup success, failure, and cancellation follow the wrapped TypePact close summary.".into(),
                qualification: "Source-reviewed against the listed CPython tags; controlled runtime cases and exact interpreter/source hashes are recorded in docs/planning/phases/0.3/contextlib-conformance.md; 0.3 release qualification pending.".into(),
            },
        ];
        let mut available_models = if matches!(target_python_version, "3.11" | "3.12" | "3.13") {
            available_models
        } else {
            Vec::new()
        };
        available_models.extend(
            flow_models
                .resources
                .iter()
                .filter(|model| {
                    model
                        .supported_python_versions
                        .contains(target_python_version)
                })
                .map(|model| CheckModelMetadata {
                    id: model.id.clone(),
                    qualified_symbol: format!("{}::{}", model.qualified_symbol, model.close_method),
                    source_revisions: vec![format!(
                        "{} (Python {})",
                        model.source_revision,
                        model
                            .supported_python_versions
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )],
                    source_urls: vec![model.source_url.clone()],
                    selection_rule: format!(
                        "Exact resource symbol and close method; selected for Python versions {}.",
                        model
                            .supported_python_versions
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    modeled_behavior: format!(
                        "normal={:?}; exceptional={:?}; cancelled={:?}; asynchronous={}; repeat-close={:?}; post-close-methods={}",
                        model.normal,
                        model.exceptional,
                        model.cancelled,
                        model.asynchronous,
                        model.repeat_close,
                        model
                            .post_close_methods
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    qualification: format!(
                        "trust={}; reviewer={}; platform={}; duty-handoff={}; suppression={}; argument-aliases={}; result-alias={}; valid={}; invalid={}; unknown={}",
                        model.trust,
                        model.reviewer,
                        model.platform_assumptions.join(", "),
                        model.duty_handoff,
                        model.suppression,
                        model
                            .argument_aliases
                            .iter()
                            .map(|alias| format!("{}->{}", alias.source, alias.target))
                            .collect::<Vec<_>>()
                            .join(", "),
                        model.result_alias.as_deref().unwrap_or("none"),
                        model.valid_cases.join("; "),
                        model.invalid_cases.join("; "),
                        model.unknown_cases.join("; ")
                    ),
                }),
        );
        available_models.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            check_schema_version: CHECK_SCHEMA_VERSION,
            schema_version: inspection.schema_version,
            target_python_version: target_python_version.into(),
            model_catalog_version: CHECK_MODEL_CATALOG_VERSION,
            available_models,
            assurance: inspection.assurance,
            contracts: inspection.contracts,
            diagnostics,
            coverage: inspection.coverage,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Access {
    Unspecified,
    Shared,
    Owned,
    Exclusive,
}

impl Access {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Shared => "shared",
            Self::Owned => "owned",
            Self::Exclusive => "exclusive",
        }
    }
}

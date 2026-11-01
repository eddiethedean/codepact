use serde::{Deserialize, Serialize};

pub const CHECK_SCHEMA_VERSION: u8 = 1;
pub const CHECK_MODEL_CATALOG_VERSION: u8 = 1;

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
        let available_models = if matches!(target_python_version, "3.11" | "3.12" | "3.13") {
            available_models
        } else {
            Vec::new()
        };
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

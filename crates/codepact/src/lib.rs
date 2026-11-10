//! Bounded, non-executing inspection of CodePact declarations in Python source.
pub mod adoption;
pub mod db;
pub use db::{
    AnalysisCancelled, AnalysisSession, CancellationToken, SessionCheckResult, SessionInputError,
    SessionInspection,
};
mod flow;
mod frontend;
mod identifiers;
pub use identifiers::is_python_identifier;
mod model;
mod semantic;
mod source;
pub(crate) use model::Access;
pub use model::{
    CheckDiagnostic, CheckModelMetadata, CheckResult, Contract, Coverage, Diagnostic, Inspection,
    Lifecycle, Provenance, Span, CHECK_MODEL_CATALOG_VERSION, CHECK_SCHEMA_VERSION,
};
pub(crate) use source::span as source_span;
pub const SCHEMA_VERSION: u8 = 0;
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 1_000_000;
pub const DEFAULT_MAX_DECLARATIONS: usize = 10_000;
pub const DEFAULT_MAX_INPUT_FILES: usize = 10_000;
pub const DEFAULT_MAX_AST_DECLARATIONS: usize = 20_000;
pub const DEFAULT_MAX_TOTAL_AST_DECLARATIONS: usize = 50_000;
pub const DEFAULT_MAX_IMPORT_EDGES: usize = 50_000;
pub const DEFAULT_MAX_SESSION_MUTATIONS: usize = 1_000;

pub(crate) fn cancelled_inspection(files: &[SourceFile]) -> Inspection {
    let path = files.first().map_or("<input>", |file| file.path.as_str());
    let source_bytes = files.iter().fold(0usize, |total, file| {
        total.saturating_add(file.source.len())
    });
    Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: vec![Diagnostic {
            semantic_identity: Vec::new(),
            code: "TP9002".into(),
            severity: "warning".into(),
            message: "analysis cancelled; no result was published".into(),
            primary: Span {
                file: path.into(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
            },
            related: Vec::new(),
            certainty: "unknown".into(),
        }],
        coverage: Coverage {
            status: "incomplete".into(),
            reasons: vec![format!(
                "{path}:1: analysis cancelled; no result was published"
            )],
            source_bytes,
            declarations: 0,
        },
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub module: String,
    pub source: String,
}

impl SourceFile {
    pub fn new(
        path: impl Into<String>,
        module: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            module: module.into(),
            source: source.into(),
        }
    }
}

fn input_warning(result: &mut Inspection, path: &str, message: &str) {
    result.coverage.status = "incomplete".into();
    result.coverage.reasons.push(format!("{path}:1: {message}"));
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
        code: "TP9001".into(),
        severity: "warning".into(),
        message: message.into(),
        primary: Span {
            file: path.into(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 1,
        },
        related: Vec::new(),
        certainty: "unknown".into(),
    });
}

/// Inspect one or more explicitly supplied files. Source is treated only as text.
pub fn inspect(files: &[SourceFile], python_version: &str) -> Inspection {
    AnalysisSession::default().inspect(files, python_version)
}

/// Check resource cleanup duties in the bounded, currently supported subset.
pub fn check(files: &[SourceFile], python_version: &str) -> Inspection {
    AnalysisSession::default().check(files, python_version)
}

/// Check lifecycle obligations plus the 0.4 synchronous must-use rule.
/// Existing `check` and `check_report` keep the released rule set.
pub fn check_with_must_use(files: &[SourceFile], python_version: &str) -> Inspection {
    AnalysisSession::default().check_with_must_use(files, python_version)
}

/// Check resource cleanup duties and return the versioned checker-specific
/// result schema. `check` remains available for callers that use `Inspection`.
pub fn check_report(files: &[SourceFile], python_version: &str) -> CheckResult {
    AnalysisSession::default().check_report(files, python_version)
}

pub(crate) fn source_limit_inspection(files: &[(String, usize)]) -> Inspection {
    let mut result = Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: Vec::new(),
        coverage: Coverage {
            status: "complete-for-supported-subset".into(),
            reasons: Vec::new(),
            source_bytes: files
                .iter()
                .fold(0usize, |total, (_, bytes)| total.saturating_add(*bytes)),
            declarations: 0,
        },
    };
    let mut cumulative = 0usize;
    for (path, bytes) in files {
        cumulative = cumulative.saturating_add(*bytes);
        if *bytes > DEFAULT_MAX_SOURCE_BYTES {
            input_warning(
                &mut result,
                path,
                "source file exceeds the 1,000,000 byte inspection limit",
            );
            break;
        }
        if cumulative > DEFAULT_MAX_SOURCE_BYTES {
            input_warning(
                &mut result,
                path,
                "supplied source files exceed the 1,000,000 byte total inspection limit",
            );
            break;
        }
    }
    result
}

pub(crate) fn invalid_input_inspection(
    path: &str,
    message: &str,
    source_bytes: usize,
) -> Inspection {
    let mut result = Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: Vec::new(),
        coverage: Coverage {
            status: "complete-for-supported-subset".into(),
            reasons: Vec::new(),
            source_bytes,
            declarations: 0,
        },
    };
    input_warning(&mut result, path, message);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_resource_and_borrowed() {
        let source = "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by=\"close\")\nclass Connection:\n    def close(self): pass\ndef inspect(conn: Borrowed[Connection]): pass\n";
        let result = inspect(&[SourceFile::new("example.py", "example", source)], "3.11");
        assert_eq!(result.contracts.len(), 2, "{result:#?}");
        assert!(result.contracts.iter().any(|contract| {
            contract.access == "shared"
                && contract
                    .lifecycle
                    .as_ref()
                    .is_some_and(|lifecycle| lifecycle.closed_by == "close")
        }));
    }

    #[test]
    fn local_lookalike_is_not_recognized() {
        let source =
            "def resource(x): pass\n@resource(closed_by=\"close\")\nclass Connection: pass\n";
        let result = inspect(&[SourceFile::new("x.py", "x", source)], "3.11");
        assert!(result.contracts.is_empty());
    }

    #[test]
    fn bounded_and_deterministic() {
        let file = SourceFile::new("x.py", "x", "class A: pass\ndef f(x: A): pass\n");
        assert_eq!(
            inspect(std::slice::from_ref(&file), "3.11"),
            inspect(&[file], "3.11")
        );
    }

    #[test]
    fn syntax_errors_have_source_locations_and_incomplete_status() {
        let result = inspect(
            &[SourceFile::new("broken.py", "broken", "def f(:\n pass\n")],
            "3.11",
        );
        assert_eq!(result.coverage.status, "incomplete");
        assert_eq!(result.diagnostics[0].code, "TP4001");
        assert!(result.diagnostics[0].primary.line >= 1);
    }

    #[test]
    fn oversized_source_stops_before_parsing() {
        let source = format!("{}class C: pass\n", "#".repeat(DEFAULT_MAX_SOURCE_BYTES));
        let result = inspect(&[SourceFile::new("large.py", "large", source)], "3.11");
        assert_eq!(result.coverage.status, "incomplete");
        assert!(result.contracts.is_empty());
    }
}

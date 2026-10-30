use serde::{Deserialize, Serialize};

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

//! Inputs and evidence for the 0.8 analysis profiles.

use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeSet};
mod models;
mod publication;
mod runtime;
pub(crate) use models::{
    normalize_submodule_imports, publication_origin, scope_entry, ApplicableFactory,
    PreparedFoundations,
};
pub use models::{
    parse_project_models, validate_project_models, ExceptionalClose, ProjectResourceModel,
    ProviderIssue, ProviderOrigin, ProviderSelection, ScopedSource,
};
pub use publication::{
    decode_manifest, export_manifest, module_for_source, normalized_distribution,
    regular_relative_path, sha256, verify_publication, ArtifactFile, Manifest, ProviderPackage,
    PublicationLocation, PublishedContract, PublishedSymbol, MANIFEST_FILENAME, MARKER_FILENAME,
    MAX_METADATA_BYTES,
};
pub(crate) use publication::{select_publication, SelectedPublication};
pub(crate) use runtime::stdlib_catalog;
pub use runtime::{RuntimeProfile, CATALOG_REVISION};

pub const CHECK_SCHEMA_VERSION: u8 = 2;
pub const MODEL_CATALOG_VERSION: u8 = 5;

/// Absolute imports available to the bounded source analysis.
pub fn static_imports(file: &crate::SourceFile, target: &str) -> Vec<String> {
    crate::frontend::parse_frontend(&file.source, &file.path, &file.module, target)
        .map(|parsed| parsed.absolute_imports)
        .unwrap_or_default()
}

/// Extra inputs for the opt-in 0.8 check. Use the same inputs for a fresh or
/// reused session. Existing source-only entry points keep their options.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FoundationInputs {
    pub runtime: Option<RuntimeProfile>,
    pub providers: Vec<ProviderPackage>,
    pub project_models: Vec<ProjectResourceModel>,
    pub uv_lock: Option<String>,
    pub provider_issues: Vec<ProviderIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModelUse {
    pub symbol: String,
    pub effect: String,
    pub origin: String,
    pub applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
    pub site: crate::Span,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<ProviderOrigin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundationEvidence {
    pub runtime: Option<RuntimeProfile>,
    pub catalog_revision: String,
    pub model_uses: Vec<ModelUse>,
    pub providers: Vec<ProviderSelection>,
    pub provider_issues: Vec<ProviderIssue>,
    pub scope: Vec<ScopedSource>,
    pub exhausted_limits: Vec<String>,
    pub lock_sha256: Option<String>,
    pub project_models_sha256: Option<String>,
    pub comparison: ComparisonDescriptor,
}

/// Versioned semantic inputs for CR01. Comparison of two snapshots is a
/// separate operation in the next phase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonDescriptor {
    pub schema_version: u8,
    pub engine_version: String,
    pub rule_semantic_revision: u8,
    pub finding_identity_revision: u8,
    pub source_target: String,
    pub ownership: crate::Ownership,
    pub finite_matches: bool,
    pub catalog_revision: String,
    pub normalization_revision: u8,
    pub preservation_policy_revision: u8,
}

// Each tracked file query returns its own journal. Nested queries restore the
// previous journal, including when Salsa cancels the current calculation.
thread_local! {
    static MODEL_USES: RefCell<Option<BTreeSet<ModelUse>>> = const { RefCell::new(None) };
}

pub(crate) fn collect_model_uses<T>(enabled: bool, run: impl FnOnce() -> T) -> (T, Vec<ModelUse>) {
    struct Restore(Option<BTreeSet<ModelUse>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            MODEL_USES.with(|uses| uses.replace(self.0.take()));
        }
    }
    let previous = MODEL_USES.with(|uses| uses.replace(enabled.then(BTreeSet::new)));
    let _restore = Restore(previous);
    let result = run();
    let uses = MODEL_USES.with(|uses| uses.take().into_iter().flatten().collect());
    (result, uses)
}

pub(crate) fn record_model_use(
    symbol: &str,
    effect: &str,
    origin: &str,
    site: crate::Span,
    applicability: Result<(), &str>,
) {
    record_use(symbol, effect, origin, site, applicability, None);
}

pub(crate) fn record_use(
    symbol: &str,
    effect: &str,
    origin: &str,
    site: crate::Span,
    applicability: Result<(), &str>,
    provider: Option<ProviderOrigin>,
) {
    MODEL_USES.with(|uses| {
        if let Some(uses) = uses.borrow_mut().as_mut() {
            uses.insert(ModelUse {
                symbol: symbol.into(),
                effect: effect.into(),
                origin: origin.into(),
                applicable: applicability.is_ok(),
                reason: applicability.err().map(str::to_owned),
                site,
                provider,
            });
        }
    });
}

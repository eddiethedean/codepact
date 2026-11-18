//! Inputs and evidence for the 0.8 analysis profiles.

use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeSet};
mod models;
mod publication;
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

pub const CATALOG_REVISION: &str = "cpython-3.13.16-linux-x86_64-v1";
pub const CHECK_SCHEMA_VERSION: u8 = 2;
pub const MODEL_CATALOG_VERSION: u8 = 5;

/// Absolute imports available to the bounded source analysis.
pub fn static_imports(file: &crate::SourceFile, target: &str) -> Vec<String> {
    crate::frontend::parse_frontend(&file.source, &file.path, &file.module, target)
        .map(|parsed| parsed.absolute_imports)
        .unwrap_or_default()
}

/// The runtime of the analyzed program. The Python grammar target is a
/// separate input and the checker never derives these values from its host.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimeProfile {
    pub implementation: String,
    pub python_version: String,
    pub os: String,
    pub architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_loop: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_method: Option<String>,
    #[serde(default)]
    pub assume_standard_process_wait: bool,
}

impl RuntimeProfile {
    pub(crate) fn applicability(&self, target: &str) -> Result<(), &'static str> {
        if self.python_version.rsplit_once('.').map(|(minor, _)| minor) != Some(target) {
            return Err("the analyzed runtime and Python source target disagree");
        }
        if self.implementation != "cpython"
            || self.python_version != "3.13.16"
            || self.os != "linux"
            || self.architecture != "x86_64"
        {
            return Err("the model requires CPython 3.13.16 on Linux x86_64");
        }
        Ok(())
    }

    pub(crate) fn asyncio(&self, target: &str, default_factory: bool) -> Result<(), &'static str> {
        self.applicability(target)?;
        if self.event_loop.as_deref() != Some("default") || !default_factory {
            return Err(
                "the asyncio model requires the default event loop and non-eager task factory",
            );
        }
        Ok(())
    }

    pub(crate) fn spawn(&self, target: &str, method: &str) -> Result<(), &'static str> {
        self.applicability(target)?;
        if method != "spawn"
            || self
                .start_method
                .as_deref()
                .is_some_and(|value| value != method)
        {
            return Err("the multiprocessing model requires an explicit compatible spawn context");
        }
        Ok(())
    }
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

pub(crate) fn stdlib_catalog(
    profile: &RuntimeProfile,
    target: &str,
    ownership: crate::Ownership,
) -> Vec<crate::CheckModelMetadata> {
    if profile.applicability(target).is_err() {
        return Vec::new();
    }
    let mut catalog = vec![(
        "TPM-FILE-001",
        "builtins.open",
        "Modules/_io/textio.c",
        "Ordinary text files, local aliasing, close and context exit.",
    )];
    if profile
        .asyncio(target, ownership.assumes_default_task_factory())
        .is_ok()
    {
        catalog.push((
            "TPM-ASYNCIO-001",
            "asyncio.create_task",
            "Lib/asyncio/tasks.py",
            "Local scheduling, direct wait, cancellation, shield and timeout.",
        ));
    }
    if ownership.includes_advanced_contracts() && profile.spawn(target, "spawn").is_ok() {
        catalog.extend([
            (
                "TPM-PROCESS-001",
                "multiprocessing.Process",
                "Lib/multiprocessing/process.py",
                "Explicit-spawn local process transitions, qualified join and close.",
            ),
            (
                "TPM-POOL-001",
                "multiprocessing.Pool",
                "Lib/multiprocessing/pool.py",
                "Explicit-spawn submission and local Pool shutdown.",
            ),
        ]);
    }
    catalog.into_iter().map(|(id, symbol, source, behavior)| crate::CheckModelMetadata {
        id: id.into(),
        qualified_symbol: symbol.into(),
        source_revisions: vec!["v3.13.16".into()],
        source_urls: vec![format!("https://github.com/python/cpython/blob/v3.13.16/{source}")],
        selection_rule: "Declared CPython 3.13.16/Linux x86_64 runtime, exact binding and applicable model assumptions.".into(),
        modeled_behavior: behavior.into(),
        qualification: "Phase 0.8 experimental model. Release qualification is tracked in the phase plan.".into(),
    }).collect()
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

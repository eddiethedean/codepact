//! Selection of provider publications from validated source artifacts.

mod compare;
mod manifest;
mod package;
mod source;
use source::{source_publication, SourcePublication};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) use compare::normalized_provider_agreements;
pub use compare::{
    capture_publication, diff_publications, ContractChange, ContractChangeKind, ContractDiff,
    PublicationIdentity, PublicationSnapshot,
};
pub use manifest::{
    decode_manifest, Manifest, PublicationLocation, PublishedContract, PublishedSymbol,
};
pub use package::{
    module_for_source, normalized_distribution, regular_relative_path, sha256, ArtifactFile,
    ProviderPackage, MANIFEST_FILENAME, MARKER_FILENAME, MAX_METADATA_BYTES,
};
pub(super) use package::{qualified, version_is_literal};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SelectedPublication {
    pub package: ProviderPackage,
    pub exports: BTreeMap<String, PublishedSymbol>,
    pub source_exports: BTreeMap<String, PublishedSymbol>,
    pub importable_children: BTreeSet<String>,
    pub cleanup_methods: BTreeMap<String, BTreeSet<String>>,
    pub unavailable: BTreeMap<String, String>,
    pub reason: Option<String>,
    pub origin: &'static str,
    pub digest: String,
}

/// Export only supported declarations, without evaluating the staged Python.
pub fn export_manifest(package: &ProviderPackage, target: &str) -> Result<Manifest, String> {
    if !package.marked {
        return Err("contract export requires an empty py.codepact marker".into());
    }
    package.validate(target)?;
    let source = source_publication(package, target)?;
    Ok(Manifest::from_exports(package, target, source.exports))
}

/// Validate identity, inventory and facts against the selected contents.
/// The author verifier and installed consumer use the same validation.
pub fn verify_publication(package: &ProviderPackage, target: &str) -> Result<Manifest, String> {
    let selected = select_publication(package, target);
    if let Some(reason) = selected.reason {
        return Err(reason);
    }
    let content = package
        .manifest
        .as_deref()
        .ok_or("the built provider has no contract manifest")?;
    let manifest = decode_manifest(content)?;
    if let Some(reason) = selected.unavailable.values().next() {
        return Err(reason.clone());
    }
    Ok(manifest)
}

pub(crate) fn select_publication(package: &ProviderPackage, target: &str) -> SelectedPublication {
    let mut selected = SelectedPublication {
        package: package.clone(),
        exports: BTreeMap::new(),
        source_exports: BTreeMap::new(),
        importable_children: BTreeSet::new(),
        cleanup_methods: BTreeMap::new(),
        unavailable: BTreeMap::new(),
        reason: None,
        origin: if package.manifest.is_some() {
            "manifest"
        } else if package.marked {
            "inline-source"
        } else {
            "unmarked"
        },
        digest: package.digest(),
    };
    let result = (|| {
        package.validate(target)?;
        let SourcePublication {
            exports: source,
            importable_children,
            cleanup_methods,
        } = source_publication(package, target)?;
        selected.importable_children = importable_children;
        selected.cleanup_methods = cleanup_methods;
        selected.source_exports = source.clone();
        if !package.marked {
            return Ok(());
        }
        let Some(content) = &package.manifest else {
            selected.exports = source;
            return Ok(());
        };
        let publication = manifest::select_manifest_exports(content, package, target, &source)?;
        selected.exports = publication.exports;
        selected.unavailable = publication.unavailable;
        Ok(())
    })();
    if let Err(reason) = result {
        selected.reason = Some(reason);
        selected.exports.clear();
        selected.unavailable.clear();
    }
    selected
}

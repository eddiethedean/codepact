//! Wire format and source-backed validation of an explicitly published surface.

use super::{regular_relative_path, ArtifactFile, ProviderPackage, MAX_METADATA_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MANIFEST_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u8,
    pub producer_version: String,
    pub distribution: String,
    pub version: String,
    pub import_scope: String,
    pub python_version: String,
    pub files: Vec<ArtifactFile>,
    pub symbols: Vec<PublishedSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedSymbol {
    pub public_name: String,
    pub canonical_name: String,
    pub symbol_kind: String,
    pub contracts: Vec<PublishedContract>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedContract {
    pub kind: String,
    pub version: u8,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_method: Option<String>,
    pub required_capabilities: Vec<String>,
    pub evidence: PublicationLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationLocation {
    pub path: String,
    pub line: usize,
}

impl Manifest {
    pub(super) fn from_exports(
        package: &ProviderPackage,
        target: &str,
        exports: BTreeMap<String, PublishedSymbol>,
    ) -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            producer_version: env!("CARGO_PKG_VERSION").into(),
            distribution: package.distribution.clone(),
            version: package.version.clone(),
            import_scope: package.import_scope.clone(),
            python_version: target.into(),
            files: package.bindings(),
            symbols: exports
                .into_values()
                .filter(|symbol| !symbol.contracts.is_empty())
                .collect(),
        }
    }
}

pub fn decode_manifest(content: &str) -> Result<Manifest, String> {
    if content.len() > MAX_METADATA_BYTES {
        return Err("contract manifest exceeds the metadata byte limit".into());
    }
    let manifest: Manifest = serde_json::from_str(content)
        .map_err(|error| format!("invalid contract manifest: {error}"))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported contract manifest schema {}",
            manifest.schema_version
        ));
    }
    Ok(manifest)
}

/// Projection of one manifest over its selected source artifact. Declined
/// facts can leave other independently supported facts usable.
pub(super) struct ManifestExports {
    pub(super) exports: BTreeMap<String, PublishedSymbol>,
    pub(super) unavailable: BTreeMap<String, String>,
}

pub(super) fn select_manifest_exports(
    content: &str,
    package: &ProviderPackage,
    target: &str,
    source: &BTreeMap<String, PublishedSymbol>,
) -> Result<ManifestExports, String> {
    let manifest = decode_manifest(content)?;
    if (
        manifest.distribution.as_str(),
        manifest.version.as_str(),
        manifest.import_scope.as_str(),
    ) != (
        &package.distribution,
        &package.version,
        &package.import_scope,
    ) || manifest.producer_version.trim().is_empty()
    {
        return Err("contract manifest distribution, version or import scope does not match the selected artifact".into());
    }
    if manifest.python_version != target {
        return Err("contract manifest Python target is incompatible with this analysis".into());
    }
    if manifest.files != package.bindings() {
        return Err(
            "stale contract manifest: packaged files or SHA-256 digests do not match".into(),
        );
    }
    let mut exports = BTreeMap::new();
    let mut unavailable = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for mut symbol in manifest.symbols {
        if !seen.insert(symbol.public_name.clone()) {
            return Err("contract manifest contains duplicate public exports".into());
        }
        let Some(derived) = source.get(&symbol.public_name) else {
            return Err(format!(
                "contract manifest export {} is unresolved in the selected source",
                symbol.public_name
            ));
        };
        if symbol.canonical_name != derived.canonical_name
            || symbol.symbol_kind != derived.symbol_kind
        {
            return Err(format!(
                "contract manifest canonical identity disagrees for {}",
                symbol.public_name
            ));
        }
        let mut facts = BTreeSet::new();
        let mut valid = Vec::new();
        for fact in symbol.contracts {
            if !facts.insert((fact.kind.clone(), fact.subject.clone())) {
                return Err(format!("duplicate contract fact on {}", symbol.public_name));
            }
            if !regular_relative_path(&fact.evidence.path)
                || !package.sources.iter().any(|file| {
                    file.path == fact.evidence.path
                        && (1..=file.source.lines().count()).contains(&fact.evidence.line)
                })
            {
                return Err(format!(
                    "invalid contract evidence location on {}",
                    symbol.public_name
                ));
            }
            if !supports(&fact) {
                unavailable.insert(
                    symbol.public_name.clone(),
                    format!(
                        "unsupported mandatory capability for {} {}",
                        symbol.public_name, fact.kind
                    ),
                );
                continue;
            }
            let declared_must_use = fact.kind == "must-use"
                && fact.subject == "return"
                && symbol.symbol_kind == "function"
                && fact.resource.is_none()
                && fact.close_method.is_none();
            let checked_source = derived
                .contracts
                .iter()
                .any(|candidate| same_fact(candidate, &fact));
            if declared_must_use || checked_source {
                valid.push(fact);
            } else {
                unavailable.insert(
                    symbol.public_name.clone(),
                    format!(
                        "published effect for {} has no matching checked source declaration",
                        symbol.public_name
                    ),
                );
            }
        }
        symbol.contracts = valid;
        exports.insert(symbol.public_name.clone(), symbol);
    }
    Ok(ManifestExports {
        exports,
        unavailable,
    })
}

fn same_fact(left: &PublishedContract, right: &PublishedContract) -> bool {
    left.kind == right.kind
        && left.subject == right.subject
        && left.resource == right.resource
        && left.close_method == right.close_method
        && left.evidence == right.evidence
}

fn supports(fact: &PublishedContract) -> bool {
    fact.version == 1
        && matches!(
            fact.kind.as_str(),
            "must-use" | "resource" | "owned-return" | "owned-parameter"
        )
        && fact.required_capabilities.iter().all(|capability| {
            matches!(
                capability.as_str(),
                "synchronous-result-use" | "synchronous-resource-v1" | "synchronous-ownership-v1"
            )
        })
}

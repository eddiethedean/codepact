//! Compare checked public contracts from the downstream caller's perspective.

use super::{select_publication, ProviderPackage, PublishedSymbol};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_DIFF_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationIdentity {
    pub distribution: String,
    pub version: String,
    pub import_scope: String,
    pub python_version: String,
    pub artifact_sha256: String,
}

/// Source-backed facts retained even when one mandatory capability is unavailable.
/// Construct this only from the bounded publication validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationSnapshot {
    identity: PublicationIdentity,
    symbols: BTreeMap<String, PublishedSymbol>,
    unavailable: BTreeMap<String, String>,
}

pub fn capture_publication(
    package: &ProviderPackage,
    target: &str,
) -> Result<PublicationSnapshot, String> {
    if !package.marked || package.manifest.is_none() {
        return Err("contract comparison requires a manifest and an empty marker".into());
    }
    let selected = select_publication(package, target);
    if let Some(reason) = selected.reason {
        return Err(reason);
    }
    let mut symbols = selected.exports;
    let mut unavailable = selected.unavailable;
    for (name, mut symbol) in selected.source_exports {
        let published = symbols.get(&name);
        let omitted = symbol.contracts.iter().any(|fact| {
            published.is_none_or(|published| {
                !published
                    .contracts
                    .iter()
                    .any(|present| fact.kind == present.kind && fact.subject == present.subject)
            })
        });
        if omitted {
            unavailable
                .entry(name.clone())
                .or_insert_with(|| "a checked source contract is absent from the manifest".into());
        }
        symbol.contracts.clear();
        symbols.entry(name).or_insert(symbol);
    }
    Ok(PublicationSnapshot {
        identity: PublicationIdentity {
            distribution: package.distribution.clone(),
            version: package.version.clone(),
            import_scope: package.import_scope.clone(),
            python_version: target.into(),
            artifact_sha256: selected.digest,
        },
        symbols,
        unavailable,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContractChangeKind {
    Compatible,
    NewCallerObligation,
    WeakenedGuarantee,
    InsufficientEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContractChange {
    pub public_symbol: String,
    pub kind: ContractChangeKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDiff {
    pub contract_diff_schema_version: u8,
    pub before: PublicationIdentity,
    pub after: PublicationIdentity,
    pub compatible: bool,
    pub changes: Vec<ContractChange>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct Fact {
    resource: Option<String>,
    close_method: Option<String>,
    capabilities: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct PublicSymbol {
    canonical_name: String,
    symbol_kind: String,
    facts: BTreeMap<(String, String), Fact>,
}

fn normalize_publication(
    symbols: &BTreeMap<String, PublishedSymbol>,
) -> BTreeMap<String, PublicSymbol> {
    let mut resources = BTreeMap::new();
    for (name, symbol) in symbols {
        if symbol.symbol_kind == "resource" {
            resources
                .entry(symbol.canonical_name.as_str())
                .or_insert(name.as_str());
        }
    }
    symbols
        .iter()
        .map(|(name, symbol)| {
            let facts = symbol
                .contracts
                .iter()
                .map(|fact| {
                    let resource = fact.resource.as_ref().map(|resource| {
                        resources
                            .get(resource.as_str())
                            .map_or_else(|| resource.clone(), |name| (*name).to_owned())
                    });
                    (
                        (fact.kind.clone(), fact.subject.clone()),
                        Fact {
                            resource,
                            close_method: fact.close_method.clone(),
                            capabilities: fact.required_capabilities.iter().cloned().collect(),
                        },
                    )
                })
                .collect();
            (
                name.clone(),
                PublicSymbol {
                    canonical_name: symbol.canonical_name.clone(),
                    symbol_kind: symbol.symbol_kind.clone(),
                    facts,
                },
            )
        })
        .collect()
}

pub(crate) fn normalized_provider_agreements(
    package: &ProviderPackage,
    target: &str,
) -> BTreeSet<String> {
    normalize_publication(&select_publication(package, target).exports)
        .into_iter()
        .flat_map(|(symbol, properties)| {
            properties
                .facts
                .into_iter()
                .map(move |((kind, subject), value)| {
                    serde_json::to_string(&(
                        &package.distribution,
                        &symbol,
                        &properties.symbol_kind,
                        kind,
                        subject,
                        value,
                    ))
                    .expect("provider agreement serialization")
                })
        })
        .collect()
}

pub fn diff_publications(
    before: &PublicationSnapshot,
    after: &PublicationSnapshot,
) -> ContractDiff {
    use ContractChangeKind::{
        Compatible, InsufficientEvidence, NewCallerObligation, WeakenedGuarantee,
    };
    let left = normalize_publication(&before.symbols);
    let right = normalize_publication(&after.symbols);
    let symbols = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
    let mut changes = BTreeSet::new();
    let mut add = |name: &str, kind, detail: String| {
        changes.insert(ContractChange {
            public_symbol: name.into(),
            kind,
            detail,
        });
    };
    if before.identity.distribution != after.identity.distribution
        || before.identity.import_scope != after.identity.import_scope
        || before.identity.python_version != after.identity.python_version
    {
        add(
            "<publication>",
            InsufficientEvidence,
            "distribution, import scope, or Python target changed".into(),
        );
    }
    for name in symbols {
        if let Some(detail) = before.unavailable.get(name) {
            add(name, InsufficientEvidence, format!("before: {detail}"));
        }
        if let Some(detail) = after.unavailable.get(name) {
            add(name, InsufficientEvidence, format!("after: {detail}"));
        }
        if before.unavailable.contains_key(name) || after.unavailable.contains_key(name) {
            continue;
        }
        let (Some(left), Some(right)) = (left.get(name), right.get(name)) else {
            if left
                .get(name)
                .is_some_and(|symbol| !symbol.facts.is_empty())
            {
                add(
                    name,
                    InsufficientEvidence,
                    "previously published symbol is absent from the checked surface".into(),
                );
            } else if right
                .get(name)
                .is_some_and(|symbol| !symbol.facts.is_empty())
            {
                add(name, Compatible, "new public symbol".into());
            }
            continue;
        };
        let facts = left
            .facts
            .keys()
            .chain(right.facts.keys())
            .collect::<BTreeSet<_>>();
        if facts.is_empty() {
            continue;
        }
        if left.symbol_kind != right.symbol_kind {
            add(name, InsufficientEvidence, "symbol kind changed".into());
        }
        for key in facts {
            let description = format!("{} on {}", key.0, key.1);
            let added = match key.0.as_str() {
                "must-use" | "must-handle-return" | "owned-parameter" | "owned-return" => {
                    NewCallerObligation
                }
                "resource" => Compatible,
                _ => InsufficientEvidence,
            };
            let removed = match key.0.as_str() {
                "must-use" | "must-handle-return" | "owned-parameter" => Compatible,
                "resource" | "owned-return" => WeakenedGuarantee,
                _ => InsufficientEvidence,
            };
            match (left.facts.get(key), right.facts.get(key)) {
                (None, Some(_)) => add(name, added, format!("added {description}")),
                (Some(_), None) => add(name, removed, format!("removed {description}")),
                (Some(before), Some(after)) if before != after => {
                    add(name, removed, format!("replaced previous {description}"));
                    add(name, added, format!("introduced replacement {description}"));
                    if added == Compatible && removed == Compatible {
                        add(name, InsufficientEvidence, format!("changed {description}"));
                    }
                }
                _ => {}
            }
        }
        if left.symbol_kind == right.symbol_kind
            && left.facts == right.facts
            && left.canonical_name != right.canonical_name
        {
            add(
                name,
                Compatible,
                format!(
                    "implementation moved from {} to {}",
                    left.canonical_name, right.canonical_name
                ),
            );
        }
    }
    ContractDiff {
        contract_diff_schema_version: CONTRACT_DIFF_SCHEMA_VERSION,
        before: before.identity.clone(),
        after: after.identity.clone(),
        compatible: changes.iter().all(|change| change.kind == Compatible),
        changes: changes.into_iter().collect(),
    }
}

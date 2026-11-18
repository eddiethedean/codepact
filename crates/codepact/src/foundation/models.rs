use super::{publication, FoundationInputs, ProviderPackage, RuntimeProfile};
use crate::{db::ParseSummary, frontend::Declaration};
use crate::{SourceFile, Span};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectResourceModel {
    pub distribution: String,
    pub version: String,
    pub factory: String,
    pub resource: String,
    pub close_method: String,
    pub provenance: String,
    #[serde(default)]
    pub override_publication: bool,
    #[serde(default)]
    pub exceptional_close: ExceptionalClose,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExceptionalClose {
    #[default]
    Unknown,
    Closed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    schema_version: u8,
    models: Vec<ProjectResourceModel>,
}

pub fn parse_project_models(
    content: &str,
    target: &str,
) -> Result<Vec<ProjectResourceModel>, String> {
    if content.len() > publication::MAX_METADATA_BYTES {
        return Err("project contract file exceeds the metadata byte limit".into());
    }
    let mut file: ModelFile = serde_json::from_str(content)
        .map_err(|error| format!("malformed project contract file: {error}"))?;
    if file.schema_version != 1 || file.models.is_empty() {
        return Err(
            "project contract file requires schema_version 1 and at least one model".into(),
        );
    }
    for model in &mut file.models {
        model.distribution = publication::normalized_distribution(&model.distribution)
            .ok_or("invalid project contract distribution name")?;
    }
    validate_project_models(&file.models, target)?;
    Ok(file.models)
}

pub fn validate_project_models(
    models: &[ProjectResourceModel],
    target: &str,
) -> Result<(), String> {
    let symbol = |value: &str| {
        value.contains('.')
            && value
                .split('.')
                .all(|name| crate::is_python_identifier(name, target))
    };
    let mut factories = BTreeSet::new();
    let mut resources = BTreeMap::new();
    for model in models {
        if publication::normalized_distribution(&model.distribution).as_deref()
            != Some(&model.distribution)
            || !publication::version_is_literal(&model.version)
            || !symbol(&model.factory)
            || !symbol(&model.resource)
            || !crate::is_python_identifier(&model.close_method, target)
            || model.provenance.trim().is_empty()
            || model.provenance.len() > 4096
        {
            return Err(format!("malformed project contract for {}", model.factory));
        }
        if !factories.insert((&model.distribution, &model.version, &model.factory)) {
            return Err(format!(
                "conflicting project contracts for {}",
                model.factory
            ));
        }
        let subject = (&model.distribution, &model.version, &model.resource);
        let effect = (&model.close_method, model.exceptional_close);
        if resources
            .insert(subject, effect)
            .is_some_and(|previous| previous != effect)
        {
            return Err(format!(
                "conflicting project cleanup contracts for {}",
                model.resource
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProviderIssue {
    pub import_scope: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProviderOrigin {
    pub public_symbol: String,
    pub canonical_symbol: String,
    pub distribution: String,
    pub version: String,
    pub artifact_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Span>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_origin: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProviderSelection {
    pub import_scope: String,
    pub distribution: String,
    pub version: String,
    pub artifact_sha256: String,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ApplicableFactory {
    pub model: ProjectResourceModel,
    pub origin: ProviderOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PreparedFoundations {
    pub runtime: Option<RuntimeProfile>,
    pub providers: BTreeMap<String, ProviderSelection>,
    importable_children: BTreeSet<String>,
    pub exports: BTreeMap<String, publication::PublishedSymbol>,
    pub unavailable: BTreeMap<String, String>,
    pub factories: BTreeMap<String, ApplicableFactory>,
    pub issues: Vec<ProviderIssue>,
    pub lock_digest: Option<String>,
    pub project_models_digest: Option<String>,
}

impl PreparedFoundations {
    pub(crate) fn new(inputs: &FoundationInputs) -> Self {
        Self {
            runtime: inputs.runtime.clone(),
            providers: BTreeMap::new(),
            importable_children: BTreeSet::new(),
            exports: BTreeMap::new(),
            unavailable: BTreeMap::new(),
            factories: BTreeMap::new(),
            issues: inputs.provider_issues.clone(),
            lock_digest: inputs
                .uv_lock
                .as_ref()
                .map(|lock| publication::sha256(lock.as_bytes())),
            project_models_digest: (!inputs.project_models.is_empty()).then(|| {
                publication::sha256(
                    &serde_json::to_vec(&inputs.project_models).expect("project model JSON"),
                )
            }),
        }
    }

    pub(crate) fn load(
        &mut self,
        selected: &publication::SelectedPublication,
        inputs: &FoundationInputs,
        target: &str,
    ) {
        let package = &selected.package;
        let provider = ProviderSelection {
            import_scope: package.import_scope.clone(),
            distribution: package.distribution.clone(),
            version: package.version.clone(),
            artifact_sha256: selected.digest.clone(),
            origin: selected.origin.into(),
            reason: selected.reason.clone(),
        };
        for file in &package.sources {
            self.providers.insert(file.module.clone(), provider.clone());
        }
        self.exports.extend(selected.exports.clone());
        self.importable_children
            .extend(selected.importable_children.iter().cloned());
        self.unavailable.extend(selected.unavailable.clone());
        let lock = inputs.uv_lock.as_deref().map(LockVersions::parse);
        let validation = validate_project_models(&inputs.project_models, target);
        for model in &inputs.project_models {
            if !model
                .factory
                .starts_with(&format!("{}.", package.import_scope))
            {
                continue;
            }
            let outcome = (|| {
                validation.as_ref().map_err(Clone::clone)?;
                if model.distribution != package.distribution || model.version != package.version {
                    return Err(format!(
                        "project model expects {}=={} but the selected provider is {}=={}",
                        model.distribution, model.version, package.distribution, package.version
                    ));
                }
                if let Some(lock) = &lock {
                    lock.as_ref()
                        .map_err(Clone::clone)?
                        .verify(&package.distribution, &package.version)?;
                }
                let definition = selected.source_exports.get(&model.factory)
                    .filter(|definition| definition.symbol_kind == "function")
                    .ok_or("project model factory does not resolve to a synchronous public function in this artifact")?;
                let class = selected.source_exports.values().any(|definition| {
                    definition.canonical_name == model.resource
                        && definition.symbol_kind == "resource"
                });
                if !class {
                    return Err(
                        "project model resource does not resolve to a class in this artifact"
                            .into(),
                    );
                }
                if !selected
                    .cleanup_methods
                    .get(&model.resource)
                    .is_some_and(|methods| methods.contains(&model.close_method))
                {
                    return Err(format!("project model cleanup method {}.{} does not resolve to a supported synchronous instance method", model.resource, model.close_method));
                }
                let existing = self.exports.get(&model.factory).and_then(|symbol| {
                    symbol
                        .contracts
                        .iter()
                        .find(|fact| fact.kind == "owned-return")
                });
                if existing.is_some() && !model.override_publication {
                    if existing.is_some_and(|fact| {
                        fact.resource.as_ref() != Some(&model.resource)
                            || fact.close_method.as_ref() != Some(&model.close_method)
                    }) {
                        return Err(
                            "project model conflicts with the published factory effect".into()
                        );
                    }
                    return Ok(());
                }
                if model.override_publication && existing.is_none() {
                    return Err("project model override has no applicable published factory effect to replace".into());
                }
                let origin = ProviderOrigin {
                    public_symbol: model.factory.clone(),
                    canonical_symbol: definition.canonical_name.clone(),
                    distribution: package.distribution.clone(),
                    version: package.version.clone(),
                    artifact_sha256: selected.digest.clone(),
                    location: existing.map(|fact| provider_location(package, &fact.evidence)),
                    replaced_origin: existing.map(|_| selected.origin.to_owned()),
                };
                self.factories.insert(
                    model.factory.clone(),
                    ApplicableFactory {
                        model: model.clone(),
                        origin,
                    },
                );
                Ok(())
            })();
            if let Err(reason) = outcome {
                self.unavailable.insert(model.factory.clone(), reason);
            }
        }
    }

    pub(crate) fn module(&self, public_symbol: &str) -> Option<&ProviderSelection> {
        self.providers.get(public_symbol.rsplit_once('.')?.0)
    }

    pub(crate) fn issue_at(&self, module: &str) -> Option<&str> {
        self.providers
            .get(module)
            .and_then(|provider| provider.reason.as_deref())
            .or_else(|| {
                self.issues
                    .iter()
                    .find(|issue| {
                        module == issue.import_scope
                            || module
                                .strip_prefix(&issue.import_scope)
                                .is_some_and(|tail| tail.starts_with('.'))
                    })
                    .map(|issue| issue.reason.as_str())
            })
    }

    pub(crate) fn for_modules(
        &self,
        modules: &BTreeSet<String>,
        imported: &BTreeSet<&str>,
    ) -> Self {
        let relevant = |public: &str| {
            public
                .rsplit_once('.')
                .is_some_and(|(module, _)| modules.contains(module))
        };
        Self {
            runtime: self.runtime.clone(),
            importable_children: self
                .importable_children
                .intersection(modules)
                .cloned()
                .collect(),
            providers: self
                .providers
                .iter()
                .filter(|(module, _)| modules.contains(*module))
                .map(|(module, value)| (module.clone(), value.clone()))
                .collect(),
            exports: self
                .exports
                .iter()
                .filter(|(public, _)| relevant(public))
                .map(|(public, value)| (public.clone(), value.clone()))
                .collect(),
            unavailable: self
                .unavailable
                .iter()
                .filter(|(public, _)| relevant(public))
                .map(|(public, value)| (public.clone(), value.clone()))
                .collect(),
            factories: self
                .factories
                .iter()
                .filter(|(public, _)| relevant(public))
                .map(|(public, value)| (public.clone(), value.clone()))
                .collect(),
            // Discovery issues include packages that cannot be represented by
            // an analyzed source file. Direct unresolved imports need them.
            issues: self
                .issues
                .iter()
                .filter(|issue| {
                    imported.iter().any(|module| {
                        *module == issue.import_scope
                            || module
                                .strip_prefix(&issue.import_scope)
                                .is_some_and(|tail| tail.starts_with('.'))
                    })
                })
                .cloned()
                .collect(),
            lock_digest: None,
            project_models_digest: None,
        }
    }

    pub(crate) fn submodule_imports(&self, summary: &ParseSummary) -> BTreeSet<String> {
        if self.importable_children.is_empty() {
            return BTreeSet::new();
        }
        summary
            .declarations
            .iter()
            .flatten()
            .filter_map(|declaration| match declaration {
                Declaration::ImportFrom {
                    module,
                    level: 0,
                    names,
                    ..
                } if self.providers.contains_key(module) => Some((module, names)),
                _ => None,
            })
            .flat_map(|(module, names)| {
                names
                    .iter()
                    .map(move |name| format!("{module}.{}", name.name))
            })
            .filter(|module| self.importable_children.contains(module))
            .collect()
    }
}

pub(crate) fn normalize_submodule_imports(
    summary: &mut ParseSummary,
    submodules: &BTreeSet<String>,
) {
    if submodules.is_empty() {
        return;
    }
    let Some(declarations) = &mut summary.declarations else {
        return;
    };
    let mut normalized = Vec::with_capacity(declarations.len());
    for declaration in std::mem::take(declarations) {
        match declaration {
            Declaration::ImportFrom {
                module,
                level: 0,
                names,
                range,
            } if names
                .iter()
                .any(|name| submodules.contains(&format!("{module}.{}", name.name))) =>
            {
                for mut name in names {
                    let child = format!("{module}.{}", name.name);
                    if submodules.contains(&child) {
                        name.alias.get_or_insert(name.name);
                        name.name = child;
                        normalized.push(Declaration::Import {
                            names: vec![name],
                            range,
                        });
                    } else {
                        normalized.push(Declaration::ImportFrom {
                            module: module.clone(),
                            level: 0,
                            names: vec![name],
                            range,
                        });
                    }
                }
            }
            declaration => normalized.push(declaration),
        }
    }
    *declarations = normalized;
    summary.absolute_imports.extend(submodules.iter().cloned());
}

pub(crate) fn provider_location(
    package: &ProviderPackage,
    evidence: &publication::PublicationLocation,
) -> Span {
    Span {
        file: format!(
            "@dependency/{}@{}/{}",
            package.distribution, package.version, evidence.path
        ),
        line: evidence.line,
        column: 1,
        end_line: evidence.line,
        end_column: 1,
    }
}

pub(crate) fn publication_origin(
    provider: &ProviderSelection,
    symbol: &publication::PublishedSymbol,
    fact: &publication::PublishedContract,
) -> ProviderOrigin {
    ProviderOrigin {
        public_symbol: symbol.public_name.clone(),
        canonical_symbol: symbol.canonical_name.clone(),
        distribution: provider.distribution.clone(),
        version: provider.version.clone(),
        artifact_sha256: provider.artifact_sha256.clone(),
        location: Some(Span {
            file: format!(
                "@dependency/{}@{}/{}",
                provider.distribution, provider.version, fact.evidence.path
            ),
            line: fact.evidence.line,
            column: 1,
            end_line: fact.evidence.line,
            end_column: 1,
        }),
        replaced_origin: None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedSource {
    pub path: String,
    pub module: String,
    pub sha256: String,
    pub supplied_imports: Vec<String>,
    pub recognized_imports: Vec<String>,
    pub unresolved_imports: Vec<String>,
}

pub(crate) fn scope_entry(
    file: &SourceFile,
    imports: Vec<String>,
    selected: &BTreeSet<String>,
) -> ScopedSource {
    let (supplied_imports, external): (Vec<_>, Vec<_>) = imports
        .into_iter()
        .partition(|module| selected.contains(module));
    let (recognized_imports, unresolved_imports) = external.into_iter().partition(|module| {
        crate::semantic::is_known_external_module(module)
            || matches!(
                module.as_str(),
                "__future__"
                    | "codepact"
                    | "codepact.contracts"
                    | "codepact.rust"
                    | "codepact.outcomes"
                    | "contextlib"
                    | "asyncio"
                    | "multiprocessing"
                    | "signal"
                    | "concurrent.futures"
            )
    });
    ScopedSource {
        path: file.path.clone(),
        module: file.module.clone(),
        sha256: publication::sha256(file.source.as_bytes()),
        supplied_imports,
        recognized_imports,
        unresolved_imports,
    }
}

struct LockVersions(BTreeMap<String, Vec<Option<String>>>);

impl LockVersions {
    fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > publication::MAX_METADATA_BYTES {
            return Err("uv.lock exceeds the metadata byte limit".into());
        }
        let lock: toml::Value =
            toml::from_str(raw).map_err(|error| format!("cannot read uv.lock: {error}"))?;
        if lock.get("version").and_then(toml::Value::as_integer) != Some(1)
            || lock.get("revision").and_then(toml::Value::as_integer) != Some(3)
        {
            return Err("unsupported uv.lock version or revision".into());
        }
        let packages = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("uv.lock has no package inventory")?;
        let mut names = BTreeMap::<String, Vec<Option<String>>>::new();
        for package in packages {
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .and_then(publication::normalized_distribution)
                .ok_or("uv.lock contains an invalid package name")?;
            let supported = package.get("resolution-markers").is_none()
                && package
                    .get("source")
                    .and_then(toml::Value::as_table)
                    .is_some_and(|source| {
                        source.len() == 1
                            && source
                                .get("registry")
                                .and_then(toml::Value::as_str)
                                .is_some()
                    });
            let version = supported
                .then(|| {
                    package
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                })
                .flatten();
            names.entry(name).or_default().push(version);
        }
        Ok(Self(names))
    }

    fn verify(&self, distribution: &str, installed: &str) -> Result<(), String> {
        match self.0.get(distribution).map(Vec::as_slice) {
            Some([Some(locked)]) if locked == installed => Ok(()),
            Some([Some(locked)]) => Err(format!(
                "uv.lock version {locked} disagrees with installed {distribution}=={installed}"
            )),
            None => Err(format!("uv.lock has no entry for installed {distribution}")),
            _ => Err(format!(
                "uv.lock entry for {distribution} is ambiguous or conditional"
            )),
        }
    }
}

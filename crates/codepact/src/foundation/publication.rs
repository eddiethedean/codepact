use crate::{
    frontend::{AstExprKind, Declaration, SourceRange},
    SourceFile, Span,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MANIFEST_FILENAME: &str = "codepact.contracts.json";
pub const MARKER_FILENAME: &str = "py.codepact";
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MANIFEST_SCHEMA_VERSION: u8 = 1;

/// The selected regular package from an installed distribution, with paths
/// relative to the import root. Hosts establish file ownership and read files.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderPackage {
    pub distribution: String,
    pub version: String,
    pub import_scope: String,
    pub sources: Vec<SourceFile>,
    pub marked: bool,
    pub manifest: Option<String>,
}

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
pub struct ArtifactFile {
    pub path: String,
    pub sha256: String,
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

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn normalized_distribution(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return None;
    }
    let mut output = String::new();
    for byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                output.push(byte.to_ascii_lowercase().into())
            }
            b'-' | b'_' | b'.' if !output.ends_with('-') => output.push('-'),
            b'-' | b'_' | b'.' => {}
            _ => return None,
        }
    }
    Some(output)
}

pub fn regular_relative_path(path: &str) -> bool {
    !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|component| !matches!(component, "" | "." | ".."))
}

pub(super) fn version_is_literal(version: &str) -> bool {
    version.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && version.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'!' | b'+' | b'-' | b'_')
        })
}

fn qualified(name: &str, target: &str) -> bool {
    !name.is_empty()
        && name
            .split('.')
            .all(|part| crate::is_python_identifier(part, target))
}

fn package_path(scope: &str) -> String {
    scope.replace('.', "/")
}

impl ProviderPackage {
    pub fn marker_path(&self) -> String {
        format!("{}/{}", package_path(&self.import_scope), MARKER_FILENAME)
    }

    pub fn manifest_path(&self) -> String {
        format!("{}/{}", package_path(&self.import_scope), MANIFEST_FILENAME)
    }

    pub(crate) fn validate(&self, target: &str) -> Result<(), String> {
        if normalized_distribution(&self.distribution).as_deref() != Some(&self.distribution)
            || !version_is_literal(&self.version)
            || !qualified(&self.import_scope, target)
        {
            return Err("invalid provider distribution, exact version or import scope".into());
        }
        let mut paths = BTreeSet::new();
        let mut modules = BTreeSet::new();
        let prefix = format!("{}/", package_path(&self.import_scope));
        if self.sources.len() > crate::DEFAULT_MAX_INPUT_FILES
            || self
                .sources
                .iter()
                .map(|file| file.source.len())
                .sum::<usize>()
                > crate::DEFAULT_MAX_SOURCE_BYTES
        {
            return Err("provider source discovery exceeded the file or byte budget".into());
        }
        for file in &self.sources {
            if !regular_relative_path(&file.path)
                || !file.path.starts_with(&prefix)
                || module_for_source(&file.path, target).as_deref() != Some(&file.module)
                || !paths.insert(&file.path)
                || !modules.insert(&file.module)
            {
                return Err(
                    "provider sources contain an invalid or duplicate path/module identity".into(),
                );
            }
        }
        if !paths.contains(&format!("{prefix}__init__.py")) {
            return Err("the provider import scope must be a regular Python package".into());
        }
        for file in &self.sources {
            if file
                .path
                .strip_prefix(&prefix)
                .unwrap()
                .rsplit_once('/')
                .is_some_and(|(subdir, _)| {
                    let mut package = prefix.clone();
                    subdir.split('/').any(|component| {
                        package.push_str(component);
                        package.push('/');
                        !paths.contains(&format!("{package}__init__.py"))
                    })
                })
            {
                return Err("provider source reaches an unsupported namespace package".into());
            }
        }
        Ok(())
    }

    pub(crate) fn sources_for_analysis(&self) -> Vec<SourceFile> {
        self.sources
            .iter()
            .map(|file| {
                SourceFile::new(
                    format!(
                        "@dependency/{}@{}/{}",
                        self.distribution, self.version, file.path
                    ),
                    &file.module,
                    &file.source,
                )
            })
            .collect()
    }

    fn bindings(&self) -> Vec<ArtifactFile> {
        let mut files = self
            .sources
            .iter()
            .map(|file| ArtifactFile {
                path: file.path.clone(),
                sha256: sha256(file.source.as_bytes()),
            })
            .collect::<Vec<_>>();
        if self.marked {
            files.push(ArtifactFile {
                path: self.marker_path(),
                sha256: sha256(b""),
            });
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        files
    }

    pub(crate) fn digest(&self) -> String {
        sha256(
            &serde_json::to_vec(&(
                &self.distribution,
                &self.version,
                &self.import_scope,
                self.bindings(),
                &self.manifest,
            ))
            .expect("provider digest inputs are JSON values"),
        )
    }
}

pub fn module_for_source(path: &str, target: &str) -> Option<String> {
    if !regular_relative_path(path) {
        return None;
    }
    let without_suffix = path.strip_suffix(".py")?;
    let path = without_suffix
        .strip_suffix("/__init__")
        .unwrap_or(without_suffix);
    path.split('/')
        .all(|name| crate::is_python_identifier(name, target))
        .then(|| path.replace('/', "."))
}

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

/// Export only supported declarations. This derives the export surface and
/// source evidence from staged inputs without evaluating Python.
pub fn export_manifest(package: &ProviderPackage, target: &str) -> Result<Manifest, String> {
    if !package.marked {
        return Err("contract export requires an empty py.codepact marker".into());
    }
    package.validate(target)?;
    let source = source_publication(package, target)?;
    Ok(Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        producer_version: env!("CARGO_PKG_VERSION").into(),
        distribution: package.distribution.clone(),
        version: package.version.clone(),
        import_scope: package.import_scope.clone(),
        python_version: target.into(),
        files: package.bindings(),
        symbols: source
            .exports
            .into_values()
            .filter(|symbol| !symbol.contracts.is_empty())
            .collect(),
    })
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
            return Err(
                "contract manifest Python target is incompatible with this analysis".into(),
            );
        }
        if manifest.files != package.bindings() {
            return Err(
                "stale contract manifest: packaged files or SHA-256 digests do not match".into(),
            );
        }
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
                    selected.unavailable.insert(
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
                    selected.unavailable.insert(
                        symbol.public_name.clone(),
                        format!(
                            "published effect for {} has no matching checked source declaration",
                            symbol.public_name
                        ),
                    );
                }
            }
            symbol.contracts = valid;
            selected.exports.insert(symbol.public_name.clone(), symbol);
        }
        Ok(())
    })();
    if let Err(reason) = result {
        selected.reason = Some(reason);
        selected.exports.clear();
        selected.unavailable.clear();
    }
    selected
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

#[derive(Clone)]
enum Binding {
    Definition {
        range: SourceRange,
        is_resource: bool,
    },
    Reexport {
        module: String,
        name: String,
        explicit: bool,
    },
}

struct SourcePublication {
    exports: BTreeMap<String, PublishedSymbol>,
    importable_children: BTreeSet<String>,
    cleanup_methods: BTreeMap<String, BTreeSet<String>>,
}

fn source_publication(
    package: &ProviderPackage,
    target: &str,
) -> Result<SourcePublication, String> {
    let files = package.sources_for_analysis();
    let crate::semantic::PublicationInspection {
        inspection,
        must_use,
        declarations: parsed,
    } = crate::semantic::publication_inspection(&files, target)?;
    let mut bindings = BTreeMap::<String, BTreeMap<String, Binding>>::new();
    let mut all = BTreeMap::<String, Option<BTreeSet<String>>>::new();
    let mut cleanup_methods = BTreeMap::new();
    for file in &files {
        let mut names = BTreeMap::new();
        for declaration in &parsed[&file.module] {
            match declaration {
                Declaration::Function {
                    name,
                    range,
                    is_async: false,
                    is_generator: false,
                    unsupported: false,
                    decorators,
                    must_use_decorator,
                    ..
                } if decorators.is_empty() || *must_use_decorator => {
                    names.insert(
                        name.clone(),
                        Binding::Definition {
                            range: *range,
                            is_resource: false,
                        },
                    );
                }
                Declaration::Class {
                    name,
                    range,
                    has_unresolved_base_classes: false,
                    methods,
                    bound_methods,
                    unsupported,
                    ..
                } => {
                    let latest = methods
                        .iter()
                        .map(|method| (&method.name, method))
                        .collect::<BTreeMap<_, _>>();
                    cleanup_methods.insert(
                        format!("{}.{}", file.module, name),
                        latest
                            .into_values()
                            .filter(|method| {
                                !unsupported
                                    && bound_methods.contains(&method.name)
                                    && !method.decorated
                                    && !method.is_async
                                    && !method.is_generator
                                    && method.parameters.len() == 1
                                    && method.signature.positional_only.len()
                                        + method.signature.positional_or_keyword.len()
                                        == 1
                            })
                            .map(|method| method.name.clone())
                            .collect(),
                    );
                    names.insert(
                        name.clone(),
                        Binding::Definition {
                            range: *range,
                            is_resource: true,
                        },
                    );
                }
                Declaration::ImportFrom {
                    module,
                    level: 0,
                    names: imports,
                    ..
                } => {
                    for import in imports {
                        if import.name == "*" {
                            names.clear();
                            all.insert(file.module.clone(), None);
                            continue;
                        }
                        let local = import.alias.as_deref().unwrap_or(&import.name);
                        names.insert(
                            local.to_owned(),
                            Binding::Reexport {
                                module: module.clone(),
                                name: import.name.clone(),
                                explicit: import.alias.is_some(),
                            },
                        );
                    }
                }
                Declaration::Assignment {
                    name: Some(name),
                    value,
                    ..
                } if name == "__all__" => {
                    let selected = value.as_ref().and_then(|value| match &value.kind {
                        AstExprKind::Tuple(names) | AstExprKind::Collection(names) => names
                            .iter()
                            .map(|name| match &name.kind {
                                AstExprKind::String(name) => Some(name.clone()),
                                _ => None,
                            })
                            .collect(),
                        _ => None,
                    });
                    all.insert(file.module.clone(), selected);
                }
                Declaration::Assignment {
                    name: Some(name), ..
                }
                | Declaration::TypeAlias {
                    name: Some(name), ..
                }
                | Declaration::Class { name, .. }
                | Declaration::Function { name, .. } => {
                    names.remove(name);
                }
                Declaration::Delete { names: deleted, .. } => {
                    for name in deleted.iter().flatten() {
                        names.remove(name);
                        if name == "__all__" {
                            all.insert(file.module.clone(), None);
                        }
                    }
                }
                Declaration::Import { names: imports, .. } => {
                    for import in imports {
                        names.remove(
                            import.alias.as_deref().unwrap_or_else(|| {
                                import.name.split('.').next().unwrap_or_default()
                            }),
                        );
                    }
                }
                Declaration::Unsupported { .. } | Declaration::ModuleFlow { .. } => {
                    names.clear();
                    all.insert(file.module.clone(), None);
                }
                _ => {}
            }
        }
        bindings.insert(file.module.clone(), names);
    }
    let by_module = files
        .iter()
        .map(|file| (file.module.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut exports = BTreeMap::new();
    for (public_module, names) in &bindings {
        if public_module
            .strip_prefix(&package.import_scope)
            .is_some_and(|tail| tail.split('.').any(|part| part.starts_with('_')))
        {
            continue;
        }
        for (public_name, binding) in names {
            let visible = all.get(public_module).map_or_else(
                || {
                    !public_name.starts_with('_')
                        && !matches!(
                            binding,
                            Binding::Reexport {
                                explicit: false,
                                ..
                            }
                        )
                },
                |all| {
                    all.as_ref()
                        .is_some_and(|names| names.contains(public_name))
                },
            );
            if !visible {
                continue;
            }
            let mut module = public_module.as_str();
            let mut name = public_name.as_str();
            let mut seen = BTreeSet::new();
            while seen.len() < 32 && seen.insert((module, name)) {
                match bindings.get(module).and_then(|names| names.get(name)) {
                    Some(Binding::Reexport {
                        module: next,
                        name: exported,
                        ..
                    }) => {
                        module = next;
                        name = exported;
                    }
                    Some(Binding::Definition { range, is_resource }) => {
                        let file = by_module[module];
                        let site = crate::source_span(&file.path, &file.source, *range);
                        let canonical = format!("{module}.{name}");
                        let mut contracts = source_facts(
                            file,
                            name,
                            *is_resource,
                            &canonical,
                            site,
                            &inspection,
                            &must_use,
                        );
                        contracts.sort_by(|left, right| {
                            (&left.kind, &left.subject).cmp(&(&right.kind, &right.subject))
                        });
                        let public = format!("{public_module}.{public_name}");
                        exports.insert(
                            public.clone(),
                            PublishedSymbol {
                                public_name: public,
                                canonical_name: canonical,
                                symbol_kind: if *is_resource { "resource" } else { "function" }
                                    .into(),
                                contracts,
                            },
                        );
                        break;
                    }
                    None => break,
                }
            }
        }
    }
    Ok(SourcePublication {
        exports,
        importable_children: importable_children(&files, &parsed),
        cleanup_methods,
    })
}

fn importable_children(
    files: &[SourceFile],
    parsed: &BTreeMap<String, Vec<Declaration>>,
) -> BTreeSet<String> {
    // A from-import asks for the child module only when the parent does not
    // supply that attribute. An unknown initializer can supply any attribute.
    let mut parent_bindings = BTreeMap::new();
    for file in files
        .iter()
        .filter(|file| file.path.ends_with("/__init__.py"))
    {
        let mut names = BTreeSet::new();
        let mut known = true;
        for declaration in &parsed[&file.module] {
            match declaration {
                Declaration::Function { name, .. }
                | Declaration::Class { name, .. }
                | Declaration::Assignment {
                    name: Some(name), ..
                }
                | Declaration::TypeAlias {
                    name: Some(name), ..
                } => {
                    names.insert(name.as_str());
                }
                Declaration::Import { names: imports, .. } => {
                    for import in imports {
                        names.insert(import.alias.as_deref().unwrap_or_else(|| {
                            import.name.split('.').next().unwrap_or(&import.name)
                        }));
                    }
                }
                Declaration::ImportFrom { names: imports, .. } => {
                    for import in imports {
                        names.insert(import.alias.as_deref().unwrap_or(&import.name));
                    }
                }
                Declaration::Expression { value, .. }
                    if matches!(value.kind, AstExprKind::String(_)) => {}
                Declaration::Delete { .. } => {}
                _ => known = false,
            }
        }
        if known && !names.contains("*") && !names.contains("__getattr__") {
            parent_bindings.insert(file.module.as_str(), names);
        }
    }
    files
        .iter()
        .filter_map(|file| {
            let (parent, child) = file.module.rsplit_once('.')?;
            let names = parent_bindings.get(parent)?;
            (!names.contains(child)).then(|| file.module.clone())
        })
        .collect()
}

fn source_facts(
    file: &SourceFile,
    name: &str,
    is_resource: bool,
    canonical: &str,
    site: Span,
    inspection: &crate::Inspection,
    must_use: &BTreeMap<String, Span>,
) -> Vec<PublishedContract> {
    let path = file
        .path
        .strip_prefix(crate::DEPENDENCY_SOURCE_PREFIX)
        .and_then(|path| path.split_once('/'))
        .map(|(_, path)| path)
        .expect("provider logical path");
    let mut facts = Vec::new();
    if !is_resource && must_use.get(canonical).is_some_and(|span| span == &site) {
        facts.push(PublishedContract {
            kind: "must-use".into(),
            version: 1,
            subject: "return".into(),
            resource: None,
            close_method: None,
            required_capabilities: vec!["synchronous-result-use".into()],
            evidence: PublicationLocation {
                path: path.into(),
                line: site.line,
            },
        });
    }
    for contract in inspection
        .contracts
        .iter()
        .filter(|contract| contract.source.file == file.path)
    {
        let Some(lifecycle) = &contract.lifecycle else {
            continue;
        };
        let (kind, subject, capability) = if is_resource
            && contract.display_name == name
            && contract.base_symbol == canonical
            && contract.source.line <= site.line
        {
            ("resource", "receiver", "synchronous-resource-v1")
        } else if !is_resource
            && contract.access == "owned"
            && (site.line..=site.end_line).contains(&contract.source.line)
        {
            let Some(subject) = contract
                .display_name
                .strip_prefix(name)
                .and_then(|tail| tail.strip_prefix('.'))
            else {
                continue;
            };
            (
                if subject == "return" {
                    "owned-return"
                } else {
                    "owned-parameter"
                },
                subject,
                "synchronous-ownership-v1",
            )
        } else {
            continue;
        };
        facts.push(PublishedContract {
            kind: kind.into(),
            version: 1,
            subject: subject.into(),
            resource: Some(contract.base_symbol.clone()),
            close_method: Some(lifecycle.closed_by.clone()),
            required_capabilities: vec![capability.into()],
            evidence: PublicationLocation {
                path: path.into(),
                line: contract.source.line,
            },
        });
    }
    facts
}

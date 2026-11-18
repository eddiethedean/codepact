//! Source export discovery and supported facts for regular provider packages.

use super::{ProviderPackage, PublicationLocation, PublishedContract, PublishedSymbol};
use crate::{
    flow::outcomes::{scan_declarations, OutcomeContract},
    frontend::{AstExprKind, Declaration, SourceRange},
    SourceFile, Span,
};
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) struct SourcePublication {
    pub(super) exports: BTreeMap<String, PublishedSymbol>,
    pub(super) importable_children: BTreeSet<String>,
    pub(super) cleanup_methods: BTreeMap<String, BTreeSet<String>>,
}

pub(super) fn source_publication(
    package: &ProviderPackage,
    target: &str,
) -> Result<SourcePublication, String> {
    let files = package.sources_for_analysis();
    let crate::semantic::PublicationInspection {
        inspection,
        must_use,
        declarations: parsed,
    } = crate::semantic::publication_inspection(&files, target)?;
    let mut cleanup_methods = BTreeMap::new();
    let supplied_modules = files.iter().map(|file| file.module.clone()).collect();
    let outcome_declarations = parsed
        .iter()
        .map(|(module, declarations)| {
            (
                module.as_str(),
                scan_declarations(declarations, &supplied_modules).returns,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let bindings = files
        .iter()
        .map(|file| {
            let bindings = bind_module(
                &file.module,
                &parsed[&file.module],
                &outcome_declarations[&*file.module],
                &mut cleanup_methods,
            );
            (file.module.as_str(), bindings)
        })
        .collect::<BTreeMap<_, _>>();
    let by_module = files
        .iter()
        .map(|file| (file.module.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut exports = BTreeMap::new();
    for (public_module, module) in &bindings {
        if public_module
            .strip_prefix(&package.import_scope)
            .is_some_and(|tail| tail.split('.').any(|part| part.starts_with('_')))
        {
            continue;
        }
        for (public_name, binding) in &module.names {
            if !module.visibility.contains(public_name, binding) {
                continue;
            }
            let Some(definition) = resolve_definition(&bindings, public_module, public_name) else {
                continue;
            };
            let file = by_module[definition.module];
            let site = crate::source_span(&file.path, &file.source, definition.range);
            let canonical = format!("{}.{}", definition.module, definition.name);
            let mut contracts = source_facts(
                file,
                definition.name,
                definition.is_resource,
                &canonical,
                site,
                &inspection,
                &must_use,
            );
            if let Some(outcome) =
                outcome_declarations[definition.module].get(&definition.range.start)
            {
                let path = file
                    .path
                    .strip_prefix(crate::DEPENDENCY_SOURCE_PREFIX)
                    .and_then(|path| path.split_once('/'))
                    .map(|(_, path)| path)
                    .expect("provider logical path");
                contracts.push(PublishedContract {
                    kind: "must-handle-return".into(),
                    version: 1,
                    subject: "return".into(),
                    resource: None,
                    close_method: None,
                    required_capabilities: vec![outcome.capability().into()],
                    evidence: PublicationLocation {
                        path: path.into(),
                        line: crate::source_span(&file.path, &file.source, definition.range).line,
                    },
                });
            }
            contracts.sort_by(|left, right| {
                (&left.kind, &left.subject).cmp(&(&right.kind, &right.subject))
            });
            let public = format!("{public_module}.{public_name}");
            exports.insert(
                public.clone(),
                PublishedSymbol {
                    public_name: public,
                    canonical_name: canonical,
                    symbol_kind: if definition.is_resource {
                        "resource"
                    } else {
                        "function"
                    }
                    .into(),
                    contracts,
                },
            );
        }
    }
    Ok(SourcePublication {
        exports,
        importable_children: importable_children(&files, &parsed),
        cleanup_methods,
    })
}

enum ExportList {
    Implicit,
    Explicit(BTreeSet<String>),
    Unknown,
}

impl ExportList {
    fn contains(&self, name: &str, binding: &Binding) -> bool {
        match self {
            Self::Implicit => {
                !name.starts_with('_')
                    && !matches!(
                        binding,
                        Binding::Reexport {
                            explicit: false,
                            ..
                        }
                    )
            }
            Self::Explicit(names) => names.contains(name),
            Self::Unknown => false,
        }
    }
}

struct ModuleBindings {
    names: BTreeMap<String, Binding>,
    visibility: ExportList,
}

/// Interpret a module's declaration order to find its final binding and export list.
fn bind_module(
    module: &str,
    declarations: &[Declaration],
    outcomes: &BTreeMap<usize, OutcomeContract>,
    cleanup_methods: &mut BTreeMap<String, BTreeSet<String>>,
) -> ModuleBindings {
    let mut names = BTreeMap::new();
    let mut visibility = ExportList::Implicit;
    for declaration in declarations {
        match declaration {
            Declaration::Function {
                name,
                range,
                is_async,
                is_generator: false,
                unsupported: false,
                decorators,
                must_use_decorator,
                ..
            } if (!is_async || outcomes.contains_key(&range.start))
                && (decorators.is_empty() || *must_use_decorator) =>
            {
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
                    format!("{module}.{name}"),
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
                        visibility = ExportList::Unknown;
                        continue;
                    }
                    let local = import.member_binding();
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
                visibility = selected.map_or(ExportList::Unknown, ExportList::Explicit);
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
                        visibility = ExportList::Unknown;
                    }
                }
            }
            Declaration::Import { names: imports, .. } => {
                for import in imports {
                    names.remove(import.import_binding());
                }
            }
            Declaration::Unsupported { .. } | Declaration::ModuleFlow { .. } => {
                names.clear();
                visibility = ExportList::Unknown;
            }
            _ => {}
        }
    }
    ModuleBindings { names, visibility }
}

struct Definition<'a> {
    module: &'a str,
    name: &'a str,
    range: SourceRange,
    is_resource: bool,
}

fn resolve_definition<'a>(
    bindings: &'a BTreeMap<&str, ModuleBindings>,
    mut module: &'a str,
    mut name: &'a str,
) -> Option<Definition<'a>> {
    let mut seen = BTreeSet::new();
    while seen.len() < 32 && seen.insert((module, name)) {
        match bindings.get(module)?.names.get(name)? {
            Binding::Reexport {
                module: next,
                name: exported,
                ..
            } => {
                module = next;
                name = exported;
            }
            Binding::Definition { range, is_resource } => {
                return Some(Definition {
                    module,
                    name,
                    range: *range,
                    is_resource: *is_resource,
                });
            }
        }
    }
    None
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
                        names.insert(import.import_binding());
                    }
                }
                Declaration::ImportFrom { names: imports, .. } => {
                    for import in imports {
                        names.insert(import.member_binding());
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

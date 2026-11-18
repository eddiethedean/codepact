//! Opt-in policy for declarations in the selected source scope.
use super::{DeclarationKind, DeclarationMode, EffectiveConfig};
use crate::frontend::{parse_frontend, Declaration};
use crate::{CheckDiagnostic, CheckResult, Contract, ProviderEvidence, SourceFile, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn revision(
    result: &CheckResult,
    files: &[SourceFile],
    config: &EffectiveConfig,
) -> Option<String> {
    if config.declaration_policy.is_empty() {
        return None;
    }
    let dependency_scope = result.foundations.as_ref().map(|evidence| {
        (
            &evidence.providers,
            &evidence.provider_issues,
            &evidence.lock_sha256,
            &evidence.project_models_sha256,
        )
    });
    let modules = files
        .iter()
        .map(|file| (file.path.as_str(), file.module.as_str()))
        .collect::<BTreeMap<_, _>>();
    let declarations = result
        .contracts
        .iter()
        .filter_map(|contract| {
            let module = modules.get(contract.source.file.as_str())?;
            config
                .declaration_policy
                .iter()
                .any(|entry| {
                    entry.module == *module
                        && entry.function.as_ref().is_none_or(|function| {
                            contract
                                .display_name
                                .strip_prefix(function)
                                .is_some_and(|suffix| suffix.starts_with('.'))
                        })
                })
                .then(|| {
                    (
                        *module,
                        &contract.base_symbol,
                        &contract.display_name,
                        &contract.access,
                        contract
                            .lifecycle
                            .as_ref()
                            .map(|lifecycle| lifecycle.closed_by.as_str()),
                    )
                })
        })
        .collect::<BTreeSet<_>>();
    let bytes = serde_json::to_vec(&(
        2u8,
        &config.declaration_policy,
        &config.include,
        &config.exclude,
        &config.enabled_rules,
        &config.severity,
        &config.required_providers,
        declarations,
        dependency_scope,
    ))
    .expect("declaration-policy revision inputs are serializable");
    Some(super::fingerprint::hash(&bytes))
}

pub(super) fn apply(result: &mut CheckResult, files: &[SourceFile], config: &EffectiveConfig) {
    if config.declaration_policy.is_empty() {
        return;
    }
    let mut diagnostics = Vec::new();
    let mut uncovered = Vec::new();
    let mut matched = BTreeSet::new();
    let codepact_shadowed = files.iter().any(|file| file.module == "codepact");
    for file in files {
        let selected = config
            .declaration_policy
            .iter()
            .filter(|entry| {
                entry.module == file.module
                    && config.enabled_rules.contains(entry.requirement.code())
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            continue;
        }
        for entry in selected.iter().filter(|entry| entry.function.is_none()) {
            matched.insert(*entry);
        }
        let Ok(parsed) = parse_frontend(
            &file.source,
            &file.path,
            &file.module,
            &config.python_version,
        ) else {
            continue;
        };
        for (declaration, (marked, known_decorators)) in
            final_functions(&parsed.declarations, codepact_shadowed).into_values()
        {
            if let Declaration::Function {
                ref name,
                ref parameters,
                is_async,
                is_generator,
                unsupported,
                range,
                ..
            } = *declaration
            {
                let mut requirements = BTreeMap::new();
                for entry in &selected {
                    if let Some(function) = &entry.function {
                        if function == name {
                            matched.insert(*entry);
                            requirements.insert(entry.requirement, entry.mode);
                        }
                    } else if !name.starts_with('_') && !is_async && !is_generator {
                        requirements.entry(entry.requirement).or_insert(entry.mode);
                    }
                }
                if !requirements.is_empty() {
                    let primary = signature_start(file, range);
                    let subject = format!("{}.{name}", file.module);
                    if is_async || is_generator || unsupported || !known_decorators {
                        uncovered.push(format!("{}:{}: declaration policy could not resolve a supported synchronous signature for {subject}", file.path, primary.line));
                    } else {
                        for (kind, mode) in requirements {
                            let contract_for = |label: &str| {
                                result.contracts.iter().find(|contract| {
                                    contract.source.file == file.path
                                        && contract.source.line >= primary.line
                                        && (contract.source.end_line, contract.source.end_column)
                                            <= (primary.end_line, primary.end_column)
                                        && contract.display_name == label
                                        && contract.lifecycle.is_some()
                                })
                            };
                            let mut emit =
                                |parameter: &str,
                                 contract: Option<&Contract>,
                                 message: String,
                                 repair: &str| {
                                    diagnostics.push(finding(
                                        result,
                                        kind,
                                        mode,
                                        &subject,
                                        parameter,
                                        contract,
                                        primary.clone(),
                                        message,
                                        repair,
                                    ));
                                };
                            match kind {
                                DeclarationKind::ResourceAccess => {
                                    for parameter in parameters {
                                        if let Some(contract) =
                                            contract_for(&format!("{name}.{}", parameter.name))
                                                .filter(|contract| contract.access == "unspecified")
                                        {
                                            emit(&parameter.name, Some(contract), format!("{subject} requires explicit resource access for {}", parameter.name), "Declare the parameter Owned, Borrowed, or BorrowedMut according to the callable's contract.");
                                        }
                                    }
                                }
                                DeclarationKind::OwnedReturn => {
                                    let contract = contract_for(&format!("{name}.return"));
                                    if contract.is_none_or(|contract| contract.access != "owned") {
                                        emit("return", contract, format!("{subject} requires an owned resource return declaration"), "Declare the returned resource Owned when transferring cleanup responsibility to the caller.");
                                    }
                                }
                                DeclarationKind::MustUse if !marked => {
                                    emit(
                                        "return",
                                        None,
                                        format!("{subject} requires an immediate-use declaration"),
                                        "Declare @must_use or annotate the return with MustUse.",
                                    );
                                }
                                DeclarationKind::MustUse => {}
                            }
                        }
                    }
                }
            }
        }
    }
    for entry in &config.declaration_policy {
        if config.enabled_rules.contains(entry.requirement.code()) && !matched.contains(entry) {
            let subject = entry.function.as_ref().map_or_else(
                || entry.module.clone(),
                |name| format!("{}.{name}", entry.module),
            );
            uncovered.push(format!(
                "declaration policy selector {subject} is outside the resolved source scope"
            ));
        }
    }
    if !uncovered.is_empty() {
        result.coverage.status = "incomplete".into();
        result.coverage.reasons.extend(uncovered);
        result.coverage.reasons.sort();
        result.coverage.reasons.dedup();
    }
    result.diagnostics.extend(diagnostics);
}

fn signature_start(file: &SourceFile, range: crate::frontend::SourceRange) -> Span {
    // Preserve the full definition bound for matching declaration facts. The
    // diagnostic itself is anchored at the start of that definition.
    crate::source_span(&file.path, &file.source, range)
}

type FunctionBinding<'a> = (&'a Declaration, (bool, bool));

fn final_functions(
    declarations: &[Declaration],
    codepact_shadowed: bool,
) -> BTreeMap<&str, FunctionBinding<'_>> {
    let mut functions = BTreeMap::new();
    let mut imports = BTreeMap::new();
    for declaration in declarations {
        match declaration {
            Declaration::Function {
                name,
                decorators,
                returns,
                ..
            } => {
                let marker = crate::semantic::declaration_policy_must_use(
                    decorators,
                    returns.as_ref(),
                    &imports,
                );
                functions.insert(name.as_str(), (declaration, marker));
                imports.remove(name);
            }
            Declaration::Import { names, .. } => {
                for name in names {
                    let local = name.import_binding();
                    functions.remove(local);
                    imports.remove(local);
                    if name.name == "typing"
                        || (!codepact_shadowed
                            && (name.name == "codepact" || name.name.starts_with("codepact.")))
                    {
                        let identity = if name.alias.is_some() {
                            name.name.replace('.', "::")
                        } else {
                            local.to_owned()
                        };
                        imports.insert(local.to_owned(), identity);
                    }
                }
            }
            Declaration::ImportFrom {
                module,
                level,
                names,
                ..
            } => {
                for name in names {
                    if name.name == "*" {
                        functions.clear();
                        imports.clear();
                        continue;
                    }
                    let local = name.member_binding();
                    functions.remove(local);
                    imports.remove(local);
                    if *level == 0
                        && (module == "typing"
                            || (!codepact_shadowed
                                && matches!(
                                    module.as_str(),
                                    "codepact" | "codepact.contracts" | "codepact.rust"
                                )))
                    {
                        imports.insert(
                            local.to_owned(),
                            format!("{}::{}", module.replace('.', "::"), name.name),
                        );
                    }
                }
            }
            Declaration::Class { name, .. }
            | Declaration::Assignment {
                name: Some(name),
                value: Some(_),
                ..
            }
            | Declaration::TypeAlias {
                name: Some(name), ..
            } => {
                functions.remove(name.as_str());
                imports.remove(name);
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    functions.remove(name.as_str());
                    imports.remove(name);
                }
            }
            Declaration::ModuleFlow { .. } | Declaration::Unsupported { .. } => {
                functions.clear();
                imports.clear();
            }
            _ => {}
        }
    }
    functions
}

#[allow(clippy::too_many_arguments)]
fn finding(
    result: &CheckResult,
    kind: DeclarationKind,
    mode: DeclarationMode,
    subject: &str,
    parameter: &str,
    contract: Option<&Contract>,
    mut primary: Span,
    message: String,
    repair: &str,
) -> CheckDiagnostic {
    primary.end_line = primary.line;
    primary.end_column = primary.column + 3;
    let provider_evidence = contract.and_then(|contract| {
        let base = contract.base_symbol.replace("::", ".");
        let provider = result
            .foundations
            .as_ref()?
            .providers
            .iter()
            .filter(|provider| {
                base.strip_prefix(&provider.import_scope)
                    .is_some_and(|suffix| suffix.starts_with('.'))
            })
            .max_by_key(|provider| provider.import_scope.len())?;
        Some(ProviderEvidence {
            public_symbol: base.clone(),
            canonical_symbol: base,
            distribution: provider.distribution.clone(),
            version: provider.version.clone(),
            origin_kind: provider.origin.clone(),
        })
    });
    let mut identity = vec![subject.to_owned(), parameter.to_owned()];
    if let Some(contract) = contract {
        identity.push(contract.base_symbol.clone());
    }
    CheckDiagnostic {
        semantic_identity: identity,
        provider_evidence,
        code: kind.code().into(),
        severity: match mode {
            DeclarationMode::Preview => "info",
            DeclarationMode::Enforce => "error",
        }
        .into(),
        message,
        primary,
        related: contract
            .map(|contract| vec![contract.source.clone()])
            .unwrap_or_default(),
        certainty: "definite".into(),
        completion: None,
        trust: "static-source".into(),
        coverage_reason: None,
        repair: Some(repair.into()),
    }
}

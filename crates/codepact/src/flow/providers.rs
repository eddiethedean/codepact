use super::*;
use crate::foundation::{self, ApplicableFactory, PreparedFoundations, ProviderOrigin};

#[derive(Clone)]
pub(super) struct BoundFact {
    pub(super) kind: String,
    origin: String,
    provider: ProviderOrigin,
    resource: Option<String>,
}

fn verified_close(fact: &foundation::PublishedContract, definitions: &ProjectFunctionMap) -> bool {
    let Some((module, class)) = fact
        .resource
        .as_deref()
        .and_then(|name| name.rsplit_once('.'))
    else {
        return false;
    };
    let Some(close) = &fact.close_method else {
        return false;
    };
    definitions
        .get(&(module.to_owned(), format!("@{class}.{close}")))
        .is_some_and(|method| {
            method.supported_binding
                && !method.is_async
                && method.parameters.len() == 1
                && method.signature.vararg.is_none()
                && method.signature.kwarg.is_none()
        })
}

pub(super) fn public_functions(
    models: &FlowModelCatalog,
    definitions: &ProjectFunctionMap,
) -> ProjectFunctionMap {
    let Some(inputs) = models.foundations.as_ref() else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for (public, symbol) in &inputs.exports {
        let Some((module, name)) = public.rsplit_once('.') else {
            continue;
        };
        let Some((canonical_module, canonical_name)) = symbol.canonical_name.rsplit_once('.')
        else {
            continue;
        };
        let Some((definition, provider)) = definitions
            .get(&(canonical_module.to_owned(), canonical_name.to_owned()))
            .zip(inputs.providers.get(module))
        else {
            continue;
        };
        let outcome_return = definition.outcome_return.filter(|contract| {
            symbol.contracts.iter().any(|fact| {
                fact.kind == "must-handle-return"
                    && fact
                        .required_capabilities
                        .iter()
                        .any(|capability| capability == contract.capability())
            })
        });
        if symbol.symbol_kind != "function"
            || (definition.is_async && outcome_return.is_none())
            || !definition.supported_binding
        {
            continue;
        }
        if inputs.unavailable.contains_key(public)
            && !symbol.contracts.iter().any(|fact| fact.kind == "must-use")
            && outcome_return.is_none()
            && symbol
                .contracts
                .iter()
                .all(|fact| fact.kind != "owned-parameter")
            && !symbol
                .contracts
                .iter()
                .any(|fact| fact.kind == "owned-return" && verified_close(fact, definitions))
        {
            continue;
        }
        let mut published = definition.clone();
        published.outcome_return = outcome_return;
        published.parameter_contracts.retain(|parameter, _| {
            symbol.contracts.iter().any(|fact| {
                fact.kind == "owned-parameter"
                    && fact.subject == *parameter
                    && verified_close(fact, definitions)
            })
        });
        if published.parameter_contracts.len() != definition.parameter_contracts.len() {
            // A body can rely on entry facts excluded from its public record.
            continue;
        }
        let mut owned_return = false;
        for fact in &symbol.contracts {
            if (fact.kind == "must-handle-return" && outcome_return.is_some())
                || (matches!(fact.kind.as_str(), "owned-return" | "owned-parameter")
                    && verified_close(fact, definitions))
            {
                owned_return |= fact.kind == "owned-return";
                published.publication.push(BoundFact {
                    kind: fact.kind.clone(),
                    origin: provider.origin.clone(),
                    provider: foundation::publication_origin(provider, symbol, fact),
                    resource: fact.resource.clone(),
                });
            }
        }
        if owned_return
            && inputs.unavailable.get(public).is_some_and(|reason| {
                reason == "project model conflicts with the published factory effect"
            })
        {
            continue;
        }
        // Only body effects selected by this public record enter flow. The
        // must-use pass can still apply an independent declared requirement.
        if published.publication.is_empty() {
            continue;
        }
        if !owned_return {
            if published.return_access.is_some() {
                continue;
            }
            published.return_access = None;
        }
        for contract in published.parameter_contracts.values_mut() {
            mark_source_verified(contract);
        }
        result.insert((module.to_owned(), name.to_owned()), published);
    }
    for (public, factory) in &inputs.factories {
        let Some((module, name)) = public.rsplit_once('.') else {
            continue;
        };
        let Some((canonical_module, canonical_name)) =
            factory.origin.canonical_symbol.rsplit_once('.')
        else {
            continue;
        };
        let Some(definition) =
            definitions.get(&(canonical_module.to_owned(), canonical_name.to_owned()))
        else {
            continue;
        };
        let mut modeled = definition.clone();
        modeled.modeled_factory = Some(factory.clone());
        result.insert((module.to_owned(), name.to_owned()), modeled);
    }
    result
}

pub(super) fn mark_source_verified(contract: &mut Contract) {
    contract.provenance.push(crate::Provenance {
        origin: "installed-checked-source".into(),
        trust: "source-checked".into(),
    });
}

pub(super) fn register_external_bindings(
    file: &SourceFile,
    declarations: &[Declaration],
    bindings: &mut BTreeMap<String, String>,
    inputs: &PreparedFoundations,
) {
    // Replay binding changes for the provider names. This is also where a
    // missing publication retains its actual public identity for coverage.
    let mut selected = BTreeMap::new();
    for declaration in declarations {
        match declaration {
            Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } => {
                let provider = inputs.providers.get(module);
                let internal = provider.is_some() && inputs.providers.get(&file.module) == provider;
                for import in names {
                    if import.name == "*" {
                        selected.clear();
                        continue;
                    }
                    let local = import.alias.as_deref().unwrap_or(&import.name);
                    selected.remove(local);
                    if (provider.is_some() && !internal) || inputs.issue_at(module).is_some() {
                        selected.insert(
                            local.to_owned(),
                            format!("provider:{module}.{}", import.name),
                        );
                    }
                }
            }
            Declaration::Import { names, .. } => {
                for import in names {
                    let local = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                    selected.remove(local);
                    if inputs.providers.contains_key(&import.name)
                        || inputs.issue_at(&import.name).is_some()
                    {
                        // Bind the root of a normal dotted import. An import
                        // with "as" binds the selected module itself.
                        let module = import.alias.as_ref().map_or(local, |_| &import.name);
                        selected.insert(local.to_owned(), format!("module:{module}"));
                    }
                }
            }
            Declaration::Assignment {
                name: Some(name), ..
            }
            | Declaration::TypeAlias {
                name: Some(name), ..
            }
            | Declaration::Function { name, .. }
            | Declaration::Class { name, .. } => {
                selected.remove(name);
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    selected.remove(name);
                }
            }
            Declaration::ModuleFlow { .. } | Declaration::Unsupported { .. } => selected.clear(),
            _ => {}
        }
    }
    bindings.extend(selected);
}

pub(super) fn binding_for_import(
    inputs: Option<&PreparedFoundations>,
    caller: &str,
    module: &str,
    name: &str,
) -> Option<String> {
    let inputs = inputs?;
    let selected = inputs.providers.get(module);
    if selected.is_some() && inputs.providers.get(caller) == selected {
        return None;
    }
    (selected.is_some() || inputs.issue_at(module).is_some())
        .then(|| format!("provider:{module}.{name}"))
}

pub(super) fn public_name(function: &AstExpr, env: &State) -> Option<String> {
    fn module_name(expression: &AstExpr, env: &State) -> Option<String> {
        match &expression.kind {
            AstExprKind::Name(name) if !env.shadowed_functions.contains(name) => env
                .function_bindings
                .get(name)?
                .strip_prefix("module:")
                .map(str::to_owned),
            AstExprKind::Attribute(base, member) => {
                Some(format!("{}.{member}", module_name(base, env)?))
            }
            _ => None,
        }
    }
    match &function.kind {
        AstExprKind::Name(name) if !env.shadowed_functions.contains(name) => env
            .function_bindings
            .get(name)?
            .strip_prefix("provider:")
            .map(str::to_owned),
        AstExprKind::Attribute(base, member) => {
            Some(format!("{}.{member}", module_name(base, env)?))
        }
        _ => None,
    }
}

pub(super) fn record_call(
    expression: &AstExpr,
    env: &State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let Some(inputs) = env.models.foundations.as_ref() else {
        return;
    };
    let Some((function, _, _)) = call_parts(expression) else {
        return;
    };
    let Some(public) = public_name(function, env) else {
        return;
    };
    let Some((module, _)) = public.rsplit_once('.') else {
        return;
    };
    let provider = inputs.module(&public);
    if let Some(reason) = inputs
        .unavailable
        .get(&public)
        .map(String::as_str)
        .or_else(|| inputs.issue_at(module))
    {
        foundation::record_use(
            &public,
            "publication",
            provider.map_or("provider-resolution", |provider| &provider.origin),
            span(file, expression.range),
            Err(reason),
            None,
        );
        boundary(result, file, expression.range, reason);
    }
    if let Some((provider, symbol)) = provider.zip(inputs.exports.get(&public)) {
        for fact in &symbol.contracts {
            if fact.kind == "must-use" {
                foundation::record_use(
                    &public,
                    "must-use",
                    &provider.origin,
                    span(file, expression.range),
                    Ok(()),
                    Some(foundation::publication_origin(provider, symbol, fact)),
                );
            }
        }
    } else if provider
        .is_some_and(|provider| provider.origin == "manifest" && provider.reason.is_none())
        && !inputs.factories.contains_key(&public)
    {
        foundation::record_use(
            &public,
            "publication",
            "manifest",
            span(file, expression.range),
            Err("the selected manifest has no entry for this public export"),
            None,
        );
    }
}

pub(super) fn checked_call(definition: &FunctionBody, file: &SourceFile, range: SourceRange) {
    for fact in &definition.publication {
        foundation::record_use(
            &fact.provider.public_symbol,
            &fact.kind,
            &fact.origin,
            span(file, range),
            Ok(()),
            Some(fact.provider.clone()),
        );
    }
}

pub(super) fn result_matches(definition: &FunctionBody, resource: Option<&Contract>) -> bool {
    definition
        .publication
        .iter()
        .filter(|fact| fact.kind == "owned-return")
        .all(|fact| {
            resource.is_some_and(|resource| fact.resource.as_ref() == Some(&resource.base_symbol))
        })
}

pub(super) fn project_factory(
    factory: &ApplicableFactory,
    bindings: &BTreeMap<String, &AstExpr>,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let arguments_compatible = bindings.values().all(|argument| {
        expr_name(argument)
            .is_none_or(|name| !env.places.contains_key(name) && !env.unknown_places.contains(name))
    });
    let applies = arguments_compatible
        .then_some(())
        .ok_or("a project factory model cannot accept tracked live handles");
    foundation::record_use(
        &factory.model.factory,
        "fresh-owned-return",
        &format!("project-model: {}", factory.model.provenance),
        span(file, range),
        applies,
        Some(factory.origin.clone()),
    );
    queue_raise(env, range, None);
    if let Err(reason) = applies {
        boundary(result, file, range, reason);
        return;
    }
    let resource = Contract {
        schema_version: crate::SCHEMA_VERSION,
        base_symbol: factory.model.resource.clone(),
        display_name: factory.model.resource.clone(),
        lifecycle: Some(crate::Lifecycle {
            closed_by: factory.model.close_method.clone(),
        }),
        access: "owned".into(),
        source: factory
            .origin
            .location
            .clone()
            .unwrap_or_else(|| span(file, range)),
        provenance: vec![crate::Provenance {
            origin: format!("project-model:{}", factory.model.factory),
            trust: "reviewed-model".into(),
        }],
    };
    runtime::allocate_modeled_resource(env, resource, file, range, result);
}

pub(super) fn project_close(resource: &Contract, env: &State) -> Option<ResourceEffectModel> {
    let factory = resource
        .provenance
        .iter()
        .find_map(|item| item.origin.strip_prefix("project-model:"))?;
    let model = &env
        .models
        .foundations
        .as_ref()?
        .factories
        .get(factory)?
        .model;
    Some(crate::model::project_resource_close(
        model,
        &env.target_python_version,
    ))
}

pub(super) fn record_close(
    resource: &Contract,
    env: &State,
    file: &SourceFile,
    range: SourceRange,
) {
    let factory = resource
        .provenance
        .iter()
        .find_map(|item| item.origin.strip_prefix("project-model:"));
    if let Some(factory) =
        factory.and_then(|factory| env.models.foundations.as_ref()?.factories.get(factory))
    {
        foundation::record_use(
            &format!("{}.{}", factory.model.resource, factory.model.close_method),
            "receiver-cleanup",
            &format!("project-model: {}", factory.model.provenance),
            span(file, range),
            Ok(()),
            Some(factory.origin.clone()),
        );
    }
}

use super::*;

pub(super) fn resolved_await_call(
    awaited: &AstExpr,
    file: &SourceFile,
    env: &State,
    functions: &FunctionMap,
) -> bool {
    let AstExprKind::Call { function, .. } = &awaited.kind else {
        return false;
    };
    if let Some(name) = expr_name(function) {
        if env.shadowed_functions.contains(name) {
            return false;
        }
        let scoped = format!("{}::{name}", file.module);
        let definition = if env.module_scope || env.function_bindings_complete {
            env.function_bindings
                .get(name)
                .and_then(|binding| functions.get(binding))
        } else {
            functions.get(&scoped).or_else(|| {
                functions
                    .get(name)
                    .filter(|definition| definition.file.module == file.module)
            })
        };
        return definition.is_some_and(|definition| definition.is_async);
    }
    let Some((receiver, method, _, _)) = called_method(awaited) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    if let Some(definition) =
        ownership::qualified_function_key(name, method, env).and_then(|key| functions.get(&key))
    {
        return definition.is_async;
    }
    env.places.get(name).is_some_and(|targets| {
        !targets.is_empty()
            && targets.iter().all(|object| {
                env.resources.get(object).is_some_and(|resource| {
                    resource
                        .lifecycle
                        .as_ref()
                        .is_some_and(|lifecycle| lifecycle.closed_by == method)
                        || resource_method(resource, method, functions)
                            .is_some_and(|definition| definition.is_async)
                })
            })
    })
}

pub(super) fn await_call_captures(
    awaited: &AstExpr,
    state: &State,
) -> Option<(BTreeSet<String>, bool)> {
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &awaited.kind
    else {
        return None;
    };
    let mut targets = BTreeSet::new();
    let mut unknown = false;
    let mut capture = |expression: &AstExpr| match &expression.kind {
        AstExprKind::Name(name) => {
            if let Some(objects) = state.places.get(name) {
                targets.extend(objects.iter().cloned());
            }
            unknown |= state.unknown_places.contains(name);
        }
        AstExprKind::String(_)
        | AstExprKind::Boolean(_)
        | AstExprKind::NoneLiteral
        | AstExprKind::Primitive => {}
        _ => unknown = true,
    };
    match &function.kind {
        AstExprKind::Attribute(receiver, _) => capture(receiver),
        AstExprKind::Name(_) => {}
        _ => capture(function),
    }
    for argument in arguments {
        capture(argument);
    }
    for (_, argument) in keywords {
        capture(argument);
    }
    Some((targets, unknown))
}

pub(super) fn resolve_imported_contracts(
    module: &str,
    name: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &Inspection,
    visited: &mut BTreeSet<(String, String)>,
) -> Vec<Contract> {
    let key = (module.to_owned(), name.to_owned());
    if !visited.insert(key.clone()) {
        return Vec::new();
    }
    let resolved = (|| {
        let provider = files.iter().find(|file| file.module == module)?;
        let declarations = summaries.get(&provider.path)?.declarations.as_ref()?;
        let mut binding = Vec::new();
        for declaration in declarations {
            match declaration {
                Declaration::Class {
                    name: bound,
                    decorators,
                    ..
                } if bound == name => {
                    let ranges: Vec<_> =
                        decorators.iter().map(|decorator| decorator.range).collect();
                    binding = class_resource_contract(provider, bound, &ranges, &result.contracts)
                        .into_iter()
                        .collect();
                }
                Declaration::Function { name: bound, .. }
                | Declaration::TypeAlias {
                    name: Some(bound), ..
                } if bound == name => binding.clear(),
                Declaration::Assignment {
                    name: Some(bound),
                    value: Some(_),
                    range,
                    ..
                } if bound == name => {
                    binding = result
                        .contracts
                        .iter()
                        .find(|contract| {
                            contract.display_name == *bound
                                && contract_inside_range(contract, provider, *range)
                        })
                        .cloned()
                        .into_iter()
                        .collect();
                }
                Declaration::Delete { names, .. }
                    if names.iter().any(|bound| bound.as_deref() == Some(name)) =>
                {
                    binding.clear();
                }
                Declaration::ImportFrom {
                    module: source_module,
                    level,
                    names,
                    ..
                } => {
                    if let Some(import) = names
                        .iter()
                        .find(|import| import.alias.as_deref().unwrap_or(&import.name) == name)
                    {
                        binding = if *level == 0 {
                            resolve_imported_contracts(
                                source_module,
                                &import.name,
                                files,
                                summaries,
                                result,
                                visited,
                            )
                        } else {
                            Vec::new()
                        };
                    } else if names.iter().any(|import| import.name == "*") {
                        binding.clear();
                    }
                }
                Declaration::Import { names, .. } => {
                    if names.iter().any(|import| {
                        import
                            .alias
                            .as_deref()
                            .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default())
                            == name
                    }) {
                        binding.clear();
                    }
                }
                Declaration::Unsupported { .. } => binding.clear(),
                _ => {}
            }
        }
        Some(binding)
    })()
    .unwrap_or_default();
    visited.remove(&key);
    resolved
}

pub(super) fn contracts_for_module(
    file: &SourceFile,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &Inspection,
) -> Vec<Contract> {
    let mut contracts: Vec<_> = result
        .contracts
        .iter()
        .filter(|contract| {
            contract.source.file == file.path
                || (contract.access == "shared"
                    && contract.lifecycle.is_some()
                    && contract.display_name.contains('.'))
        })
        .cloned()
        .collect();
    if let Some(declarations) = summaries
        .get(&file.path)
        .and_then(|summary| summary.declarations.as_ref())
    {
        for declaration in declarations {
            if let Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } = declaration
            {
                for import in names {
                    for mut contract in resolve_imported_contracts(
                        module,
                        &import.name,
                        files,
                        summaries,
                        result,
                        &mut BTreeSet::new(),
                    ) {
                        contract.display_name =
                            import.alias.as_deref().unwrap_or(&import.name).to_owned();
                        contracts.push(contract);
                    }
                }
            }
        }
    }
    contracts.sort_by(|left, right| {
        left.base_symbol
            .cmp(&right.base_symbol)
            .then(left.display_name.cmp(&right.display_name))
    });
    contracts.dedup_by(|left, right| {
        left.base_symbol == right.base_symbol && left.display_name == right.display_name
    });
    contracts
}

pub(super) fn final_class_bindings(
    file: &SourceFile,
    declarations: &[Declaration],
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &Inspection,
) -> BTreeMap<String, Option<Contract>> {
    let mut bindings = BTreeMap::new();
    for declaration in declarations {
        match declaration {
            Declaration::Class {
                name, decorators, ..
            } => {
                let ranges: Vec<_> = decorators.iter().map(|decorator| decorator.range).collect();
                bindings.insert(
                    name.clone(),
                    class_resource_contract(file, name, &ranges, &result.contracts),
                );
            }
            Declaration::Function { name, .. } => {
                bindings.insert(name.clone(), None);
            }
            Declaration::Assignment {
                name: Some(name),
                value,
                range,
                ..
            } => {
                if let Some(value) = value {
                    let resource_view = result
                        .contracts
                        .iter()
                        .filter(|contract| {
                            contract.display_name == *name
                                && contract_inside_range(contract, file, *range)
                        })
                        .max_by_key(|contract| (contract.source.line, contract.source.column))
                        .cloned();
                    let binding = resource_view.or_else(|| {
                        expr_name(value)
                            .and_then(|source| bindings.get(source))
                            .cloned()
                            .flatten()
                    });
                    bindings.insert(name.clone(), binding);
                }
            }
            Declaration::ImportFrom {
                module,
                level,
                names,
                ..
            } => {
                if names.iter().any(|import| import.name == "*") {
                    for binding in bindings.values_mut() {
                        *binding = None;
                    }
                    bindings.insert(UNKNOWN_EXCEPTION_BINDINGS.to_owned(), None);
                } else {
                    for import in names {
                        let local = import.alias.as_deref().unwrap_or(&import.name);
                        let binding = if *level == 0 {
                            resolve_imported_contracts(
                                module,
                                &import.name,
                                files,
                                summaries,
                                result,
                                &mut BTreeSet::new(),
                            )
                            .into_iter()
                            .next()
                            .map(|mut contract| {
                                contract.display_name = local.to_owned();
                                contract
                            })
                        } else {
                            None
                        };
                        bindings.insert(local.to_owned(), binding);
                    }
                }
            }
            Declaration::Import { names, .. } => {
                for import in names {
                    let local = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                    bindings.insert(local.to_owned(), None);
                }
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    bindings.insert(name.clone(), None);
                }
            }
            Declaration::TypeAlias {
                name: Some(name), ..
            } => {
                bindings.insert(name.clone(), None);
            }
            Declaration::Unsupported {
                message,
                unknown_exception_bindings,
                ..
            } => {
                for binding in bindings.values_mut() {
                    *binding = None;
                }
                for name in unknown_exception_bindings {
                    bindings.insert(name.clone(), None);
                }
                if *message == crate::frontend::MODULE_STAR_IMPORT_UNCERTAINTY {
                    bindings.insert(UNKNOWN_EXCEPTION_BINDINGS.to_owned(), None);
                }
            }
            Declaration::Assignment { .. }
            | Declaration::TypeAlias { .. }
            | Declaration::Expression { .. } => {}
        }
    }
    bindings
}

pub(super) fn final_function_bindings(
    module: &str,
    functions: &FunctionMap,
    declarations: &[Declaration],
    supplied_modules: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let prefix = format!("{module}::");
    let mut bindings: BTreeMap<_, _> = functions
        .iter()
        .filter_map(|(key, body)| {
            let name = key.strip_prefix(&prefix)?;
            if name.starts_with('@') || name.contains(':') {
                return None;
            }
            let binding = body.binding_key.as_ref()?;
            functions
                .contains_key(binding)
                .then(|| (name.to_owned(), binding.clone()))
        })
        .collect();
    for declaration in declarations {
        match declaration {
            Declaration::Function { name, .. } => {
                if let Some(definition) = functions.get(&format!("{module}::{name}")) {
                    if let Some(binding) = &definition.binding_key {
                        bindings.insert(name.clone(), binding.clone());
                    }
                }
            }
            Declaration::Class { name, .. } => {
                bindings.remove(name);
            }
            Declaration::ImportFrom {
                module: source,
                level: 0,
                names,
                range,
            } => {
                for import in names {
                    let local = import.alias.as_deref().unwrap_or(&import.name);
                    if let Some(external) = ownership::external_member(source, &import.name)
                        .filter(|_| !supplied_modules.contains(source))
                    {
                        bindings.insert(local.to_owned(), external.into());
                    } else if source == "codepact" && import.name == "discard" {
                        bindings.insert(local.to_owned(), "external:codepact.discard".into());
                    } else if source == "contextlib"
                        && !supplied_modules.contains(source)
                        && matches!(import.name.as_str(), "closing" | "aclosing")
                    {
                        bindings.insert(
                            local.to_owned(),
                            format!("external:contextlib.{}", import.name),
                        );
                    } else if let Some(binding) = functions
                        .get(&format!("import:{module}:{}:{local}", range.start))
                        .and_then(|definition| definition.binding_key.as_ref())
                    {
                        bindings.insert(local.to_owned(), binding.clone());
                    } else {
                        bindings.remove(local);
                    }
                }
            }
            Declaration::Import { names, .. } => {
                for import in names {
                    let local = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                    if matches!(import.name.as_str(), "contextlib" | "codepact" | "asyncio")
                        && !supplied_modules.contains(&import.name)
                    {
                        bindings.insert(local.to_owned(), format!("external:{}", import.name));
                    } else if supplied_modules.contains(&import.name) {
                        bindings.insert(local.to_owned(), format!("module:{}", import.name));
                    } else {
                        bindings.remove(local);
                    }
                }
            }
            Declaration::Assignment {
                name: Some(name), ..
            } => {
                bindings.remove(name);
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    bindings.remove(name);
                }
            }
            Declaration::TypeAlias {
                name: Some(name), ..
            } => {
                bindings.remove(name);
            }
            Declaration::Unsupported { .. } => bindings.clear(),
            _ => {}
        }
    }
    bindings
}

pub(super) fn external_module_bindings(
    declarations: &[Declaration],
    exports: &BTreeMap<String, BTreeMap<String, String>>,
    supplied_modules: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut bindings = BTreeMap::new();
    for declaration in declarations {
        match declaration {
            Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } => {
                for import in names {
                    let local = import.alias.as_deref().unwrap_or(&import.name);
                    let external = if let Some(external) =
                        ownership::external_member(module, &import.name)
                            .filter(|_| !supplied_modules.contains(module))
                    {
                        Some(external.into())
                    } else if module == "codepact" && import.name == "discard" {
                        Some("external:codepact.discard".into())
                    } else if module == "contextlib"
                        && !supplied_modules.contains(module)
                        && matches!(import.name.as_str(), "closing" | "aclosing")
                    {
                        Some(format!("external:contextlib.{}", import.name))
                    } else {
                        exports
                            .get(module)
                            .and_then(|module| module.get(&import.name))
                            .cloned()
                    };
                    if let Some(external) = external {
                        bindings.insert(local.to_owned(), external);
                    } else {
                        bindings.remove(local);
                    }
                }
            }
            Declaration::Import { names, .. } => {
                for import in names {
                    let local = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                    if matches!(import.name.as_str(), "contextlib" | "codepact" | "asyncio")
                        && !supplied_modules.contains(&import.name)
                    {
                        bindings.insert(local.to_owned(), format!("external:{}", import.name));
                    } else if supplied_modules.contains(&import.name) {
                        bindings.insert(local.to_owned(), format!("module:{}", import.name));
                    } else {
                        bindings.remove(local);
                    }
                }
            }
            Declaration::Assignment {
                name: Some(name),
                value: Some(value),
                ..
            } => {
                let external = match &value.kind {
                    AstExprKind::Name(source) => bindings.get(source).cloned(),
                    AstExprKind::Attribute(base, member) => expr_name(base)
                        .and_then(|name| bindings.get(name))
                        .and_then(|binding| binding.strip_prefix("external:"))
                        .and_then(|module| ownership::external_member(module, member))
                        .map(str::to_owned),
                    _ => None,
                };
                if let Some(external) = external {
                    bindings.insert(name.clone(), external);
                } else {
                    bindings.remove(name);
                }
            }
            Declaration::Function { name, .. }
            | Declaration::Class { name, .. }
            | Declaration::Assignment {
                name: Some(name), ..
            }
            | Declaration::TypeAlias {
                name: Some(name), ..
            } => {
                bindings.remove(name);
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    bindings.remove(name);
                }
            }
            Declaration::Unsupported { .. } => bindings.clear(),
            _ => {}
        }
    }
    bindings
}

pub(super) fn resolve_imported_function(
    module: &str,
    name: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    functions: &ProjectFunctionMap,
    cyclic_modules: &BTreeSet<String>,
    visited: &mut BTreeSet<(String, String)>,
) -> Option<FunctionBody> {
    // A resolvable body does not make a cyclic import safe to assume at runtime.
    if cyclic_modules.contains(module) {
        return None;
    }
    if !visited.insert((module.to_owned(), name.to_owned())) {
        return None;
    }
    let provider = files.iter().find(|file| file.module == module)?;
    let declarations = summaries.get(&provider.path)?.declarations.as_ref()?;
    for declaration in declarations.iter().rev() {
        match declaration {
            Declaration::Function {
                name: bound,
                decorators,
                must_use_decorator,
                unsupported,
                ..
            } if bound == name => {
                return if (decorators.is_empty() || *must_use_decorator) && !unsupported {
                    functions
                        .get(&(module.to_owned(), name.to_owned()))
                        .cloned()
                } else {
                    None
                };
            }
            Declaration::Class { name: bound, .. }
            | Declaration::TypeAlias {
                name: Some(bound), ..
            } if bound == name => {
                return None;
            }
            Declaration::Assignment {
                name: Some(bound),
                value: Some(_),
                ..
            } if bound == name => return None,
            Declaration::Delete { names, .. }
                if names.iter().any(|bound| bound.as_deref() == Some(name)) =>
            {
                return None;
            }
            Declaration::ImportFrom {
                module: source,
                level,
                names,
                ..
            } => {
                if let Some(import) = names
                    .iter()
                    .find(|import| import.alias.as_deref().unwrap_or(&import.name) == name)
                {
                    return if *level == 0 {
                        resolve_imported_function(
                            source,
                            &import.name,
                            files,
                            summaries,
                            functions,
                            cyclic_modules,
                            visited,
                        )
                    } else {
                        None
                    };
                }
                if names.iter().any(|import| import.name == "*") {
                    return None;
                }
            }
            Declaration::Import { names, .. } => {
                if names.iter().any(|import| {
                    import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default())
                        == name
                }) {
                    return None;
                }
            }
            Declaration::Unsupported { .. } => return None,
            _ => {}
        }
    }
    None
}

pub(super) fn evaluate_call_arguments(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    let AstExprKind::Call {
        arguments,
        keywords,
        ..
    } = &expr.kind
    else {
        return;
    };
    for argument in arguments
        .iter()
        .chain(keywords.iter().map(|(_, value)| value))
    {
        if !env.alive {
            break;
        }
        let is_possibly_unbound_name = matches!(
            &argument.kind,
            AstExprKind::Name(name) if env.possibly_unbound_places.contains(name)
        );
        if is_possibly_unbound_name {
            if let Some(name) = expr_name(argument) {
                report_possibly_unbound_alias(name, argument.range, env, file, result);
            }
        } else if !matches!(
            argument.kind,
            AstExprKind::Name(_)
                | AstExprKind::String(_)
                | AstExprKind::Boolean(_)
                | AstExprKind::NoneLiteral
                | AstExprKind::Primitive
        ) {
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
        }
    }
}

fn capture_coroutine_place(
    expression: &mut AstExpr,
    env: &State,
    id: &str,
    next_capture: &mut usize,
    captures: &mut BTreeMap<String, CapturedCoroutinePlace>,
) -> bool {
    let AstExprKind::Name(name) = &expression.kind else {
        if matches!(
            expression.kind,
            AstExprKind::String(_)
                | AstExprKind::Boolean(_)
                | AstExprKind::NoneLiteral
                | AstExprKind::Primitive
        ) {
            return false;
        }
        expression.kind = AstExprKind::Other;
        return true;
    };
    let transferred = env
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.transfer_arguments.contains(name));
    let Some(targets) = env.places.get(name) else {
        let unknown = env.unknown_places.contains(name);
        let synthetic = format!("__codepact_coro_{}_{}", id, *next_capture);
        *next_capture = next_capture.saturating_add(1);
        captures.insert(
            synthetic.clone(),
            CapturedCoroutinePlace {
                targets: BTreeSet::new(),
                unknown,
                transferred: false,
            },
        );
        expression.kind = AstExprKind::Name(synthetic);
        return unknown;
    };
    let unknown = env.unknown_places.contains(name);
    let synthetic = format!("__codepact_coro_{}_{}", id, *next_capture);
    *next_capture = next_capture.saturating_add(1);
    captures.insert(
        synthetic.clone(),
        CapturedCoroutinePlace {
            targets: targets.clone(),
            unknown,
            transferred,
        },
    );
    expression.kind = AstExprKind::Name(synthetic);
    unknown
}

pub(super) fn register_pending_coroutine(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
    binding_override: Option<String>,
) {
    let id = format!(
        "{}:{}:{}",
        file.path, expression.range.start, expression.range.end
    );
    let mut invocation = expression.clone();
    let mut captures = BTreeMap::new();
    let mut next_capture = 0;
    let mut unsupported_argument = false;
    let mut unsupported_target = false;
    if let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &mut invocation.kind
    {
        match &mut function.kind {
            AstExprKind::Attribute(receiver, _) => {
                capture_coroutine_place(receiver, env, &id, &mut next_capture, &mut captures);
            }
            AstExprKind::Name(callee) => {
                let binding =
                    binding_override.or_else(|| env.function_bindings.get(callee).cloned());
                let synthetic = format!("__codepact_coro_func_{id}");
                if let Some(binding) = binding {
                    env.function_bindings.insert(synthetic.clone(), binding);
                } else {
                    unsupported_target = true;
                }
                **function = AstExpr {
                    range: function.range,
                    kind: AstExprKind::Name(synthetic),
                };
            }
            _ => unsupported_target = true,
        }
        for argument in arguments {
            unsupported_argument |=
                capture_coroutine_place(argument, env, &id, &mut next_capture, &mut captures);
        }
        for (_, argument) in keywords {
            unsupported_argument |=
                capture_coroutine_place(argument, env, &id, &mut next_capture, &mut captures);
        }
    }
    if unsupported_argument {
        let affected = env.places.values().flatten().cloned().collect::<Vec<_>>();
        weaken(env, affected.into_iter());
        env.unknown_places.extend(env.places.keys().cloned());
        boundary(
            result,
            file,
            expression.range,
            "coroutine argument expression was evaluated at creation but its value cannot be retained precisely",
        );
    }
    if unsupported_target {
        let affected = env.places.values().flatten().cloned().collect::<Vec<_>>();
        weaken(env, affected.into_iter());
        boundary(
            result,
            file,
            expression.range,
            "coroutine callable identity cannot be retained across creation and await",
        );
    }
    let record = CoroutineRecord {
        invocation,
        file_path: file.path.clone(),
        captures,
        range: expression.range,
        origin: span(file, expression.range),
        consumed: false,
        ambiguous: false,
    };
    if let Some(existing) = env.coroutines.get_mut(&id) {
        existing.ambiguous = true;
        existing.consumed = false;
        for (capture, place) in record.captures {
            let existing_place = existing.captures.entry(capture).or_default();
            existing_place.targets.extend(place.targets);
            existing_place.unknown |= place.unknown;
            existing_place.transferred &= place.transferred;
        }
        boundary(
            result,
            file,
            expression.range,
            "repeated coroutine creation at one source site has unknown instance multiplicity",
        );
    } else if env.coroutines.len() >= MAX_PENDING_COROUTINES {
        let mut affected = BTreeSet::new();
        for place in record.captures.values() {
            affected.extend(place.targets.iter().cloned());
            if place.unknown {
                affected.extend(env.objects.keys().cloned());
            }
        }
        for object in affected {
            *env.objects.entry(object).or_default() |= UNKNOWN;
        }
        env.coroutine_budget_overflow = Some(expression.range);
        boundary(
            result,
            file,
            expression.range,
            "pending coroutine budget exceeded; captured resource effects are unknown",
        );
    } else {
        env.coroutines.insert(id.clone(), record);
    }
    env.pending_coroutine_result = Some(id);
}

use crate::{
    frontend::{AstExpr, AstExprKind, Declaration, FlowStatement, SourceRange},
    Contract, Diagnostic, Inspection, SourceFile, Span,
};
use std::collections::{BTreeMap, BTreeSet};

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const MAX_CALL_DEPTH: usize = 8;
type CallKeyword = (Option<String>, AstExpr);
type MethodCall<'a> = (&'a AstExpr, &'a str, &'a [AstExpr], &'a [CallKeyword]);
type ProjectFunctionMap = BTreeMap<(String, String), (Vec<String>, Vec<FlowStatement>)>;

#[derive(Clone, Default)]
struct State {
    places: BTreeMap<String, BTreeSet<String>>,
    unknown_places: BTreeSet<String>,
    shadowed_classes: BTreeSet<String>,
    objects: BTreeMap<String, u8>,
    kinds: BTreeMap<String, String>,
    borrowed: BTreeSet<String>,
    escaped: BTreeSet<String>,
    call_stack: BTreeSet<String>,
    close_origins: BTreeMap<String, Span>,
    alive: bool,
}

fn expr_name(expr: &AstExpr) -> Option<&str> {
    if let AstExprKind::Name(name) = &expr.kind {
        Some(name)
    } else {
        None
    }
}

fn called_method(expr: &AstExpr) -> Option<MethodCall<'_>> {
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &expr.kind
    else {
        return None;
    };
    let AstExprKind::Attribute(receiver, method) = &function.kind else {
        return None;
    };
    Some((receiver, method, arguments, keywords))
}

fn merge(left: &State, right: &State) -> State {
    let mut joined = left.clone();
    for (place, targets) in &right.places {
        joined
            .places
            .entry(place.clone())
            .or_default()
            .extend(targets.iter().cloned());
    }
    joined
        .unknown_places
        .extend(right.unknown_places.iter().cloned());
    joined
        .shadowed_classes
        .extend(right.shadowed_classes.iter().cloned());
    for place in left.places.keys() {
        if !right.places.contains_key(place) {
            joined.unknown_places.insert(place.clone());
        }
    }
    for place in right.places.keys() {
        if !left.places.contains_key(place) {
            joined.unknown_places.insert(place.clone());
        }
    }
    for (object, state) in &right.objects {
        *joined.objects.entry(object.clone()).or_default() |= *state;
    }
    joined.kinds.extend(right.kinds.clone());
    joined.borrowed.extend(right.borrowed.iter().cloned());
    joined.escaped.extend(right.escaped.iter().cloned());
    for (object, origin) in &right.close_origins {
        joined
            .close_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.alive |= right.alive;
    joined
}

fn span(file: &SourceFile, range: SourceRange) -> Span {
    crate::source_span(&file.path, &file.source, range)
}

fn expression_path(expression: &AstExpr) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(name.clone()),
        AstExprKind::Attribute(value, name) => {
            Some(format!("{}.{}", expression_path(value)?, name))
        }
        _ => None,
    }
}

fn resolve_import_path(path: String, imports: &BTreeMap<String, String>) -> String {
    if let Some((alias, canonical)) = imports
        .iter()
        .filter(|(alias, _)| path == ***alias || path.starts_with(&format!("{alias}.")))
        .max_by_key(|(alias, _)| alias.len())
    {
        return format!("{}{}", canonical, &path[alias.len()..]);
    }
    path
}

fn borrowed_resource(annotation: &AstExpr, imports: &BTreeMap<String, String>) -> Option<String> {
    let AstExprKind::Subscript(container, arguments) = &annotation.kind else {
        return None;
    };
    let container_path = expression_path(container)?;
    let canonical = resolve_import_path(container_path, imports);
    if matches!(canonical.as_str(), "typepact.rust.Borrowed" | "Borrowed") {
        return arguments.first().and_then(expression_path);
    }
    if !matches!(canonical.as_str(), "typing.Annotated" | "Annotated") {
        return None;
    }
    let kind = arguments.first().and_then(expression_path)?;
    let is_shared_borrow = arguments.iter().skip(1).any(|metadata| {
        let path = match &metadata.kind {
            AstExprKind::Call { function, .. } => expression_path(function),
            _ => expression_path(metadata),
        };
        path.is_some_and(|path| {
            matches!(
                resolve_import_path(path, imports).as_str(),
                "typepact.contracts.SharedBorrow" | "SharedBorrow"
            )
        })
    });
    is_shared_borrow.then_some(kind)
}

fn report(
    result: &mut Inspection,
    code: &str,
    message: &str,
    primary: Span,
    related: Vec<Span>,
    certainty: &str,
) {
    if result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code && diagnostic.primary == primary)
    {
        return;
    }
    result.diagnostics.push(Diagnostic {
        code: code.into(),
        severity: if certainty == "definite" {
            "error"
        } else {
            "warning"
        }
        .into(),
        message: message.into(),
        primary,
        related,
        certainty: certainty.into(),
    });
}

fn boundary(result: &mut Inspection, file: &SourceFile, range: SourceRange, message: &str) {
    let primary = span(file, range);
    let reason = format!("{}:{}: {message}", file.path, primary.line);
    result.coverage.status = "incomplete".into();
    if result.coverage.reasons.contains(&reason) {
        return;
    }
    result.coverage.reasons.push(reason);
    result.diagnostics.push(Diagnostic {
        code: "TP9001".into(),
        severity: "warning".into(),
        message: message.into(),
        primary,
        related: Vec::new(),
        certainty: "unknown".into(),
    });
}

fn weaken(state: &mut State, objects: impl Iterator<Item = String>) {
    for object in objects {
        *state.objects.entry(object).or_default() |= UNKNOWN;
    }
    for object in state.escaped.clone() {
        *state.objects.entry(object).or_default() |= UNKNOWN;
    }
}

type FunctionMap = BTreeMap<String, (Vec<String>, Vec<FlowStatement>)>;

fn copy_object_metadata(child: &mut State, parent: &State, objects: &BTreeSet<String>) {
    for object in objects {
        if let Some(mask) = parent.objects.get(object) {
            child.objects.insert(object.clone(), *mask);
        }
        if let Some(kind) = parent.kinds.get(object) {
            child.kinds.insert(object.clone(), kind.clone());
        }
        if parent.borrowed.contains(object) {
            child.borrowed.insert(object.clone());
        }
        if let Some(origin) = parent.close_origins.get(object) {
            child.close_origins.insert(object.clone(), origin.clone());
        }
    }
}

fn resolve_imported_contracts(
    module: &str,
    name: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &Inspection,
    visited: &mut BTreeSet<(String, String)>,
) -> Vec<Contract> {
    if !visited.insert((module.to_owned(), name.to_owned())) {
        return Vec::new();
    }
    let direct: Vec<_> = result
        .contracts
        .iter()
        .filter(|contract| {
            contract.lifecycle.is_some()
                && contract
                    .base_symbol
                    .starts_with(&format!("{module}.{name}#"))
        })
        .cloned()
        .collect();
    if !direct.is_empty() {
        return direct;
    }
    let Some(provider) = files.iter().find(|file| file.module == module) else {
        return Vec::new();
    };
    let Some(declarations) = summaries
        .get(&provider.path)
        .and_then(|summary| summary.declarations.as_ref())
    else {
        return Vec::new();
    };
    declarations
        .iter()
        .filter_map(|declaration| {
            let Declaration::ImportFrom {
                module: source_module,
                level: 0,
                names,
                ..
            } = declaration
            else {
                return None;
            };
            names
                .iter()
                .find(|import| import.alias.as_deref().unwrap_or(&import.name) == name)
                .map(|import| (source_module.clone(), import.name.clone()))
        })
        .flat_map(|(source_module, source_name)| {
            resolve_imported_contracts(
                &source_module,
                &source_name,
                files,
                summaries,
                result,
                visited,
            )
        })
        .collect()
}

fn resolve_imported_function(
    module: &str,
    name: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    functions: &ProjectFunctionMap,
    visited: &mut BTreeSet<(String, String)>,
) -> Option<(Vec<String>, Vec<FlowStatement>)> {
    if !visited.insert((module.to_owned(), name.to_owned())) {
        return None;
    }
    if let Some(function) = functions.get(&(module.to_owned(), name.to_owned())) {
        return Some(function.clone());
    }
    let provider = files.iter().find(|file| file.module == module)?;
    let declarations = summaries.get(&provider.path)?.declarations.as_ref()?;
    for declaration in declarations {
        let Declaration::ImportFrom {
            module: source_module,
            level: 0,
            names,
            ..
        } = declaration
        else {
            continue;
        };
        if let Some(import) = names
            .iter()
            .find(|import| import.alias.as_deref().unwrap_or(&import.name) == name)
        {
            if let Some(function) = resolve_imported_function(
                source_module,
                &import.name,
                files,
                summaries,
                functions,
                visited,
            ) {
                return Some(function);
            }
        }
    }
    None
}

fn evaluate_call_arguments(
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
        if matches!(
            argument.kind,
            AstExprKind::Call { .. } | AstExprKind::Tuple(_)
        ) {
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
        }
    }
}

fn consume_expr(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    if depth > MAX_CALL_DEPTH {
        weaken(
            env,
            env.places
                .values()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .into_iter(),
        );
        boundary(
            result,
            file,
            expr.range,
            "call-summary recursion budget exceeded; affected resources are unknown",
        );
        return;
    }
    evaluate_call_arguments(expr, env, file, contracts, functions, result, depth);
    if let Some((receiver, method, arguments, keywords)) = called_method(expr) {
        if let Some(name) = expr_name(receiver) {
            if let Some(targets) = env.places.get(name).cloned() {
                let has_open_alternative = targets.iter().any(|object| {
                    let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
                    mask & (OPEN | UNKNOWN) != 0
                }) || env.unknown_places.contains(name);
                for object in &targets {
                    let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
                    let is_close = contracts.iter().any(|contract| {
                        contract.display_name
                            == env
                                .kinds
                                .get(object)
                                .map(String::as_str)
                                .unwrap_or_default()
                            && contract
                                .lifecycle
                                .as_ref()
                                .is_some_and(|lifecycle| lifecycle.closed_by == method)
                    });
                    if mask & CLOSED != 0 && !is_close {
                        let certainty = if !has_open_alternative && mask & UNKNOWN == 0 {
                            "definite"
                        } else {
                            "possible"
                        };
                        let primary = span(file, expr.range);
                        let related = env.close_origins.get(object).cloned().into_iter().collect();
                        report(
                            result,
                            "TP1001",
                            "use of a resource that may be closed",
                            primary,
                            related,
                            certainty,
                        );
                    }
                    if env.borrowed.contains(object)
                        && contracts.iter().any(|contract| {
                            contract.display_name
                                == env
                                    .kinds
                                    .get(object)
                                    .map(String::as_str)
                                    .unwrap_or_default()
                                && contract
                                    .lifecycle
                                    .as_ref()
                                    .is_some_and(|lifecycle| lifecycle.closed_by == method)
                        })
                    {
                        report(
                            result,
                            "TP2001",
                            "Borrowed parameter cannot close the resource",
                            span(file, expr.range),
                            Vec::new(),
                            "definite",
                        );
                    }
                    if is_close {
                        if mask & CLOSED != 0 {
                            boundary(
                                result,
                                file,
                                expr.range,
                                "repeat-close behavior is not modeled",
                            );
                        }
                        env.close_origins
                            .insert(object.clone(), span(file, expr.range));
                        if targets.len() == 1 && !env.unknown_places.contains(name) {
                            env.objects.insert(object.clone(), CLOSED);
                        } else {
                            *env.objects.entry(object.clone()).or_default() |= CLOSED;
                        }
                    } else if let Some((parameters, body)) = functions.get(&format!(
                        "@{}.{}",
                        env.kinds
                            .get(object)
                            .map(String::as_str)
                            .unwrap_or_default(),
                        method
                    )) {
                        let method_key = format!(
                            "@{}.{}",
                            env.kinds
                                .get(object)
                                .map(String::as_str)
                                .unwrap_or_default(),
                            method
                        );
                        if env.call_stack.contains(&method_key) {
                            weaken(env, [object.clone()].into_iter());
                            boundary(
                                result,
                                file,
                                expr.range,
                                "recursive method summary is unknown",
                            );
                            continue;
                        }
                        let mut child = State {
                            alive: true,
                            call_stack: env.call_stack.clone(),
                            ..State::default()
                        };
                        child.call_stack.insert(method_key);
                        child.places.insert(
                            parameters.first().cloned().unwrap_or_else(|| "self".into()),
                            BTreeSet::from([object.clone()]),
                        );
                        copy_object_metadata(&mut child, env, &BTreeSet::from([object.clone()]));
                        child.objects.insert(object.clone(), mask);
                        if let Some(origin) = env.close_origins.get(object) {
                            child.close_origins.insert(object.clone(), origin.clone());
                        }
                        for (parameter, argument) in parameters.iter().skip(1).zip(arguments) {
                            if let Some(actual) = expr_name(argument) {
                                if let Some(targets) = env.places.get(actual) {
                                    child.places.insert(parameter.clone(), targets.clone());
                                    copy_object_metadata(&mut child, env, targets);
                                }
                            }
                        }
                        for (name, argument) in keywords {
                            let Some(index) = name.as_ref().and_then(|name| {
                                parameters.iter().position(|parameter| parameter == name)
                            }) else {
                                continue;
                            };
                            if let Some(actual) = expr_name(argument) {
                                if let Some(targets) = env.places.get(actual) {
                                    child
                                        .places
                                        .insert(parameters[index].clone(), targets.clone());
                                    copy_object_metadata(&mut child, env, targets);
                                }
                            }
                        }
                        execute(
                            body,
                            &mut child,
                            file,
                            contracts,
                            functions,
                            result,
                            depth + 1,
                        );
                        for (changed_object, value) in child.objects {
                            env.objects.insert(changed_object, value);
                        }
                        env.close_origins.extend(child.close_origins);
                        env.escaped.extend(child.escaped);
                    } else if mask & OPEN != 0 && !contracts.is_empty() {
                        *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
                        boundary(
                            result,
                            file,
                            expr.range,
                            &format!("unknown method effect for {method}"),
                        );
                    }
                }
            } else {
                weaken(
                    env,
                    env.places
                        .values()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter(),
                );
                boundary(
                    result,
                    file,
                    expr.range,
                    "unresolved method receiver may affect tracked resources",
                );
            }
        } else {
            // Attribute calls without a resolved local resource receiver are unknown effects.
            weaken(
                env,
                env.places
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            boundary(
                result,
                file,
                expr.range,
                "unresolved method call may affect tracked resources",
            );
        }
        for argument in arguments {
            if let Some(name) = expr_name(argument) {
                if let Some(targets) = env.places.get(name) {
                    env.escaped.extend(targets.iter().cloned());
                }
            }
        }
        for (_, argument) in keywords {
            if let Some(name) = expr_name(argument) {
                if let Some(targets) = env.places.get(name) {
                    env.escaped.extend(targets.iter().cloned());
                }
            }
        }
        return;
    }
    if let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &expr.kind
    {
        if let Some(callee) = expr_name(function) {
            if let Some((parameters, body)) = functions.get(callee) {
                if env.call_stack.contains(callee) {
                    weaken(
                        env,
                        env.places
                            .values()
                            .flatten()
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_iter(),
                    );
                    boundary(
                        result,
                        file,
                        expr.range,
                        "recursive function summary is unknown",
                    );
                    return;
                }
                let mut child = State {
                    alive: true,
                    call_stack: env.call_stack.clone(),
                    ..State::default()
                };
                child.call_stack.insert(callee.to_string());
                for (parameter, argument) in parameters.iter().zip(arguments) {
                    if let Some(actual) = expr_name(argument) {
                        if let Some(targets) = env.places.get(actual) {
                            child.places.insert(parameter.clone(), targets.clone());
                            copy_object_metadata(&mut child, env, targets);
                            if env.unknown_places.contains(actual) {
                                child.unknown_places.insert(parameter.clone());
                            }
                        }
                    }
                }
                for (name, argument) in keywords {
                    let Some(name) = name else {
                        weaken(
                            env,
                            env.places
                                .values()
                                .flatten()
                                .cloned()
                                .collect::<Vec<_>>()
                                .into_iter(),
                        );
                        continue;
                    };
                    if let Some(index) = parameters.iter().position(|parameter| parameter == name) {
                        if let Some(actual) = expr_name(argument) {
                            if let Some(targets) = env.places.get(actual) {
                                child
                                    .places
                                    .insert(parameters[index].clone(), targets.clone());
                                copy_object_metadata(&mut child, env, targets);
                                if env.unknown_places.contains(actual) {
                                    child.unknown_places.insert(parameters[index].clone());
                                }
                            }
                        }
                    }
                }
                let diagnostic_start = result.diagnostics.len();
                execute(
                    body,
                    &mut child,
                    file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                );
                for diagnostic in result.diagnostics.iter_mut().skip(diagnostic_start) {
                    if diagnostic.code == "TP1001" {
                        diagnostic.primary = span(file, expr.range);
                    }
                }
                for object in child.objects.keys() {
                    if let Some(value) = child.objects.get(object) {
                        env.objects.insert(object.clone(), *value);
                    }
                }
                env.close_origins.extend(child.close_origins);
                env.escaped.extend(child.escaped);
            } else {
                let mut affected = Vec::new();
                for argument in arguments {
                    if let Some(name) = expr_name(argument) {
                        if let Some(targets) = env.places.get(name) {
                            affected.extend(targets.iter().cloned());
                        }
                    }
                }
                for (_, argument) in keywords {
                    if let Some(name) = expr_name(argument) {
                        if let Some(targets) = env.places.get(name) {
                            affected.extend(targets.iter().cloned());
                        }
                    }
                }
                weaken(env, affected.into_iter());
                env.escaped.extend(env.places.values().flatten().cloned());
                boundary(
                    result,
                    file,
                    expr.range,
                    &format!("unknown call effect for {callee}"),
                );
            }
        } else {
            weaken(
                env,
                env.places
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            boundary(
                result,
                file,
                expr.range,
                "unresolved call may affect tracked resources",
            );
        }
    }
    match &expr.kind {
        AstExprKind::Name(name) if env.places.contains_key(name) => {
            if let Some(targets) = env.places.get(name) {
                env.escaped.extend(targets.iter().cloned());
            }
            boundary(
                result,
                file,
                expr.range,
                "resource value escapes through an unsupported expression",
            );
        }
        AstExprKind::Tuple(items) => {
            for item in items {
                consume_expr(item, env, file, contracts, functions, result, depth + 1);
            }
        }
        AstExprKind::Other
        | AstExprKind::Attribute(_, _)
        | AstExprKind::Subscript(_, _)
        | AstExprKind::DepthLimit
            if !env.places.is_empty() =>
        {
            weaken(
                env,
                env.places
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            boundary(
                result,
                file,
                expr.range,
                "unsupported expression may affect or use a tracked resource",
            );
        }
        _ => {}
    }
}

fn execute(
    statements: &[FlowStatement],
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    for statement in statements {
        if !env.alive {
            break;
        }
        match statement {
            FlowStatement::Assign { name, value, range } => {
                evaluate_call_arguments(value, env, file, contracts, functions, result, depth);
                if let AstExprKind::Call { function, .. } = &value.kind {
                    if let Some(class) = expr_name(function) {
                        if contracts.iter().any(|contract| {
                            contract.display_name == class
                                && contract.lifecycle.is_some()
                                && !env.shadowed_classes.contains(class)
                        }) {
                            let object = format!("{}:{}:{}", file.path, range.start, class);
                            env.objects.insert(object.clone(), OPEN);
                            env.kinds.insert(object.clone(), class.to_owned());
                            env.places.insert(name.clone(), BTreeSet::from([object]));
                            env.shadowed_classes.remove(name);
                            continue;
                        }
                    }
                }
                if let AstExprKind::Call {
                    arguments,
                    keywords,
                    ..
                } = &value.kind
                {
                    let may_return_resource = arguments
                        .iter()
                        .chain(keywords.iter().map(|(_, value)| value))
                        .any(|argument| {
                            expr_name(argument).is_some_and(|name| env.places.contains_key(name))
                        });
                    if may_return_resource {
                        env.places.remove(name);
                        consume_expr(value, env, file, contracts, functions, result, depth);
                        boundary(
                            result,
                            file,
                            *range,
                            "resource-valued call result is not tracked",
                        );
                        continue;
                    }
                }
                if let Some(source) = expr_name(value) {
                    if let Some(objects) = env.places.get(source).cloned() {
                        env.places.insert(name.clone(), objects);
                        if env.unknown_places.contains(source) {
                            env.unknown_places.insert(name.clone());
                        } else {
                            env.unknown_places.remove(name);
                        }
                    } else {
                        env.places.remove(name);
                        env.unknown_places.remove(name);
                    }
                } else {
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                    consume_expr(value, env, file, contracts, functions, result, depth);
                }
                env.shadowed_classes.insert(name.clone());
            }
            FlowStatement::Expression { value, .. } => {
                consume_expr(value, env, file, contracts, functions, result, depth)
            }
            FlowStatement::Pass { .. } => {}
            FlowStatement::If {
                test,
                body,
                alternate,
                ..
            } => {
                consume_expr(test, env, file, contracts, functions, result, depth);
                let before = env.clone();
                let mut left = before.clone();
                left.alive = true;
                let mut right = before;
                right.alive = true;
                execute(body, &mut left, file, contracts, functions, result, depth);
                execute(
                    alternate, &mut right, file, contracts, functions, result, depth,
                );
                *env = match (left.alive, right.alive) {
                    (true, true) => merge(&left, &right),
                    (true, false) => left,
                    (false, true) => right,
                    (false, false) => {
                        let mut joined = merge(&left, &right);
                        joined.alive = false;
                        joined
                    }
                };
            }
            FlowStatement::Return { value, .. } => {
                if let Some(value) = value {
                    consume_expr(value, env, file, contracts, functions, result, depth);
                }
                env.alive = false;
            }
            FlowStatement::Unsupported { range } => {
                weaken(
                    env,
                    env.places
                        .values()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter(),
                );
                boundary(
                    result,
                    file,
                    *range,
                    "unsupported flow weakens tracked resources",
                );
            }
        }
    }
}

pub(crate) fn check_bodies(
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
) {
    let mut project_functions: BTreeMap<(String, String), (Vec<String>, Vec<FlowStatement>)> =
        BTreeMap::new();
    for file in files {
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        for declaration in declarations {
            match declaration {
                Declaration::Function {
                    name,
                    parameters,
                    body,
                    is_async,
                    ..
                } => {
                    if *is_async {
                        continue;
                    }
                    project_functions.insert(
                        (file.module.clone(), name.clone()),
                        (
                            parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            body.clone(),
                        ),
                    );
                }
                Declaration::Class { name, methods, .. } => {
                    for method in methods {
                        if method.is_async {
                            continue;
                        }
                        project_functions.insert(
                            (file.module.clone(), format!("@{name}.{}", method.name)),
                            (method.parameters.clone(), method.body.clone()),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    for file in files {
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        let mut contracts: Vec<_> = result
            .contracts
            .iter()
            .filter(|contract| contract.source.file == file.path)
            .cloned()
            .collect();
        let mut functions: FunctionMap = project_functions
            .iter()
            .filter(|((module, _), _)| module == &file.module)
            .map(|((_, name), body)| (name.clone(), body.clone()))
            .collect();
        for declaration in declarations {
            if let Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } = declaration
            {
                for import in names {
                    let local = import.alias.as_deref().unwrap_or(&import.name);
                    if let Some(function) = resolve_imported_function(
                        module,
                        &import.name,
                        files,
                        summaries,
                        &project_functions,
                        &mut BTreeSet::new(),
                    ) {
                        functions.insert(local.to_owned(), function.clone());
                    }
                    let imported_methods: Vec<_> = project_functions
                        .iter()
                        .filter(|((provider, key), _)| {
                            let prefix = format!("@{}.", import.name);
                            provider == module && key.starts_with(&prefix)
                        })
                        .map(|((_, key), body)| {
                            (
                                format!("@{}{}", local, &key[import.name.len() + 1..]),
                                body.clone(),
                            )
                        })
                        .collect();
                    functions.extend(imported_methods);
                    for mut contract in resolve_imported_contracts(
                        module,
                        &import.name,
                        files,
                        summaries,
                        result,
                        &mut BTreeSet::new(),
                    ) {
                        if let Some((owner_module, identity)) = contract
                            .base_symbol
                            .split('#')
                            .next()
                            .and_then(|qualified| qualified.rsplit_once('.'))
                        {
                            let prefix = format!("@{identity}.");
                            let methods: Vec<_> = project_functions
                                .iter()
                                .filter(|((provider, key), _)| {
                                    provider == owner_module && key.starts_with(&prefix)
                                })
                                .map(|((_, key), body)| {
                                    (
                                        format!("@{}{}", local, &key[identity.len() + 1..]),
                                        body.clone(),
                                    )
                                })
                                .collect();
                            functions.extend(methods);
                        }
                        contract.display_name = local.to_owned();
                        contracts.push(contract);
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
        let mut annotation_imports = BTreeMap::new();
        for declaration in declarations {
            match declaration {
                Declaration::ImportFrom {
                    module,
                    level: 0,
                    names,
                    ..
                } => {
                    for import in names {
                        annotation_imports.insert(
                            import.alias.clone().unwrap_or_else(|| import.name.clone()),
                            format!("{module}.{}", import.name),
                        );
                    }
                }
                Declaration::Import { names, .. } => {
                    for import in names {
                        if let Some(alias) = &import.alias {
                            annotation_imports.insert(alias.clone(), import.name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        for declaration in declarations {
            let Declaration::Function {
                name,
                parameters,
                body,
                is_async,
                range,
                ..
            } = declaration
            else {
                continue;
            };
            let mut env = State {
                alive: true,
                ..State::default()
            };
            for parameter in parameters {
                let Some(annotation) = &parameter.annotation else {
                    continue;
                };
                let parameter_name = format!("{name}.{}", parameter.name);
                let Some(parameter_contract) = result.contracts.iter().find(|contract| {
                    contract.display_name == parameter_name
                        && contract.access == "shared"
                        && contract.lifecycle.is_some()
                }) else {
                    continue;
                };
                let kind = borrowed_resource(annotation, &annotation_imports).or_else(|| {
                    result
                        .contracts
                        .iter()
                        .find(|contract| {
                            contract.base_symbol == parameter_contract.base_symbol
                                && contract.lifecycle.is_some()
                                && !contract.display_name.contains('.')
                        })
                        .map(|contract| contract.display_name.clone())
                });
                let Some(kind) = kind else {
                    continue;
                };
                if !contracts
                    .iter()
                    .any(|contract| contract.display_name == kind && contract.lifecycle.is_some())
                {
                    continue;
                }
                let object = format!("{}:{}:{}", file.path, range.start, parameter.name);
                env.objects.insert(object.clone(), OPEN);
                env.kinds.insert(object.clone(), kind.clone());
                env.places
                    .insert(parameter.name.clone(), BTreeSet::from([object.clone()]));
                env.borrowed.insert(object);
            }
            if *is_async {
                boundary(result, file, *range, "async function body is not checked");
                continue;
            }
            execute(body, &mut env, file, &contracts, &functions, result, 0);
            let _ = name;
        }
        let mut module = State {
            alive: true,
            ..State::default()
        };
        if let Some(summary) = summaries.get(&file.path) {
            execute(
                &summary.module_body,
                &mut module,
                file,
                &contracts,
                &functions,
                result,
                0,
            );
        }
    }
}

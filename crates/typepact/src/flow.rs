use crate::{
    frontend::{AstExpr, AstExprKind, Declaration, FlowStatement, SourceRange},
    Contract, Diagnostic, Inspection, SourceFile, Span,
};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const MAX_CALL_DEPTH: usize = 8;
const MAX_CONTROL_FLOW_DEPTH: usize = 128;
const MAX_ALIAS_TARGETS: usize = 64;
const MAX_FLOW_OBJECTS: usize = 4_096;
pub(crate) const MAX_FLOW_STEPS: usize = 100_000;
thread_local! {
    static FLOW_STEPS: Cell<usize> = const { Cell::new(0) };
    static FLOW_STEP_LIMIT: Cell<usize> = const { Cell::new(MAX_FLOW_STEPS) };
    static FLOW_BUDGET_REPORTED: Cell<bool> = const { Cell::new(false) };
}
type CallKeyword = (Option<String>, AstExpr);
type MethodCall<'a> = (&'a AstExpr, &'a str, &'a [AstExpr], &'a [CallKeyword]);
#[derive(Clone)]
struct FunctionBody {
    file: SourceFile,
    name: String,
    parameters: Vec<String>,
    statements: Vec<FlowStatement>,
    supported_binding: bool,
}

struct FlowContext<'a> {
    contracts: &'a [Contract],
    functions: &'a FunctionMap,
    result: &'a mut Inspection,
}

struct MethodInvocation<'a> {
    definition: &'a FunctionBody,
    object: &'a str,
    initial_mask: u8,
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
    parent: &'a State,
    depth: usize,
    call_key: String,
}

type ProjectFunctionMap = BTreeMap<(String, String), FunctionBody>;

#[derive(Clone, Default)]
struct State {
    places: BTreeMap<String, BTreeSet<String>>,
    unknown_places: BTreeSet<String>,
    shadowed_classes: BTreeSet<String>,
    shadowed_functions: BTreeSet<String>,
    objects: BTreeMap<String, u8>,
    kinds: BTreeMap<String, String>,
    borrowed: BTreeSet<String>,
    borrowed_origins: BTreeMap<String, Span>,
    escaped: BTreeSet<String>,
    call_stack: BTreeSet<String>,
    close_origins: BTreeMap<String, Span>,
    alive: bool,
}

#[derive(Default)]
struct JoinEffects {
    aliases_over_budget: Vec<String>,
    possibly_unbound: Vec<String>,
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

fn expression_may_return_resource(expression: &AstExpr) -> bool {
    match &expression.kind {
        AstExprKind::NoneLiteral | AstExprKind::String(_) => false,
        AstExprKind::Tuple(items) => items.iter().any(expression_may_return_resource),
        AstExprKind::Name(_)
        | AstExprKind::Attribute(_, _)
        | AstExprKind::Subscript(_, _)
        | AstExprKind::Call { .. }
        | AstExprKind::Other
        | AstExprKind::DepthLimit => true,
    }
}

fn statements_may_return_resource(statements: &[FlowStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        FlowStatement::Return {
            value: Some(value), ..
        } => expression_may_return_resource(value),
        FlowStatement::If {
            body, alternate, ..
        } => statements_may_return_resource(body) || statements_may_return_resource(alternate),
        FlowStatement::Unsupported { .. } => true,
        _ => false,
    })
}

fn call_may_return_resource(
    expression: &AstExpr,
    env: &State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
) -> bool {
    if let Some((receiver, method, _, _)) = called_method(expression) {
        if let Some(name) = expr_name(receiver) {
            if let Some(object) = env
                .places
                .get(name)
                .and_then(|objects| objects.iter().next())
            {
                let kind = env
                    .kinds
                    .get(object)
                    .map(String::as_str)
                    .unwrap_or_default();
                let is_close = contracts.iter().any(|contract| {
                    contract.display_name == kind
                        && contract
                            .lifecycle
                            .as_ref()
                            .is_some_and(|lifecycle| lifecycle.closed_by == method)
                });
                let method_key = format!("@{kind}.{method}");
                let definition = contracts
                    .iter()
                    .find(|contract| contract.display_name == kind)
                    .and_then(|contract| {
                        contract
                            .base_symbol
                            .split('#')
                            .next()
                            .and_then(|base| base.rsplit_once('.'))
                    })
                    .and_then(|(module, base)| {
                        functions.get(&format!("{module}::@{base}.{method}"))
                    })
                    .or_else(|| functions.get(&format!("{}::{method_key}", file.module)))
                    .or_else(|| {
                        functions
                            .get(&method_key)
                            .filter(|definition| definition.file.module == file.module)
                    });
                if is_close {
                    return definition.is_none_or(|definition| {
                        !definition.supported_binding
                            || statements_may_return_resource(&definition.statements)
                    });
                }
                return definition.is_none_or(|definition| {
                    !definition.supported_binding
                        || statements_may_return_resource(&definition.statements)
                });
            }
        }
        return true;
    }

    let AstExprKind::Call { function, .. } = &expression.kind else {
        return false;
    };
    let Some(callee) = expr_name(function) else {
        return true;
    };
    let definition = functions
        .get(&format!("{}::{callee}", file.module))
        .or_else(|| {
            functions
                .get(callee)
                .filter(|definition| definition.file.module == file.module)
        });
    definition.is_none_or(|definition| statements_may_return_resource(&definition.statements))
}

fn merge(left: &State, right: &State) -> (State, JoinEffects) {
    let mut joined = left.clone();
    let mut effects = JoinEffects::default();
    joined
        .unknown_places
        .extend(right.unknown_places.iter().cloned());
    joined
        .shadowed_classes
        .extend(right.shadowed_classes.iter().cloned());
    joined
        .shadowed_functions
        .extend(right.shadowed_functions.iter().cloned());
    let places: BTreeSet<_> = left
        .places
        .keys()
        .chain(right.places.keys())
        .cloned()
        .collect();
    for place in places {
        let left_targets = left.places.get(&place);
        let right_targets = right.places.get(&place);
        let mut targets = left_targets.cloned().unwrap_or_default();
        targets.extend(right_targets.into_iter().flatten().cloned());
        if left_targets.is_some() != right_targets.is_some() {
            joined.unknown_places.insert(place.clone());
            let missing_path_is_unbound = if left_targets.is_some() {
                !right.unknown_places.contains(&place)
            } else {
                !left.unknown_places.contains(&place)
            };
            if missing_path_is_unbound {
                effects.possibly_unbound.push(place.clone());
            }
        }
        if targets.len() > MAX_ALIAS_TARGETS {
            joined.places.remove(&place);
            joined.unknown_places.insert(place.clone());
            effects.aliases_over_budget.push(place);
            for object in targets {
                *joined.objects.entry(object).or_default() |= UNKNOWN;
            }
        } else {
            joined.places.insert(place, targets);
        }
    }
    for (object, state) in &right.objects {
        *joined.objects.entry(object.clone()).or_default() |= *state;
    }
    joined.kinds.extend(right.kinds.clone());
    joined.borrowed.extend(right.borrowed.iter().cloned());
    for (object, origin) in &right.borrowed_origins {
        joined
            .borrowed_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.escaped.extend(right.escaped.iter().cloned());
    for (object, origin) in &right.close_origins {
        joined
            .close_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.alive |= right.alive;
    (joined, effects)
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

fn take_flow_step(
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let within_budget = FLOW_STEPS.with(|steps| {
        let current = steps.get();
        if current >= FLOW_STEP_LIMIT.with(Cell::get) {
            false
        } else {
            steps.set(current + 1);
            true
        }
    });
    if within_budget {
        return true;
    }
    weaken(
        env,
        env.places
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter(),
    );
    let first = FLOW_BUDGET_REPORTED.with(|reported| {
        if reported.get() {
            false
        } else {
            reported.set(true);
            true
        }
    });
    if first {
        boundary(
            result,
            file,
            range,
            "lifecycle flow work budget exceeded; tracked resources are unknown",
        );
    }
    false
}

pub(crate) fn reset_flow_budget(limit: usize) {
    FLOW_STEPS.with(|steps| steps.set(0));
    FLOW_STEP_LIMIT.with(|budget| budget.set(limit));
    FLOW_BUDGET_REPORTED.with(|reported| reported.set(false));
}

fn weaken(state: &mut State, objects: impl Iterator<Item = String>) {
    for object in objects {
        *state.objects.entry(object).or_default() |= UNKNOWN;
    }
    for object in state.escaped.clone() {
        *state.objects.entry(object).or_default() |= UNKNOWN;
    }
}

type FunctionMap = BTreeMap<String, FunctionBody>;

fn assigned_names(statements: &[FlowStatement]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_assigned_names(statements, &mut names);
    names
}

fn collect_assigned_names(statements: &[FlowStatement], names: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            FlowStatement::Assign { name, .. } => {
                names.insert(name.clone());
            }
            FlowStatement::If {
                body, alternate, ..
            } => {
                collect_assigned_names(body, names);
                collect_assigned_names(alternate, names);
            }
            _ => {}
        }
    }
}

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
        if let Some(origin) = parent.borrowed_origins.get(object) {
            child
                .borrowed_origins
                .insert(object.clone(), origin.clone());
        }
        if let Some(origin) = parent.close_origins.get(object) {
            child.close_origins.insert(object.clone(), origin.clone());
        }
    }
}

fn check_borrowed_precondition(
    definition: &FunctionBody,
    parameter: &str,
    argument: &AstExpr,
    env: &State,
    call_site: Span,
    contracts: &[Contract],
    result: &mut Inspection,
) {
    let Some(name) = expr_name(argument) else {
        return;
    };
    let Some(targets) = env.places.get(name) else {
        return;
    };
    let requires_open_resource = contracts.iter().any(|contract| {
        contract.display_name == format!("{}.{parameter}", definition.name)
            && contract.source.file == definition.file.path
            && contract.access == "shared"
            && contract.lifecycle.is_some()
    });
    if !requires_open_resource {
        return;
    }
    let has_open = targets
        .iter()
        .any(|object| env.objects.get(object).copied().unwrap_or(UNKNOWN) & (OPEN | UNKNOWN) != 0)
        || env.unknown_places.contains(name);
    let has_closed = targets
        .iter()
        .any(|object| env.objects.get(object).copied().unwrap_or(UNKNOWN) & CLOSED != 0);
    if has_closed {
        report(
            result,
            "TP1001",
            "Borrowed call requires an open resource",
            call_site,
            targets
                .iter()
                .filter_map(|object| env.close_origins.get(object).cloned())
                .collect(),
            if has_open { "possible" } else { "definite" },
        );
    }
}

fn execute_method_body(invocation: MethodInvocation<'_>, context: FlowContext<'_>) -> State {
    let MethodInvocation {
        definition,
        object,
        initial_mask,
        arguments,
        keywords,
        parent,
        depth,
        call_key,
    } = invocation;
    let mut child = State {
        alive: true,
        call_stack: parent.call_stack.clone(),
        ..State::default()
    };
    child.call_stack.insert(call_key);
    child
        .shadowed_functions
        .extend(definition.parameters.iter().cloned());
    child
        .shadowed_functions
        .extend(assigned_names(&definition.statements));
    let receiver = definition
        .parameters
        .first()
        .cloned()
        .unwrap_or_else(|| "self".into());
    child
        .places
        .insert(receiver, BTreeSet::from([object.to_owned()]));
    copy_object_metadata(&mut child, parent, &BTreeSet::from([object.to_owned()]));
    child.objects.insert(object.to_owned(), initial_mask);
    for (parameter, argument) in definition.parameters.iter().skip(1).zip(arguments) {
        if let Some(actual) = expr_name(argument) {
            if let Some(targets) = parent.places.get(actual) {
                child.places.insert(parameter.clone(), targets.clone());
                copy_object_metadata(&mut child, parent, targets);
                if parent.unknown_places.contains(actual) {
                    child.unknown_places.insert(parameter.clone());
                }
            }
        }
    }
    for (name, argument) in keywords {
        let Some(index) = name.as_ref().and_then(|name| {
            definition
                .parameters
                .iter()
                .position(|parameter| parameter == name)
        }) else {
            continue;
        };
        if let Some(actual) = expr_name(argument) {
            if let Some(targets) = parent.places.get(actual) {
                child
                    .places
                    .insert(definition.parameters[index].clone(), targets.clone());
                copy_object_metadata(&mut child, parent, targets);
                if parent.unknown_places.contains(actual) {
                    child
                        .unknown_places
                        .insert(definition.parameters[index].clone());
                }
            }
        }
    }
    let returned = execute(
        &definition.statements,
        &mut child,
        &definition.file,
        context.contracts,
        context.functions,
        context.result,
        depth + 1,
    );
    for exit in returned {
        for (changed_object, value) in exit.objects {
            *child.objects.entry(changed_object).or_default() |= value;
        }
        child.close_origins.extend(exit.close_origins);
        child.escaped.extend(exit.escaped);
    }
    child
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
    let symbol = format!("{module}.{name}");
    let direct: Vec<_> = result
        .contracts
        .iter()
        .filter(|contract| {
            contract.lifecycle.is_some()
                && (contract.base_symbol == symbol
                    || contract.base_symbol.starts_with(&format!("{symbol}#")))
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
) -> Option<FunctionBody> {
    if !visited.insert((module.to_owned(), name.to_owned())) {
        return None;
    }
    let provider = files.iter().find(|file| file.module == module)?;
    let provider_summary = summaries.get(&provider.path)?;
    if assigned_names(&provider_summary.module_body).contains(name) {
        return None;
    }
    if let Some(function) = functions.get(&(module.to_owned(), name.to_owned())) {
        return Some(function.clone());
    }
    let declarations = provider_summary.declarations.as_ref()?;
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
    if !take_flow_step(env, file, expr.range, result) {
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
                    let kind = env.kinds.get(object).cloned().unwrap_or_default();
                    let method_key = format!("@{kind}.{method}");
                    let definition = contracts
                        .iter()
                        .find(|contract| contract.display_name == kind)
                        .and_then(|contract| {
                            contract
                                .base_symbol
                                .split('#')
                                .next()
                                .and_then(|base| base.rsplit_once('.'))
                        })
                        .and_then(|(module, base)| {
                            functions.get(&format!("{module}::@{base}.{method}"))
                        })
                        .or_else(|| functions.get(&format!("{}::{method_key}", file.module)))
                        .or_else(|| {
                            functions
                                .get(&method_key)
                                .filter(|definition| definition.file.module == file.module)
                        });
                    if definition.is_some_and(|definition| !definition.supported_binding) {
                        let mut affected = vec![object.clone()];
                        for argument in arguments
                            .iter()
                            .chain(keywords.iter().map(|(_, value)| value))
                            .filter_map(expr_name)
                        {
                            if let Some(targets) = env.places.get(argument) {
                                affected.extend(targets.iter().cloned());
                            }
                        }
                        weaken(env, affected.into_iter());
                        boundary(
                            result,
                            file,
                            expr.range,
                            "decorated method binding is not modeled",
                        );
                        continue;
                    }
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
                            env.borrowed_origins
                                .get(object)
                                .cloned()
                                .into_iter()
                                .collect(),
                            "definite",
                        );
                    }
                    if is_close && mask & CLOSED != 0 {
                        boundary(
                            result,
                            file,
                            expr.range,
                            "repeat-close behavior is not modeled",
                        );
                    }
                    if let Some(definition) = definition {
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
                        let initial_mask = env.objects.get(object).copied().unwrap_or(mask);
                        let child = execute_method_body(
                            MethodInvocation {
                                definition,
                                object,
                                initial_mask,
                                arguments,
                                keywords,
                                parent: env,
                                depth,
                                call_key: method_key,
                            },
                            FlowContext {
                                contracts,
                                functions,
                                result,
                            },
                        );
                        for (changed_object, value) in child.objects {
                            if targets.len() == 1 && !env.unknown_places.contains(name) {
                                env.objects.insert(changed_object, value);
                            } else {
                                *env.objects.entry(changed_object).or_default() |= value;
                            }
                        }
                        if is_close {
                            env.close_origins
                                .insert(object.clone(), span(file, expr.range));
                            if targets.len() == 1 && !env.unknown_places.contains(name) {
                                env.objects.insert(object.clone(), CLOSED);
                            } else {
                                *env.objects.entry(object.clone()).or_default() |= CLOSED;
                            }
                        }
                        env.close_origins.extend(child.close_origins);
                        env.escaped.extend(child.escaped);
                    } else if is_close {
                        env.close_origins
                            .insert(object.clone(), span(file, expr.range));
                        if targets.len() == 1 && !env.unknown_places.contains(name) {
                            env.objects.insert(object.clone(), CLOSED);
                        } else {
                            *env.objects.entry(object.clone()).or_default() |= CLOSED;
                        }
                    } else if mask & OPEN != 0 && !contracts.is_empty() {
                        let mut affected = vec![object.clone()];
                        for argument in arguments
                            .iter()
                            .chain(keywords.iter().map(|(_, value)| value))
                            .filter_map(expr_name)
                        {
                            if let Some(targets) = env.places.get(argument) {
                                affected.extend(targets.iter().cloned());
                            }
                        }
                        weaken(env, affected.into_iter());
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
            let scoped_name = format!("{}::{callee}", file.module);
            let known_function = if env.shadowed_functions.contains(callee) {
                None
            } else {
                functions.get(&scoped_name).or_else(|| {
                    functions
                        .get(callee)
                        .filter(|definition| definition.file.module == file.module)
                })
            };
            if let Some(definition) = known_function {
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
                child
                    .shadowed_functions
                    .extend(definition.parameters.iter().cloned());
                child
                    .shadowed_functions
                    .extend(assigned_names(&definition.statements));
                for (parameter, argument) in definition.parameters.iter().zip(arguments) {
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
                    if let Some(index) = definition
                        .parameters
                        .iter()
                        .position(|parameter| parameter == name)
                    {
                        if let Some(actual) = expr_name(argument) {
                            if let Some(targets) = env.places.get(actual) {
                                child
                                    .places
                                    .insert(definition.parameters[index].clone(), targets.clone());
                                copy_object_metadata(&mut child, env, targets);
                                if env.unknown_places.contains(actual) {
                                    child
                                        .unknown_places
                                        .insert(definition.parameters[index].clone());
                                }
                            }
                        }
                    }
                }
                let call_site = span(file, expr.range);
                for (parameter, argument) in definition.parameters.iter().zip(arguments) {
                    check_borrowed_precondition(
                        definition,
                        parameter,
                        argument,
                        env,
                        call_site.clone(),
                        contracts,
                        result,
                    );
                }
                for (name, argument) in keywords {
                    if let Some(parameter) = name.as_ref().filter(|name| {
                        definition
                            .parameters
                            .iter()
                            .any(|parameter| parameter == *name)
                    }) {
                        check_borrowed_precondition(
                            definition,
                            parameter,
                            argument,
                            env,
                            call_site.clone(),
                            contracts,
                            result,
                        );
                    }
                }
                let diagnostic_start = result.diagnostics.len();
                let returned = execute(
                    &definition.statements,
                    &mut child,
                    &definition.file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                );
                for exit in returned {
                    for (object, state) in exit.objects {
                        *child.objects.entry(object).or_default() |= state;
                    }
                    child.close_origins.extend(exit.close_origins);
                    child.escaped.extend(exit.escaped);
                }
                let body_diagnostics = result.diagnostics.split_off(diagnostic_start);
                for mut diagnostic in body_diagnostics {
                    if diagnostic.code == "TP1001" {
                        diagnostic.primary = call_site.clone();
                        if let Some(existing) = result.diagnostics.iter_mut().find(|existing| {
                            existing.code == diagnostic.code
                                && existing.primary == diagnostic.primary
                                && existing.certainty == diagnostic.certainty
                        }) {
                            for related in diagnostic.related {
                                if !existing.related.contains(&related) {
                                    existing.related.push(related);
                                }
                            }
                            continue;
                        }
                    }
                    result.diagnostics.push(diagnostic);
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
) -> Vec<State> {
    let mut returned_states = Vec::new();
    for statement in statements {
        if !env.alive {
            break;
        }
        if !take_flow_step(env, file, statement_range(statement), result) {
            break;
        }
        match statement {
            FlowStatement::Assign { name, value, range } => {
                if let AstExprKind::Call { function, .. } = &value.kind {
                    if let Some(class) = expr_name(function) {
                        if contracts.iter().any(|contract| {
                            contract.display_name == class
                                && contract.lifecycle.is_some()
                                && !env.shadowed_classes.contains(class)
                        }) {
                            evaluate_call_arguments(
                                value, env, file, contracts, functions, result, depth,
                            );
                            let new_definition = contracts
                                .iter()
                                .find(|contract| contract.display_name == class)
                                .and_then(|contract| {
                                    contract
                                        .base_symbol
                                        .split('#')
                                        .next()
                                        .and_then(|base| base.rsplit_once('.'))
                                })
                                .and_then(|(module, base)| {
                                    functions.get(&format!("{module}::@{base}.__new__"))
                                })
                                .or_else(|| {
                                    functions.get(&format!("{}::@{class}.__new__", file.module))
                                });
                            if new_definition.is_some() {
                                weaken(
                                    env,
                                    env.places
                                        .values()
                                        .flatten()
                                        .cloned()
                                        .collect::<Vec<_>>()
                                        .into_iter(),
                                );
                                env.places.remove(name);
                                env.unknown_places.insert(name.clone());
                                env.shadowed_classes.insert(name.clone());
                                env.shadowed_functions.insert(name.clone());
                                boundary(
                                    result,
                                    file,
                                    *range,
                                    "custom __new__ allocation effects are not modeled",
                                );
                                continue;
                            }
                            let object = format!("{}:{}:{}", file.path, range.start, class);
                            if !env.objects.contains_key(&object)
                                && env.objects.len() >= MAX_FLOW_OBJECTS
                            {
                                env.places.remove(name);
                                env.unknown_places.insert(name.clone());
                                env.shadowed_classes.insert(name.clone());
                                boundary(
                                    result,
                                    file,
                                    *range,
                                    "tracked resource object budget exceeded; lifecycle state is unknown",
                                );
                                continue;
                            }
                            env.objects.insert(object.clone(), OPEN);
                            env.kinds.insert(object.clone(), class.to_owned());
                            if let AstExprKind::Call {
                                arguments,
                                keywords,
                                ..
                            } = &value.kind
                            {
                                let init_key = format!("@{class}.__init__");
                                let definition = contracts
                                    .iter()
                                    .find(|contract| contract.display_name == class)
                                    .and_then(|contract| {
                                        contract
                                            .base_symbol
                                            .split('#')
                                            .next()
                                            .and_then(|base| base.rsplit_once('.'))
                                    })
                                    .and_then(|(module, base)| {
                                        functions.get(&format!("{module}::@{base}.__init__"))
                                    })
                                    .or_else(|| {
                                        functions.get(&format!("{}::{init_key}", file.module))
                                    });
                                if let Some(definition) = definition {
                                    let child = execute_method_body(
                                        MethodInvocation {
                                            definition,
                                            object: &object,
                                            initial_mask: OPEN,
                                            arguments,
                                            keywords,
                                            parent: env,
                                            depth,
                                            call_key: init_key,
                                        },
                                        FlowContext {
                                            contracts,
                                            functions,
                                            result,
                                        },
                                    );
                                    for (changed_object, state) in child.objects {
                                        env.objects.insert(changed_object, state);
                                    }
                                    env.close_origins.extend(child.close_origins);
                                    env.escaped.extend(child.escaped);
                                }
                            }
                            env.places
                                .insert(name.clone(), BTreeSet::from([object.clone()]));
                            env.unknown_places.remove(name);
                            env.shadowed_classes.remove(name);
                            env.shadowed_functions.insert(name.clone());
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
                    let passes_resource = arguments
                        .iter()
                        .chain(keywords.iter().map(|(_, value)| value))
                        .any(|argument| {
                            expr_name(argument).is_some_and(|name| env.places.contains_key(name))
                        })
                        || called_method(value).is_some_and(|(receiver, _, _, _)| {
                            expr_name(receiver).is_some_and(|name| env.places.contains_key(name))
                        });
                    let may_return_resource = passes_resource
                        && call_may_return_resource(value, env, file, contracts, functions);
                    if may_return_resource {
                        consume_expr(value, env, file, contracts, functions, result, depth);
                        env.places.remove(name);
                        env.unknown_places.insert(name.clone());
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
                    } else if env.unknown_places.contains(source) {
                        env.places.remove(name);
                        env.unknown_places.insert(name.clone());
                    } else {
                        env.places.remove(name);
                        env.unknown_places.remove(name);
                    }
                } else {
                    consume_expr(value, env, file, contracts, functions, result, depth);
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                }
                env.shadowed_functions.insert(name.clone());
                let assignment_span = span(file, *range);
                let defines_resource_view = contracts.iter().any(|contract| {
                    contract.display_name == *name
                        && contract.lifecycle.is_some()
                        && contract.source.file == file.path
                        && contract.source.line == assignment_span.line
                        && contract.source.column >= assignment_span.column
                        && (contract.source.end_line < assignment_span.end_line
                            || contract.source.end_column <= assignment_span.end_column)
                });
                if defines_resource_view {
                    env.shadowed_classes.remove(name);
                } else {
                    env.shadowed_classes.insert(name.clone());
                }
            }
            FlowStatement::Expression { value, .. } => {
                consume_expr(value, env, file, contracts, functions, result, depth)
            }
            FlowStatement::Pass { .. } => {}
            FlowStatement::If {
                test,
                body,
                alternate,
                range,
            } => {
                consume_expr(test, env, file, contracts, functions, result, depth);
                if depth >= MAX_CONTROL_FLOW_DEPTH {
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
                        "control-flow nesting budget exceeded; tracked resources are unknown",
                    );
                    continue;
                }
                let before = env.clone();
                let mut left = before.clone();
                left.alive = true;
                let mut right = before;
                right.alive = true;
                let left_returns = execute(
                    body,
                    &mut left,
                    file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                );
                let right_returns = execute(
                    alternate,
                    &mut right,
                    file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                );
                returned_states.extend(left_returns);
                returned_states.extend(right_returns);
                let reachable = left.alive || right.alive;
                let (joined, effects) = match (left.alive, right.alive) {
                    (true, true) => merge(&left, &right),
                    (true, false) => (left, JoinEffects::default()),
                    (false, true) => (right, JoinEffects::default()),
                    (false, false) => {
                        let (mut joined, effects) = merge(&left, &right);
                        joined.alive = false;
                        (joined, effects)
                    }
                };
                *env = joined;
                if reachable {
                    for place in effects.possibly_unbound {
                        boundary(
                            result,
                            file,
                            *range,
                            &format!("local resource alias {place} may be unbound on one branch"),
                        );
                    }
                    for place in effects.aliases_over_budget {
                        boundary(
                            result,
                            file,
                            *range,
                            &format!(
                                "alias target budget exceeded for {place}; affected resource states are unknown"
                            ),
                        );
                    }
                }
            }
            FlowStatement::Return { value, .. } => {
                if let Some(value) = value {
                    consume_expr(value, env, file, contracts, functions, result, depth);
                }
                let mut returned = env.clone();
                returned.alive = true;
                returned_states.push(returned);
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
    returned_states
}

fn statement_range(statement: &FlowStatement) -> SourceRange {
    match statement {
        FlowStatement::Pass { range }
        | FlowStatement::Assign { range, .. }
        | FlowStatement::Expression { range, .. }
        | FlowStatement::If { range, .. }
        | FlowStatement::Return { range, .. }
        | FlowStatement::Unsupported { range } => *range,
    }
}

fn check_bodies_impl(
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
    only_file: Option<&str>,
) {
    let mut project_functions = ProjectFunctionMap::new();
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
                    decorators,
                    ..
                } => {
                    if *is_async || !decorators.is_empty() {
                        continue;
                    }
                    project_functions.insert(
                        (file.module.clone(), name.clone()),
                        FunctionBody {
                            file: file.clone(),
                            name: name.clone(),
                            parameters: parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            statements: body.clone(),
                            supported_binding: true,
                        },
                    );
                }
                Declaration::Class { name, methods, .. } => {
                    for method in methods {
                        if method.is_async {
                            continue;
                        }
                        project_functions.insert(
                            (file.module.clone(), format!("@{name}.{}", method.name)),
                            FunctionBody {
                                file: file.clone(),
                                name: format!("{name}.{}", method.name),
                                parameters: method.parameters.clone(),
                                statements: method.body.clone(),
                                supported_binding: !method.decorated,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }
    let mut qualified_functions: FunctionMap = project_functions
        .iter()
        .map(|((module, name), body)| (format!("{module}::{name}"), body.clone()))
        .collect();
    for provider in files {
        let Some(declarations) = summaries
            .get(&provider.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        for declaration in declarations {
            let Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } = declaration
            else {
                continue;
            };
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
                    qualified_functions.insert(format!("{}::{local}", provider.module), function);
                }
            }
        }
    }
    for provider in files {
        let Some(summary) = summaries.get(&provider.path) else {
            continue;
        };
        let rebound_names = assigned_names(&summary.module_body);
        let scope_prefix = format!("{}::", provider.module);
        qualified_functions.retain(|key, _| {
            !key.strip_prefix(&scope_prefix)
                .is_some_and(|name| rebound_names.contains(name))
        });
    }
    for file in files {
        if only_file.is_some_and(|path| file.path != path) {
            continue;
        }
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
        contracts.extend(
            result
                .contracts
                .iter()
                .filter(|contract| {
                    contract.access == "shared"
                        && contract.lifecycle.is_some()
                        && contract.display_name.contains('.')
                })
                .cloned(),
        );
        let mut functions: FunctionMap = project_functions
            .iter()
            .filter(|((module, _), _)| module == &file.module)
            .map(|((_, name), body)| (name.clone(), body.clone()))
            .collect();
        functions.extend(qualified_functions.clone());
        let rebound_module_names = summaries
            .get(&file.path)
            .map(|summary| assigned_names(&summary.module_body))
            .unwrap_or_default();
        let scope_prefix = format!("{}::", file.module);
        functions.retain(|name, _| {
            !rebound_module_names.contains(name)
                && !name
                    .strip_prefix(&scope_prefix)
                    .is_some_and(|scoped| rebound_module_names.contains(scoped))
        });
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
                    if !rebound_module_names.contains(local) {
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
            env.shadowed_functions
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            env.shadowed_functions.extend(assigned_names(body));
            for parameter in parameters {
                let Some(annotation) = &parameter.annotation else {
                    continue;
                };
                let parameter_name = format!("{name}.{}", parameter.name);
                let Some(parameter_contract) = result.contracts.iter().find(|contract| {
                    contract.display_name == parameter_name && contract.lifecycle.is_some()
                }) else {
                    continue;
                };
                let kind = borrowed_resource(annotation, &annotation_imports)
                    .filter(|kind| {
                        contracts.iter().any(|contract| {
                            contract.display_name == *kind && contract.lifecycle.is_some()
                        })
                    })
                    .or_else(|| {
                        result
                            .contracts
                            .iter()
                            .find(|contract| {
                                contract.base_symbol == parameter_contract.base_symbol
                                    && contract.lifecycle.is_some()
                                    && !contract.display_name.contains('.')
                            })
                            .map(|contract| contract.display_name.clone())
                    })
                    .unwrap_or_else(|| parameter_contract.display_name.clone());
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
                if parameter_contract.access == "shared" {
                    env.borrowed.insert(object.clone());
                    env.borrowed_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                }
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

pub(crate) fn check_file_bodies(
    target_path: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
) {
    check_bodies_impl(files, summaries, result, Some(target_path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Contract, Coverage, Lifecycle, Provenance};

    fn inspection() -> Inspection {
        Inspection {
            schema_version: 1,
            assurance: "test".into(),
            contracts: Vec::new(),
            diagnostics: Vec::new(),
            coverage: Coverage {
                status: "complete-for-supported-subset".into(),
                reasons: Vec::new(),
                source_bytes: 0,
                declarations: 0,
            },
        }
    }

    #[test]
    fn alias_overflow_drops_exact_targets_and_weakens_all_candidates() {
        let left_targets: BTreeSet<_> = (0..MAX_ALIAS_TARGETS)
            .map(|index| format!("left-{index}"))
            .collect();
        let mut left = State::default();
        left.places.insert("selected".into(), left_targets.clone());
        for object in &left_targets {
            left.objects.insert(object.clone(), OPEN);
        }
        let mut right = State::default();
        right
            .places
            .insert("selected".into(), BTreeSet::from(["right".into()]));
        right.objects.insert("right".into(), OPEN);
        let all_targets: BTreeSet<_> = left_targets
            .iter()
            .cloned()
            .chain(["right".to_owned()])
            .collect();

        let (joined, effects) = merge(&left, &right);

        assert_eq!(effects.aliases_over_budget, ["selected"]);
        assert!(!joined.places.contains_key("selected"));
        assert!(joined.unknown_places.contains("selected"));
        assert!(all_targets.iter().all(|object| joined
            .objects
            .get(object)
            .is_some_and(|state| state & UNKNOWN != 0)));
    }

    #[test]
    fn aliasing_an_unknown_place_preserves_the_unknown_target() {
        let file = SourceFile::new("module.py", "module", "alias = source\n");
        let mut state = State {
            alive: true,
            ..State::default()
        };
        state.unknown_places.insert("source".into());
        let statement = FlowStatement::Assign {
            name: "alias".into(),
            value: AstExpr {
                range: SourceRange { start: 8, end: 14 },
                kind: AstExprKind::Name("source".into()),
            },
            range: SourceRange { start: 0, end: 14 },
        };
        let mut result = inspection();

        execute(
            &[statement],
            &mut state,
            &file,
            &[],
            &FunctionMap::new(),
            &mut result,
            0,
        );

        assert!(state.unknown_places.contains("alias"));
        assert!(!state.places.contains_key("alias"));
    }

    #[test]
    fn control_flow_depth_exhaustion_weakens_resources_and_reports_a_boundary() {
        let file = SourceFile::new("module.py", "module", "\n".repeat(2));
        let mut state = State {
            alive: true,
            ..State::default()
        };
        state
            .places
            .insert("resource".into(), BTreeSet::from(["object".into()]));
        state.objects.insert("object".into(), OPEN);
        let mut body = vec![FlowStatement::Pass {
            range: SourceRange { start: 0, end: 1 },
        }];
        for _ in 0..=MAX_CONTROL_FLOW_DEPTH {
            body = vec![FlowStatement::If {
                test: AstExpr {
                    range: SourceRange { start: 0, end: 1 },
                    kind: AstExprKind::Name("flag".into()),
                },
                body,
                alternate: Vec::new(),
                range: SourceRange { start: 0, end: 1 },
            }];
        }
        let mut result = inspection();

        execute(
            &body,
            &mut state,
            &file,
            &[],
            &FunctionMap::new(),
            &mut result,
            0,
        );

        assert_eq!(state.objects.get("object"), Some(&(OPEN | UNKNOWN)));
        assert_eq!(result.coverage.status, "incomplete");
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("control-flow nesting budget exceeded")
        }));
    }

    #[test]
    fn call_depth_exhaustion_weakens_resources_and_reports_a_boundary() {
        let file = SourceFile::new("module.py", "module", "\n".repeat(MAX_CALL_DEPTH + 2));
        let mut state = State {
            alive: true,
            ..State::default()
        };
        state
            .places
            .insert("resource".into(), BTreeSet::from(["object".into()]));
        state.objects.insert("object".into(), OPEN);
        let mut expression = AstExpr {
            range: SourceRange { start: 0, end: 1 },
            kind: AstExprKind::Name("resource".into()),
        };
        for _ in 0..=MAX_CALL_DEPTH + 1 {
            expression = AstExpr {
                range: SourceRange { start: 0, end: 1 },
                kind: AstExprKind::Call {
                    function: Box::new(AstExpr {
                        range: SourceRange { start: 0, end: 1 },
                        kind: AstExprKind::Name("unknown".into()),
                    }),
                    arguments: vec![expression],
                    keywords: Vec::new(),
                },
            };
        }
        let mut result = inspection();

        consume_expr(
            &expression,
            &mut state,
            &file,
            &[],
            &FunctionMap::new(),
            &mut result,
            0,
        );

        assert_eq!(state.objects.get("object"), Some(&(OPEN | UNKNOWN)));
        assert_eq!(result.coverage.status, "incomplete");
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("call-summary recursion budget exceeded")
        }));
    }

    #[test]
    fn object_budget_exhaustion_does_not_create_an_exact_place() {
        let file = SourceFile::new("module.py", "module", "\n".repeat(MAX_FLOW_OBJECTS + 1));
        let contract = Contract {
            schema_version: 1,
            base_symbol: "module.Connection".into(),
            display_name: "Connection".into(),
            lifecycle: Some(Lifecycle {
                closed_by: "close".into(),
            }),
            access: "unspecified".into(),
            source: Span {
                file: file.path.clone(),
                line: 1,
                column: 0,
                end_line: 1,
                end_column: 1,
            },
            provenance: vec![Provenance {
                origin: "test".into(),
                trust: "declared-unverified".into(),
            }],
        };
        let statements: Vec<_> = (0..=MAX_FLOW_OBJECTS)
            .map(|index| FlowStatement::Assign {
                name: format!("resource_{index}"),
                value: AstExpr {
                    range: SourceRange {
                        start: index,
                        end: index + 1,
                    },
                    kind: AstExprKind::Call {
                        function: Box::new(AstExpr {
                            range: SourceRange {
                                start: index,
                                end: index + 1,
                            },
                            kind: AstExprKind::Name("Connection".into()),
                        }),
                        arguments: Vec::new(),
                        keywords: Vec::new(),
                    },
                },
                range: SourceRange {
                    start: index,
                    end: index + 1,
                },
            })
            .collect();
        let mut state = State {
            alive: true,
            ..State::default()
        };
        let mut result = inspection();

        execute(
            &statements,
            &mut state,
            &file,
            &[contract],
            &FunctionMap::new(),
            &mut result,
            0,
        );

        assert_eq!(state.objects.len(), MAX_FLOW_OBJECTS);
        assert!(!state
            .places
            .contains_key(&format!("resource_{MAX_FLOW_OBJECTS}")));
        assert!(state
            .unknown_places
            .contains(&format!("resource_{MAX_FLOW_OBJECTS}")));
        assert_eq!(result.coverage.status, "incomplete");
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0]
            .message
            .contains("object budget exceeded"));
    }
}

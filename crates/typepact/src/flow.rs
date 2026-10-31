use crate::{
    frontend::{AstExpr, AstExprKind, CallSignature, Declaration, FlowStatement, SourceRange},
    Contract, Diagnostic, Inspection, SourceFile, Span,
};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    parameters: Vec<String>,
    signature: CallSignature,
    contracts: Arc<[Contract]>,
    parameter_contracts: BTreeMap<String, Contract>,
    statements: Vec<FlowStatement>,
    supported_binding: bool,
    is_async: bool,
    binding_key: Option<String>,
    module_bindings: Arc<ModuleBindings>,
}

#[derive(Clone, Default)]
struct ModuleBindings {
    functions: BTreeMap<String, String>,
    classes: BTreeMap<String, Option<Contract>>,
}

struct FlowContext<'a> {
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
    resources: BTreeMap<String, Contract>,
    borrowed: BTreeSet<String>,
    borrowed_origins: BTreeMap<String, Span>,
    escaped: BTreeSet<String>,
    call_stack: BTreeSet<String>,
    call_depth: usize,
    control_depth: usize,
    function_bindings: BTreeMap<String, String>,
    function_bindings_complete: bool,
    class_bindings: BTreeMap<String, Option<Contract>>,
    module_function_bindings: BTreeMap<String, String>,
    module_class_bindings: BTreeMap<String, Option<Contract>>,
    imported_classes: BTreeMap<String, Option<Contract>>,
    class_bindings_complete: bool,
    module_scope: bool,
    scope_module: String,
    close_origins: BTreeMap<String, Span>,
    alive: bool,
}

fn module_bindings_for_child(
    parent: &State,
    definition: &FunctionBody,
) -> (BTreeMap<String, String>, BTreeMap<String, Option<Contract>>) {
    if parent.scope_module == definition.file.module {
        if parent.module_scope {
            (
                parent.function_bindings.clone(),
                parent.class_bindings.clone(),
            )
        } else {
            (
                parent.module_function_bindings.clone(),
                parent.module_class_bindings.clone(),
            )
        }
    } else {
        (
            definition.module_bindings.functions.clone(),
            definition.module_bindings.classes.clone(),
        )
    }
}

fn parameter_contracts(
    file: &SourceFile,
    name: &str,
    parameters: &[crate::frontend::Parameter],
    contracts: &[Contract],
) -> BTreeMap<String, Contract> {
    parameters
        .iter()
        .filter_map(|parameter| {
            let annotation = parameter.annotation.as_ref()?;
            let annotation_span = crate::source_span(&file.path, &file.source, annotation.range);
            contracts
                .iter()
                .find(|contract| {
                    contract.display_name == format!("{name}.{}", parameter.name)
                        && contract.lifecycle.is_some()
                        && contract.source == annotation_span
                })
                .cloned()
                .map(|contract| (parameter.name.clone(), contract))
        })
        .collect()
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

fn resource_contract_for_class<'a>(
    contracts: &'a [Contract],
    class: &str,
    file: &SourceFile,
    position: usize,
) -> Option<&'a Contract> {
    let current_line = file.source[..position.min(file.source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    contracts
        .iter()
        .filter(|contract| {
            contract.display_name == class
                && contract.lifecycle.is_some()
                && contract.source.file == file.path
                && contract.source.line <= current_line
        })
        .max_by_key(|contract| contract.source.line)
        .or_else(|| {
            contracts.iter().find(|contract| {
                contract.display_name == class
                    && contract.lifecycle.is_some()
                    && contract.source.file != file.path
            })
        })
}

fn resource_method<'a>(
    resource: &Contract,
    method: &str,
    functions: &'a FunctionMap,
) -> Option<&'a FunctionBody> {
    let (module, class) = resource.base_symbol.rsplit_once('.')?;
    functions.get(&format!("{module}::@{class}.{method}"))
}

fn class_resource_contract(
    file: &SourceFile,
    name: &str,
    decorators: &[SourceRange],
    contracts: &[Contract],
) -> Option<Contract> {
    contracts
        .iter()
        .find(|contract| {
            contract.lifecycle.is_some()
                && contract.display_name == name
                && contract.source.file == file.path
                && decorators
                    .iter()
                    .any(|decorator| contract.source == span(file, *decorator))
        })
        .cloned()
}

fn contract_inside_range(contract: &Contract, file: &SourceFile, range: SourceRange) -> bool {
    if contract.source.file != file.path || contract.lifecycle.is_none() {
        return false;
    }
    let container = span(file, range);
    (contract.source.line, contract.source.column) >= (container.line, container.column)
        && (contract.source.end_line, contract.source.end_column)
            <= (container.end_line, container.end_column)
}

#[derive(Debug)]
enum BindingFailure {
    Invalid,
    Unknown,
}

fn bind_arguments<'a>(
    signature: &CallSignature,
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
    receiver: bool,
) -> Result<BTreeMap<String, &'a AstExpr>, BindingFailure> {
    if keywords.iter().any(|(name, _)| name.is_none())
        || arguments
            .iter()
            .any(|arg| matches!(arg.kind, AstExprKind::Other | AstExprKind::DepthLimit))
    {
        return Err(BindingFailure::Unknown);
    }
    let positional: Vec<_> = signature
        .positional_only
        .iter()
        .chain(&signature.positional_or_keyword)
        .collect();
    if receiver && positional.is_empty() {
        return Err(if signature.vararg.is_some() {
            BindingFailure::Unknown
        } else {
            BindingFailure::Invalid
        });
    }
    let mut supplied = BTreeSet::new();
    if receiver {
        supplied.insert(positional[0].clone());
    }
    let mut bindings = BTreeMap::new();
    let mut unknown = false;
    for (index, argument) in arguments.iter().enumerate() {
        if let Some(parameter) = positional.get(index + usize::from(receiver)) {
            supplied.insert((*parameter).clone());
            bindings.insert((*parameter).clone(), argument);
        } else if signature.vararg.is_some() {
            unknown = true;
        } else {
            return Err(BindingFailure::Invalid);
        }
    }
    for (name, argument) in keywords {
        let name = name.as_ref().expect("expanded keywords handled above");
        if signature.positional_or_keyword.contains(name) || signature.keyword_only.contains(name) {
            if !supplied.insert(name.clone()) {
                return Err(BindingFailure::Invalid);
            }
            bindings.insert(name.clone(), argument);
        } else if signature.kwarg.is_some() {
            unknown = true;
        } else {
            return Err(BindingFailure::Invalid);
        }
    }
    for parameter in positional.into_iter().chain(signature.keyword_only.iter()) {
        if !supplied.contains(parameter) {
            if signature.optional.contains(parameter) {
                // Default objects and variadic containers do not have tracked aliases.
                unknown |= signature.unknown_defaults.contains(parameter);
            } else {
                return Err(BindingFailure::Invalid);
            }
        }
    }
    if unknown {
        Err(BindingFailure::Unknown)
    } else {
        Ok(bindings)
    }
}

fn binding_failure(
    failure: BindingFailure,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let message = match failure {
        BindingFailure::Invalid => {
            env.alive = false;
            "invalid call argument binding ends normal lifecycle flow"
        }
        BindingFailure::Unknown => {
            let affected: Vec<_> = env.places.values().flatten().cloned().collect();
            weaken(env, affected.into_iter());
            env.escaped.extend(env.places.values().flatten().cloned());
            "call argument binding is not modeled"
        }
    };
    boundary(result, file, range, message);
}

fn expression_may_return_resource(expression: &AstExpr) -> bool {
    match &expression.kind {
        AstExprKind::NoneLiteral | AstExprKind::String(_) | AstExprKind::Primitive => false,
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
    functions: &FunctionMap,
) -> bool {
    if let Some((receiver, method, _, _)) = called_method(expression) {
        return expr_name(receiver)
            .and_then(|name| env.places.get(name))
            .is_none_or(|objects| {
                objects.iter().any(|object| {
                    env.resources
                        .get(object)
                        .and_then(|resource| resource_method(resource, method, functions))
                        .is_none_or(|definition| {
                            !definition.supported_binding
                                || statements_may_return_resource(&definition.statements)
                        })
                })
            });
    }

    let AstExprKind::Call { function, .. } = &expression.kind else {
        return false;
    };
    let Some(callee) = expr_name(function) else {
        return true;
    };
    let definition = if env.function_bindings_complete {
        env.function_bindings
            .get(callee)
            .and_then(|binding| functions.get(binding))
    } else {
        functions
            .get(&format!("{}::{callee}", file.module))
            .or_else(|| {
                functions
                    .get(callee)
                    .filter(|definition| definition.file.module == file.module)
            })
    };
    definition.is_none_or(|definition| statements_may_return_resource(&definition.statements))
}

fn merge(left: &State, right: &State) -> (State, JoinEffects) {
    let mut joined = left.clone();
    let mut effects = JoinEffects::default();
    let binding_names: BTreeSet<_> = left
        .function_bindings
        .keys()
        .chain(right.function_bindings.keys())
        .cloned()
        .collect();
    for name in binding_names {
        if left.function_bindings.get(&name) != right.function_bindings.get(&name) {
            joined.function_bindings.remove(&name);
        }
    }
    let class_names: BTreeSet<_> = left
        .class_bindings
        .keys()
        .chain(right.class_bindings.keys())
        .cloned()
        .collect();
    for name in class_names {
        if left.class_bindings.get(&name) != right.class_bindings.get(&name) {
            joined.class_bindings.remove(&name);
        }
    }
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
    joined.resources.extend(right.resources.clone());
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

fn join_normal_exits(fallthrough: State, returned: Vec<State>) -> State {
    let mut exits = returned.into_iter();
    let initial = if fallthrough.alive {
        fallthrough
    } else if let Some(exit) = exits.next() {
        exit
    } else {
        return fallthrough;
    };
    exits.fold(initial, |joined, exit| merge(&joined, &exit).0)
}

fn span(file: &SourceFile, range: SourceRange) -> Span {
    crate::source_span(&file.path, &file.source, range)
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
        if let Some(resource) = parent.resources.get(object) {
            child.resources.insert(object.clone(), resource.clone());
        }
        if parent.borrowed.contains(object) {
            child.borrowed.insert(object.clone());
        }
        if parent.escaped.contains(object) {
            child.escaped.insert(object.clone());
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
    result: &mut Inspection,
) {
    let Some(name) = expr_name(argument) else {
        return;
    };
    let Some(targets) = env.places.get(name) else {
        return;
    };
    let requires_open_resource = definition
        .parameter_contracts
        .get(parameter)
        .is_some_and(|contract| contract.access == "shared" && contract.lifecycle.is_some());
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
        call_key,
    } = invocation;
    let (module_function_bindings, module_class_bindings) =
        module_bindings_for_child(parent, definition);
    let mut child = State {
        alive: true,
        call_stack: parent.call_stack.clone(),
        function_bindings: module_function_bindings.clone(),
        function_bindings_complete: true,
        class_bindings: module_class_bindings.clone(),
        module_function_bindings,
        module_class_bindings,
        class_bindings_complete: true,
        call_depth: parent.call_depth.saturating_add(1),
        control_depth: parent.control_depth,
        scope_module: definition.file.module.clone(),
        ..State::default()
    };
    child.call_stack.insert(call_key);
    copy_object_metadata(
        &mut child,
        parent,
        &parent.escaped.iter().cloned().collect(),
    );
    child
        .shadowed_functions
        .extend(definition.parameters.iter().cloned());
    child
        .shadowed_functions
        .extend(assigned_names(&definition.statements));
    child
        .shadowed_classes
        .extend(definition.parameters.iter().cloned());
    child
        .shadowed_classes
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
    let bindings = bind_arguments(&definition.signature, arguments, keywords, true)
        .expect("method arguments were validated before execution");
    for (parameter, argument) in bindings {
        if let Some(actual) = expr_name(argument) {
            if let Some(targets) = parent.places.get(actual) {
                child.places.insert(parameter.clone(), targets.clone());
                copy_object_metadata(&mut child, parent, targets);
                if parent.unknown_places.contains(actual) {
                    child.unknown_places.insert(parameter);
                }
            }
        }
    }
    let returned = execute(
        &definition.statements,
        &mut child,
        &definition.file,
        &definition.contracts,
        context.functions,
        context.result,
        0,
    );
    join_normal_exits(child, returned)
}

fn resolve_imported_contracts(
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

fn contracts_for_module(
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

fn final_class_bindings(
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
            Declaration::Unsupported { .. } => {
                for binding in bindings.values_mut() {
                    *binding = None;
                }
            }
            Declaration::Assignment { .. } | Declaration::TypeAlias { .. } => {}
        }
    }
    bindings
}

fn final_function_bindings(module: &str, functions: &FunctionMap) -> BTreeMap<String, String> {
    let prefix = format!("{module}::");
    functions
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
    let declarations = summaries.get(&provider.path)?.declarations.as_ref()?;
    for declaration in declarations.iter().rev() {
        match declaration {
            Declaration::Function {
                name: bound,
                decorators,
                unsupported,
                ..
            } if bound == name => {
                return if decorators.is_empty() && !unsupported {
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
        if !env.alive {
            break;
        }
        if !matches!(
            argument.kind,
            AstExprKind::Name(_)
                | AstExprKind::String(_)
                | AstExprKind::NoneLiteral
                | AstExprKind::Primitive
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
    if depth > MAX_CALL_DEPTH || env.call_depth >= MAX_CALL_DEPTH {
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
    if !env.alive {
        return;
    }
    if let Some((receiver, method, arguments, keywords)) = called_method(expr) {
        if let Some(name) = expr_name(receiver) {
            if let Some(targets) = env.places.get(name).cloned() {
                let has_open_alternative = targets.iter().any(|object| {
                    let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
                    mask & (OPEN | UNKNOWN) != 0
                }) || env.unknown_places.contains(name);
                let exact_singleton = targets.len() == 1 && !env.unknown_places.contains(name);
                let uncertain_receiver = !exact_singleton;
                // A singleton receiver can be evaluated in place. Cloning the whole
                // environment for every method call made straight-line resource
                // workloads quadratic in the number of resources.
                let incoming = (!exact_singleton).then(|| env.clone());
                let mut normal_outcome: Option<State> = None;
                let mut dead_exact_outcome: Option<State> = None;
                if incoming
                    .as_ref()
                    .is_some_and(|incoming| incoming.unknown_places.contains(name))
                {
                    let incoming = incoming
                        .as_ref()
                        .expect("unknown receiver has an input state");
                    let mut unknown = incoming.clone();
                    let affected: Vec<_> = targets
                        .iter()
                        .cloned()
                        .chain(
                            arguments
                                .iter()
                                .chain(keywords.iter().map(|(_, value)| value))
                                .filter_map(expr_name)
                                .filter_map(|name| incoming.places.get(name))
                                .flatten()
                                .cloned(),
                        )
                        .collect();
                    weaken(&mut unknown, affected.into_iter());
                    boundary(
                        result,
                        file,
                        expr.range,
                        "method receiver may have an unknown target",
                    );
                    normal_outcome = Some(unknown);
                }
                for object in &targets {
                    let mut alternative = if exact_singleton {
                        std::mem::take(env)
                    } else {
                        incoming
                            .as_ref()
                            .expect("multi-target receiver has an input state")
                            .clone()
                    };
                    alternative
                        .places
                        .insert(name.to_owned(), BTreeSet::from([object.clone()]));
                    alternative.unknown_places.remove(name);
                    let diagnostic_start = result.diagnostics.len();
                    (|| {
                        let env = &mut alternative;
                        let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
                        let resource = env.resources.get(object).cloned();
                        let is_close = resource
                            .as_ref()
                            .and_then(|resource| resource.lifecycle.as_ref())
                            .is_some_and(|lifecycle| lifecycle.closed_by == method);
                        let method_key = resource
                            .as_ref()
                            .map(|resource| format!("{}::{method}", resource.base_symbol))
                            .unwrap_or_else(|| method.to_owned());
                        let definition = resource
                            .as_ref()
                            .and_then(|resource| resource_method(resource, method, functions));
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
                            weaken(env, affected.iter().cloned());
                            env.escaped.extend(affected);
                            boundary(
                                result,
                                file,
                                expr.range,
                                "decorated method binding is not modeled",
                            );
                            return;
                        }
                        if let Some(definition) = definition {
                            if let Err(failure) =
                                bind_arguments(&definition.signature, arguments, keywords, true)
                            {
                                binding_failure(failure, env, file, expr.range, result);
                                return;
                            }
                        }
                        if definition.is_some_and(|definition| definition.is_async) {
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
                            weaken(env, affected.iter().cloned());
                            env.escaped.extend(affected);
                            boundary(
                                result,
                                file,
                                expr.range,
                                "async method effects are not modeled",
                            );
                            return;
                        }
                        if mask & CLOSED != 0 && !is_close {
                            let certainty = if !has_open_alternative && mask & UNKNOWN == 0 {
                                "definite"
                            } else {
                                "possible"
                            };
                            let primary = span(file, expr.range);
                            let related =
                                env.close_origins.get(object).cloned().into_iter().collect();
                            report(
                                result,
                                "TP1001",
                                "use of a resource that may be closed",
                                primary,
                                related,
                                certainty,
                            );
                        }
                        if env.borrowed.contains(object) && is_close {
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
                                return;
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
                                    call_key: method_key,
                                },
                                FlowContext { functions, result },
                            );
                            for (changed_object, value) in child.objects {
                                env.objects.insert(changed_object, value);
                            }
                            if is_close && child.alive {
                                env.close_origins
                                    .insert(object.clone(), span(file, expr.range));
                                env.objects.insert(object.clone(), CLOSED);
                            }
                            env.resources.extend(child.resources);
                            env.close_origins.extend(child.close_origins);
                            env.escaped.extend(child.escaped);
                            if !child.alive {
                                env.alive = false;
                            }
                        } else if is_close {
                            env.close_origins
                                .insert(object.clone(), span(file, expr.range));
                            env.objects.insert(object.clone(), CLOSED);
                        } else {
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
                            env.escaped.insert(object.clone());
                            boundary(
                                result,
                                file,
                                expr.range,
                                &format!("unknown method effect for {method}"),
                            );
                        }
                    })();
                    if uncertain_receiver {
                        for diagnostic in &mut result.diagnostics[diagnostic_start..] {
                            if diagnostic.code == "TP1001"
                                && diagnostic.primary != span(file, expr.range)
                            {
                                diagnostic.certainty = "possible".into();
                                diagnostic.severity = "warning".into();
                            }
                        }
                    }
                    if alternative.alive {
                        normal_outcome = Some(match normal_outcome {
                            Some(previous) => merge(&previous, &alternative).0,
                            None => alternative,
                        });
                    } else if exact_singleton {
                        dead_exact_outcome = Some(alternative);
                    }
                }
                *env = normal_outcome.unwrap_or_else(|| match incoming {
                    Some(mut incoming) => {
                        incoming.alive = false;
                        incoming
                    }
                    None => {
                        dead_exact_outcome.expect("exact receiver has a normal or stopped outcome")
                    }
                });
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
            } else if env.module_scope || env.function_bindings_complete {
                env.function_bindings
                    .get(callee)
                    .and_then(|binding| functions.get(binding))
            } else {
                functions.get(&scoped_name).or_else(|| {
                    functions
                        .get(callee)
                        .filter(|definition| definition.file.module == file.module)
                })
            };
            if let Some(definition) = known_function {
                let bindings =
                    match bind_arguments(&definition.signature, arguments, keywords, false) {
                        Ok(bindings) => bindings,
                        Err(failure) => {
                            binding_failure(failure, env, file, expr.range, result);
                            return;
                        }
                    };
                if definition.is_async {
                    let affected: Vec<_> = arguments
                        .iter()
                        .chain(keywords.iter().map(|(_, value)| value))
                        .filter_map(expr_name)
                        .filter_map(|name| env.places.get(name))
                        .flatten()
                        .cloned()
                        .collect();
                    weaken(env, affected.iter().cloned());
                    env.escaped.extend(affected);
                    boundary(
                        result,
                        file,
                        expr.range,
                        "async function effects are not modeled",
                    );
                    return;
                }
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
                let (module_function_bindings, module_class_bindings) =
                    module_bindings_for_child(env, definition);
                let mut child = State {
                    alive: true,
                    call_stack: env.call_stack.clone(),
                    function_bindings: module_function_bindings.clone(),
                    function_bindings_complete: true,
                    class_bindings: module_class_bindings.clone(),
                    module_function_bindings,
                    module_class_bindings,
                    class_bindings_complete: true,
                    call_depth: env.call_depth.saturating_add(1),
                    control_depth: env.control_depth,
                    scope_module: definition.file.module.clone(),
                    ..State::default()
                };
                child.call_stack.insert(callee.to_string());
                copy_object_metadata(&mut child, env, &env.escaped.iter().cloned().collect());
                child
                    .shadowed_functions
                    .extend(definition.parameters.iter().cloned());
                child
                    .shadowed_functions
                    .extend(assigned_names(&definition.statements));
                child
                    .shadowed_classes
                    .extend(definition.parameters.iter().cloned());
                child
                    .shadowed_classes
                    .extend(assigned_names(&definition.statements));
                let call_site = span(file, expr.range);
                for (parameter, argument) in bindings {
                    if let Some(actual) = expr_name(argument) {
                        if let Some(targets) = env.places.get(actual) {
                            child.places.insert(parameter.clone(), targets.clone());
                            copy_object_metadata(&mut child, env, targets);
                            if env.unknown_places.contains(actual) {
                                child.unknown_places.insert(parameter.clone());
                            }
                        }
                    }
                    check_borrowed_precondition(
                        definition,
                        &parameter,
                        argument,
                        env,
                        call_site.clone(),
                        result,
                    );
                }
                let diagnostic_start = result.diagnostics.len();
                let returned = execute(
                    &definition.statements,
                    &mut child,
                    &definition.file,
                    &definition.contracts,
                    functions,
                    result,
                    0,
                );
                let child = join_normal_exits(child, returned);
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
                env.resources.extend(child.resources);
                env.close_origins.extend(child.close_origins);
                env.escaped.extend(child.escaped);
                if !child.alive {
                    env.alive = false;
                }
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
        | AstExprKind::DepthLimit => {
            env.unknown_places.extend(env.places.keys().cloned());
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
        if let FlowStatement::If {
            test,
            body,
            alternate,
            range,
        } = statement
        {
            consume_expr(test, env, file, contracts, functions, result, 0);
            if !env.alive {
                continue;
            }
            if depth >= MAX_CONTROL_FLOW_DEPTH || env.control_depth >= MAX_CONTROL_FLOW_DEPTH {
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
            let before = Box::new(env.clone());
            let mut left = Box::new((*before).clone());
            left.control_depth = left.control_depth.saturating_add(1);
            left.alive = true;
            let mut right = Box::new((*before).clone());
            right.control_depth = right.control_depth.saturating_add(1);
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
            let (mut joined, effects) = match (left.alive, right.alive) {
                (true, true) => merge(&left, &right),
                (true, false) => (*left, JoinEffects::default()),
                (false, true) => (*right, JoinEffects::default()),
                (false, false) => {
                    let (mut joined, effects) = merge(&left, &right);
                    joined.alive = false;
                    (joined, effects)
                }
            };
            joined.control_depth = before.control_depth;
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
        } else if let Some(returned) =
            execute_non_if_statement(statement, env, file, contracts, functions, result)
        {
            returned_states.push(returned);
        }
    }
    returned_states
}

fn execute_non_if_statement(
    statement: &FlowStatement,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<State> {
    match statement {
        FlowStatement::Assign { name, value, range } => {
            let source_class_binding = expr_name(value)
                .filter(|source| {
                    !env.places.contains_key(*source) && !env.unknown_places.contains(*source)
                })
                .and_then(|source| env.class_bindings.get(source))
                .cloned();
            let resource_view = contracts
                .iter()
                .filter(|contract| {
                    contract.display_name == *name && contract_inside_range(contract, file, *range)
                })
                .max_by_key(|contract| (contract.source.line, contract.source.column))
                .cloned();
            let defines_resource_view = resource_view.is_some();
            if let AstExprKind::Call { function, .. } = &value.kind {
                if let Some(class) = expr_name(function) {
                    let resource_contract = match env.class_bindings.get(class) {
                        Some(binding) => binding.clone(),
                        None if env.module_scope || env.class_bindings_complete => None,
                        None => {
                            resource_contract_for_class(contracts, class, file, usize::MAX).cloned()
                        }
                    };
                    if resource_contract.is_some() && !env.shadowed_classes.contains(class) {
                        evaluate_call_arguments(value, env, file, contracts, functions, result, 0);
                        if !env.alive {
                            return None;
                        }
                        let new_definition = resource_contract
                            .as_ref()
                            .and_then(|resource| resource_method(resource, "__new__", functions));
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
                            env.class_bindings.remove(name);
                            env.shadowed_functions.insert(name.clone());
                            boundary(
                                result,
                                file,
                                *range,
                                "custom __new__ allocation effects are not modeled",
                            );
                            return None;
                        }
                        let object = format!("{}:{}:{}", file.path, range.start, class);
                        if !env.objects.contains_key(&object)
                            && env.objects.len() >= MAX_FLOW_OBJECTS
                        {
                            env.places.remove(name);
                            env.unknown_places.insert(name.clone());
                            env.shadowed_classes.insert(name.clone());
                            env.class_bindings.remove(name);
                            boundary(
                                    result,
                                    file,
                                    *range,
                                    "tracked resource object budget exceeded; lifecycle state is unknown",
                                );
                            return None;
                        }
                        env.objects.insert(object.clone(), OPEN);
                        env.resources.insert(
                            object.clone(),
                            resource_contract
                                .clone()
                                .expect("resolved resource constructor"),
                        );
                        if let AstExprKind::Call {
                            arguments,
                            keywords,
                            ..
                        } = &value.kind
                        {
                            let resource =
                                env.resources.get(&object).expect("constructed resource");
                            let init_key = format!("{}::__init__", resource.base_symbol);
                            let definition = resource_method(resource, "__init__", functions);
                            if let Some(definition) = definition {
                                if env.call_depth >= MAX_CALL_DEPTH
                                    || env.call_stack.contains(&init_key)
                                {
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
                                    weaken(env, affected.iter().cloned());
                                    env.escaped.extend(affected);
                                    boundary(
                                        result,
                                        file,
                                        *range,
                                        "recursive or over-budget constructor effects are unknown",
                                    );
                                } else if definition.is_async {
                                    weaken(env, [object.clone()].into_iter());
                                    env.escaped.insert(object.clone());
                                    boundary(
                                        result,
                                        file,
                                        *range,
                                        "async constructor effects are not modeled",
                                    );
                                } else if !definition.supported_binding {
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
                                    env.escaped.insert(object.clone());
                                    boundary(
                                        result,
                                        file,
                                        *range,
                                        "decorated method binding is not modeled",
                                    );
                                } else if let Err(failure) =
                                    bind_arguments(&definition.signature, arguments, keywords, true)
                                {
                                    binding_failure(failure, env, file, *range, result);
                                } else {
                                    let child = execute_method_body(
                                        MethodInvocation {
                                            definition,
                                            object: &object,
                                            initial_mask: OPEN,
                                            arguments,
                                            keywords,
                                            parent: env,
                                            call_key: init_key,
                                        },
                                        FlowContext { functions, result },
                                    );
                                    for (changed_object, state) in child.objects {
                                        env.objects.insert(changed_object, state);
                                    }
                                    env.resources.extend(child.resources);
                                    env.close_origins.extend(child.close_origins);
                                    env.escaped.extend(child.escaped);
                                    if !child.alive {
                                        env.alive = false;
                                    }
                                }
                            } else if let Err(failure) = bind_arguments(
                                &CallSignature::default(),
                                arguments,
                                keywords,
                                false,
                            ) {
                                binding_failure(failure, env, file, *range, result);
                            }
                        }
                        env.places
                            .insert(name.clone(), BTreeSet::from([object.clone()]));
                        env.unknown_places.remove(name);
                        env.shadowed_classes.insert(name.clone());
                        env.class_bindings.remove(name);
                        env.shadowed_functions.insert(name.clone());
                        return None;
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
                let may_return_resource =
                    passes_resource && call_may_return_resource(value, env, file, functions);
                if may_return_resource {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                    env.places.remove(name);
                    env.unknown_places.insert(name.clone());
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    boundary(
                        result,
                        file,
                        *range,
                        "resource-valued call result is not tracked",
                    );
                    return None;
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
                if !defines_resource_view {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                }
                env.places.remove(name);
                env.unknown_places.remove(name);
            }
            env.shadowed_functions.insert(name.clone());
            if defines_resource_view {
                env.shadowed_classes.remove(name);
                if let Some(contract) = resource_view {
                    env.class_bindings
                        .insert(name.clone(), Some(contract.clone()));
                }
            } else {
                if let Some(binding) = source_class_binding {
                    env.shadowed_classes.remove(name);
                    env.class_bindings.insert(name.clone(), binding);
                } else {
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                }
            }
        }
        FlowStatement::Expression { value, .. } => {
            consume_expr(value, env, file, contracts, functions, result, 0)
        }
        FlowStatement::Pass { .. } => {}
        FlowStatement::Return { value, .. } => {
            if let Some(value) = value {
                consume_expr(value, env, file, contracts, functions, result, 0);
            }
            if !env.alive {
                return None;
            }
            let mut returned = env.clone();
            returned.alive = true;
            env.alive = false;
            return Some(returned);
        }
        FlowStatement::Raise { range } => {
            boundary(result, file, *range, "raise ends normal lifecycle flow");
            env.alive = false;
        }
        FlowStatement::BindFunction {
            name,
            binding,
            class_decorators,
            range: _,
        } => {
            env.shadowed_functions.remove(name);
            env.shadowed_classes.remove(name);
            if let Some(binding) = binding {
                if binding == "class" {
                    let resource = class_resource_contract(file, name, class_decorators, contracts);
                    env.class_bindings.insert(name.clone(), resource);
                    env.function_bindings.remove(name);
                } else if binding.starts_with("import:") {
                    if functions.contains_key(binding) {
                        env.function_bindings.insert(name.clone(), binding.clone());
                    } else {
                        env.function_bindings.remove(name);
                    }
                    let contract = env.imported_classes.get(binding).cloned().flatten();
                    env.class_bindings.insert(name.clone(), contract);
                } else {
                    env.function_bindings.insert(name.clone(), binding.clone());
                    env.class_bindings.remove(name);
                }
            } else {
                env.function_bindings.remove(name);
                env.class_bindings.remove(name);
            }
        }
        FlowStatement::Unsupported { range } => {
            env.unknown_places.extend(env.places.keys().cloned());
            if env.function_bindings_complete {
                env.function_bindings.clear();
            }
            if env.class_bindings_complete {
                env.class_bindings.clear();
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
            boundary(
                result,
                file,
                *range,
                "unsupported flow weakens tracked resources",
            );
        }
        FlowStatement::If { .. } => unreachable!("if statements use the stack-bounded dispatcher"),
    }
    None
}

fn statement_range(statement: &FlowStatement) -> SourceRange {
    match statement {
        FlowStatement::Pass { range }
        | FlowStatement::Assign { range, .. }
        | FlowStatement::Expression { range, .. }
        | FlowStatement::If { range, .. }
        | FlowStatement::Return { range, .. }
        | FlowStatement::Raise { range }
        | FlowStatement::BindFunction { range, .. }
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
    let mut historical_functions = FunctionMap::new();
    let mut class_generations = BTreeMap::<(String, String), usize>::new();
    for file in files {
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        let module_contracts: Arc<[Contract]> =
            contracts_for_module(file, files, summaries, result).into();
        for declaration in declarations {
            match declaration {
                Declaration::Function {
                    name,
                    parameters,
                    signature,
                    body,
                    is_async,
                    decorators,
                    range,
                    unsupported,
                    ..
                } => {
                    if !decorators.is_empty() || *unsupported {
                        continue;
                    }
                    let binding_key = format!("{}::{}:{}", file.module, name, range.start);
                    let function = FunctionBody {
                        file: file.clone(),
                        parameters: parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        signature: signature.clone(),
                        contracts: Arc::clone(&module_contracts),
                        parameter_contracts: parameter_contracts(
                            file,
                            name,
                            parameters,
                            &module_contracts,
                        ),
                        statements: body.clone(),
                        supported_binding: true,
                        is_async: *is_async,
                        binding_key: Some(binding_key.clone()),
                        module_bindings: Arc::new(ModuleBindings::default()),
                    };
                    historical_functions.insert(binding_key, function.clone());
                    project_functions.insert((file.module.clone(), name.clone()), function);
                }
                Declaration::Class {
                    name,
                    decorators,
                    methods,
                    range,
                    ..
                } => {
                    let decorator_ranges: Vec<_> =
                        decorators.iter().map(|decorator| decorator.range).collect();
                    let resource_class =
                        class_resource_contract(file, name, &decorator_ranges, &result.contracts);
                    let generation = class_generations
                        .entry((file.module.clone(), name.clone()))
                        .or_default();
                    *generation += 1;
                    let fallback_identity = if *generation == 1 {
                        name.clone()
                    } else {
                        format!("{name}#{}", *generation)
                    };
                    let class_identity = resource_class
                        .as_ref()
                        .and_then(|contract| contract.base_symbol.rsplit_once('.'))
                        .map(|(_, class)| class.to_owned())
                        .unwrap_or(fallback_identity);
                    for method in methods {
                        if method.has_parameter_annotations {
                            boundary(
                                result,
                                file,
                                method.range,
                                "method parameter annotations are not modeled",
                            );
                        }
                        let method_key = format!("@{class_identity}.{}", method.name);
                        let definition = FunctionBody {
                            file: file.clone(),
                            parameters: method.parameters.clone(),
                            signature: method.signature.clone(),
                            contracts: Arc::clone(&module_contracts),
                            parameter_contracts: BTreeMap::new(),
                            statements: method.body.clone(),
                            supported_binding: !method.decorated,
                            is_async: method.is_async,
                            binding_key: Some(format!(
                                "{}::{}:{}:{}",
                                file.module, method_key, range.start, method.range.start
                            )),
                            module_bindings: Arc::new(ModuleBindings::default()),
                        };
                        historical_functions
                            .insert(definition.binding_key.clone().unwrap(), definition.clone());
                        project_functions.insert((file.module.clone(), method_key), definition);
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
    qualified_functions.extend(historical_functions);
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
                range,
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
                    let binding = format!("import:{}:{}:{}", provider.module, range.start, local);
                    qualified_functions.insert(binding, function.clone());
                }
            }
        }
    }
    for provider in files {
        let Some(declarations) = summaries
            .get(&provider.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        let mut final_bindings = BTreeMap::<String, Option<String>>::new();
        for declaration in declarations {
            match declaration {
                Declaration::Function {
                    name,
                    range,
                    decorators,
                    unsupported,
                    ..
                } => {
                    let key = format!("{}::{}:{}", provider.module, name, range.start);
                    final_bindings.insert(
                        name.clone(),
                        (decorators.is_empty()
                            && !unsupported
                            && qualified_functions.contains_key(&key))
                        .then_some(key),
                    );
                }
                Declaration::Class { name, .. } => {
                    final_bindings.insert(name.clone(), None);
                }
                Declaration::Assignment {
                    name: Some(name),
                    value: Some(_),
                    ..
                }
                | Declaration::TypeAlias {
                    name: Some(name), ..
                } => {
                    final_bindings.insert(name.clone(), None);
                }
                Declaration::Delete { names, .. } => {
                    for name in names.iter().flatten() {
                        final_bindings.insert(name.clone(), None);
                    }
                }
                Declaration::ImportFrom {
                    module: _,
                    level: 0,
                    names,
                    range,
                } => {
                    if names.iter().any(|import| import.name == "*") {
                        for binding in final_bindings.values_mut() {
                            *binding = None;
                        }
                    } else {
                        for import in names {
                            let local = import.alias.as_deref().unwrap_or(&import.name);
                            let key =
                                format!("import:{}:{}:{}", provider.module, range.start, local);
                            final_bindings.insert(
                                local.to_owned(),
                                qualified_functions.contains_key(&key).then_some(key),
                            );
                        }
                    }
                }
                Declaration::ImportFrom { names, .. } => {
                    if names.iter().any(|import| import.name == "*") {
                        for binding in final_bindings.values_mut() {
                            *binding = None;
                        }
                    } else {
                        for import in names {
                            let local = import.alias.as_deref().unwrap_or(&import.name);
                            final_bindings.insert(local.to_owned(), None);
                        }
                    }
                }
                Declaration::Import { names, .. } => {
                    for import in names {
                        let local = import
                            .alias
                            .as_deref()
                            .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                        final_bindings.insert(local.to_owned(), None);
                    }
                }
                Declaration::Unsupported { .. } => {
                    for binding in final_bindings.values_mut() {
                        *binding = None;
                    }
                }
                _ => {}
            }
        }
        for (name, binding) in final_bindings {
            qualified_functions.remove(&format!("{}::{name}", provider.module));
            if let Some(binding) = binding {
                if let Some(function) = qualified_functions.get(&binding).cloned() {
                    qualified_functions.insert(format!("{}::{name}", provider.module), function);
                }
            }
        }
    }
    let mut module_bindings = BTreeMap::<String, Arc<ModuleBindings>>::new();
    for file in files {
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        module_bindings.insert(
            file.path.clone(),
            Arc::new(ModuleBindings {
                functions: final_function_bindings(&file.module, &qualified_functions),
                classes: final_class_bindings(file, declarations, files, summaries, result),
            }),
        );
    }
    for function in qualified_functions.values_mut() {
        if let Some(bindings) = module_bindings.get(&function.file.path) {
            function.module_bindings = Arc::clone(bindings);
        }
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
        let contracts = contracts_for_module(file, files, summaries, result);
        let final_classes = final_class_bindings(file, declarations, files, summaries, result);
        let functions = qualified_functions.clone();
        let final_functions = final_function_bindings(&file.module, &functions);
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
                scope_module: file.module.clone(),
                function_bindings: final_functions.clone(),
                function_bindings_complete: true,
                class_bindings: final_classes.clone(),
                class_bindings_complete: true,
                module_function_bindings: final_functions.clone(),
                module_class_bindings: final_classes.clone(),
                ..State::default()
            };
            env.shadowed_functions
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            env.shadowed_functions.extend(assigned_names(body));
            env.shadowed_classes
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            env.shadowed_classes.extend(assigned_names(body));
            let function_parameter_contracts =
                parameter_contracts(file, name, parameters, &result.contracts);
            for parameter in parameters {
                if parameter.annotation.is_none() {
                    continue;
                }
                let Some(parameter_contract) =
                    function_parameter_contracts.get(&parameter.name).cloned()
                else {
                    continue;
                };
                let object = format!("{}:{}:{}", file.path, range.start, parameter.name);
                env.objects.insert(object.clone(), OPEN);
                env.resources
                    .insert(object.clone(), parameter_contract.clone());
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
            scope_module: file.module.clone(),
            function_bindings_complete: true,
            class_bindings_complete: true,
            module_scope: true,
            ..State::default()
        };
        for declaration in declarations {
            let Declaration::ImportFrom {
                module: source_module,
                level,
                names,
                range,
            } = declaration
            else {
                continue;
            };
            for import in names {
                let local = import.alias.as_deref().unwrap_or(&import.name);
                let binding_key = format!("import:{}:{}:{}", file.module, range.start, local);
                let imported = if *level == 0 {
                    resolve_imported_contracts(
                        source_module,
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
                module.imported_classes.insert(binding_key, imported);
            }
        }
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

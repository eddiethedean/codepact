use crate::model::{
    FlowModelCatalog, LifecyclePostcondition, RepeatClosePolicy, ResourceEffectModel,
};
use crate::{
    frontend::{
        AstExpr, AstExprKind, CallSignature, Declaration, FlowHandler, FlowStatement, FlowWithItem,
        SourceRange,
    },
    Contract, Diagnostic, Inspection, SourceFile, Span,
};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod async_protocols;
mod async_resources;
mod asyncio;
mod calls;
mod contexts;
mod execution;
mod executor;
mod expressions;
mod guarded_access;
mod library;
pub(crate) mod outcomes;
mod ownership;
mod process;
mod providers;
mod recursion;
mod reporting;
mod resolution;
mod runtime;
mod state;
mod tasks;
mod work;
use calls::{bind_arguments, binding_failure, evaluate_call_arguments, BindingFailure};
use execution::{
    builtin_exception_match, completion_exception_name, execute, is_unshadowed_builtin_exception,
    join_states, raised_exception_name,
};
#[cfg(test)]
use execution::{exception_handler_match, ExceptionMatch};
use expressions::{consume_expr, report_possibly_unbound_alias, report_unawaited_coroutines};
use ownership::OwnershipState;
pub(crate) use reporting::reset_flow_budget;
use reporting::{
    boundary, boundary_at_span, contract_identity, grouped_exits, report, report_cleanup_exit,
    report_module_scope_lifetimes, span, suggests_owned_handoff, take_flow_step,
};
use resolution::{
    await_call_captures, contracts_for_module, external_module_bindings, final_class_bindings,
    final_function_bindings, register_pending_coroutine, resolve_imported_contracts,
    resolve_imported_function, resolved_await_call,
};
use state::*;

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const MAX_CALL_DEPTH: usize = 8;
const MAX_CONTROL_FLOW_DEPTH: usize = 64;
const MAX_ALIAS_TARGETS: usize = 64;
const MAX_FLOW_OBJECTS: usize = 4_096;
const MAX_EXIT_PARTITIONS: usize = 256;
const MAX_PROTOCOL_STATES: usize = 32;
const MAX_STATIC_FOR_ITERATIONS: usize = 16;
const MAX_PENDING_COROUTINES: usize = 128;
pub(crate) const MAX_FLOW_STEPS: usize = 100_000;
const UNKNOWN_EXCEPTION_BINDINGS: &str = "*";
thread_local! {
    static FLOW_STEPS: Cell<usize> = const { Cell::new(0) };
    static FLOW_STEP_LIMIT: Cell<usize> = const { Cell::new(MAX_FLOW_STEPS) };
    static FLOW_BUDGET_REPORTED: Cell<bool> = const { Cell::new(false) };
}
fn module_bindings_for_child(
    parent: &State,
    definition: &FunctionBody,
) -> (BTreeMap<String, String>, BTreeMap<String, Option<Contract>>) {
    if parent.scope_module == definition.file.module {
        if parent.module_scope {
            (
                parent
                    .function_bindings
                    .iter()
                    .filter(|(name, _)| !parent.shadowed_functions.contains(*name))
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect(),
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

fn expr_name(expr: &AstExpr) -> Option<&str> {
    if let AstExprKind::Name(name) = &expr.kind {
        Some(name)
    } else {
        None
    }
}

fn external_codepact_identity(
    expression: &AstExpr,
    bindings: &BTreeMap<String, String>,
) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) => bindings
            .get(name)
            .filter(|identity| identity.starts_with("external:codepact."))
            .cloned(),
        AstExprKind::Attribute(module, member) => {
            let module = bindings
                .get(expr_name(module)?)?
                .strip_prefix("external:")?;
            ownership::external_member(module, member).map(str::to_owned)
        }
        _ => None,
    }
}

fn is_typing_literal(expression: &AstExpr, bindings: &BTreeMap<String, String>) -> bool {
    match &expression.kind {
        AstExprKind::Name(name) => bindings.get(name).is_some_and(|identity| {
            matches!(
                identity.as_str(),
                "external:typing.Literal" | "external:typing_extensions.Literal"
            )
        }),
        AstExprKind::Attribute(module, member) if member == "Literal" => expr_name(module)
            .and_then(|name| bindings.get(name))
            .is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "external:typing" | "external:typing_extensions"
                )
            }),
        _ => false,
    }
}

fn is_typing_annotated(expression: &AstExpr, bindings: &BTreeMap<String, String>) -> bool {
    match &expression.kind {
        AstExprKind::Name(name) => bindings.get(name).is_some_and(|identity| {
            matches!(
                identity.as_str(),
                "external:typing.Annotated" | "external:typing_extensions.Annotated"
            )
        }),
        AstExprKind::Attribute(module, member) if member == "Annotated" => expr_name(module)
            .and_then(|name| bindings.get(name))
            .is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "external:typing" | "external:typing_extensions"
                )
            }),
        _ => false,
    }
}

fn is_state_access(expression: &AstExpr, bindings: &BTreeMap<String, String>) -> bool {
    match &expression.kind {
        AstExprKind::Subscript(container, _) => {
            external_codepact_identity(container, bindings).as_deref()
                == Some("external:codepact.StateAccess")
        }
        AstExprKind::Call { function, .. } => {
            external_codepact_identity(function, bindings).as_deref()
                == Some("external:codepact.StateAccess")
        }
        _ => false,
    }
}

fn state_values_from_literal(
    expression: &AstExpr,
    bindings: &BTreeMap<String, String>,
) -> Option<Vec<String>> {
    let AstExprKind::Subscript(container, values) = &expression.kind else {
        return None;
    };
    if !is_typing_literal(container, bindings) {
        return None;
    }
    values
        .iter()
        .map(literal_state_values)
        .collect::<Option<Vec<_>>>()
        .map(|values| values.into_iter().flatten().collect())
}

fn protocol_precondition_for_method(
    file: &SourceFile,
    method: &crate::frontend::MethodBody,
    bindings: &BTreeMap<String, String>,
    machine: Option<&ProtocolMachine>,
    result: &mut Inspection,
    report_errors: bool,
) -> Option<(BTreeSet<String>, Span)> {
    let receiver = method.parameters.first()?;
    let annotation = method
        .parameter_annotations
        .iter()
        .find(|(name, _)| name == receiver)
        .map(|(_, annotation)| annotation)?;
    let invalid = |result: &mut Inspection| {
        if report_errors {
            protocol_declaration_error(
                result,
                file,
                annotation.range,
                "State receiver annotations require State[Type, Literal[state, ...]]",
            );
        }
        None
    };
    let AstExprKind::Subscript(container, arguments) = &annotation.kind else {
        return None;
    };
    let state_values = if external_codepact_identity(container, bindings).as_deref()
        == Some("external:codepact.State")
    {
        if arguments.len() != 2 {
            return invalid(result);
        }
        let Some(values) = state_values_from_literal(&arguments[1], bindings) else {
            return invalid(result);
        };
        values
    } else if is_typing_annotated(container, bindings) {
        let mut requirements = Vec::<Vec<String>>::new();
        for metadata in arguments.iter().skip(1) {
            if !is_state_access(metadata, bindings) {
                continue;
            }
            let AstExprKind::Subscript(_, values) = &metadata.kind else {
                return invalid(result);
            };
            let Some(values) = values
                .first()
                .and_then(|value| state_values_from_literal(value, bindings))
            else {
                return invalid(result);
            };
            requirements.push(values);
        }
        if requirements.is_empty() {
            return None;
        }
        let first = requirements[0].iter().cloned().collect::<BTreeSet<_>>();
        if requirements
            .iter()
            .any(|states| states.iter().cloned().collect::<BTreeSet<_>>() != first)
        {
            return invalid(result);
        }
        requirements.remove(0)
    } else {
        return None;
    };
    let states = state_values.iter().cloned().collect::<BTreeSet<_>>();
    if states.is_empty()
        || states.len() != state_values.len()
        || states
            .iter()
            .any(|state| !crate::is_python_identifier(state, "3.13"))
        || machine.is_none_or(|machine| !states.iter().all(|state| machine.states.contains(state)))
    {
        return invalid(result);
    }
    Some((states, span(file, annotation.range)))
}

type CallParts<'a> = (&'a AstExpr, &'a [AstExpr], &'a [(Option<String>, AstExpr)]);

fn call_parts(expression: &AstExpr) -> Option<CallParts<'_>> {
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &expression.kind
    else {
        return None;
    };
    Some((function, arguments, keywords))
}

fn literal_state_values(expression: &AstExpr) -> Option<Vec<String>> {
    match &expression.kind {
        AstExprKind::String(name) => Some(vec![name.clone()]),
        AstExprKind::Tuple(items) | AstExprKind::Collection(items) => items
            .iter()
            .map(|item| match &item.kind {
                AstExprKind::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn named_keyword<'a>(keywords: &'a [(Option<String>, AstExpr)], name: &str) -> Option<&'a AstExpr> {
    let mut matching = keywords
        .iter()
        .filter(|(key, _)| key.as_deref() == Some(name));
    let value = matching.next().map(|(_, value)| value)?;
    matching.next().is_none().then_some(value)
}

fn protocol_declaration_error(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    message: &str,
) {
    result.coverage.status = "incomplete".into();
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
        code: "TP4002".into(),
        severity: "error".into(),
        message: message.into(),
        primary: span(file, range),
        related: Vec::new(),
        certainty: "definite".into(),
    });
}

fn protocol_machine_for_class(
    file: &SourceFile,
    decorators: &[AstExpr],
    bindings: &BTreeMap<String, String>,
    result: &mut Inspection,
    report_errors: bool,
) -> Option<ProtocolMachine> {
    let mut machine: Option<ProtocolMachine> = None;
    for decorator in decorators {
        let Some((function, arguments, keywords)) = call_parts(decorator) else {
            continue;
        };
        if external_codepact_identity(function, bindings).as_deref()
            != Some("external:codepact.state_machine")
        {
            continue;
        }
        let mut invalid = || {
            report_errors.then(|| {
                protocol_declaration_error(
                    result,
                    file,
                    decorator.range,
                    "state_machine requires literal states and one initial state",
                )
            });
            None
        };
        if !arguments.is_empty() || keywords.len() != 2 {
            return invalid();
        }
        let Some(state_values) = named_keyword(keywords, "states").and_then(literal_state_values)
        else {
            return invalid();
        };
        let states = state_values.iter().cloned().collect::<BTreeSet<_>>();
        let Some(initial) =
            named_keyword(keywords, "initial").and_then(|value| match &value.kind {
                AstExprKind::String(name) => Some(name.clone()),
                _ => None,
            })
        else {
            return invalid();
        };
        if states.is_empty()
            || states.len() > MAX_PROTOCOL_STATES
            || states.len() != state_values.len()
            || states
                .iter()
                .any(|state| !crate::is_python_identifier(state, "3.13"))
            || !states.contains(&initial)
        {
            return invalid();
        }
        let next = ProtocolMachine {
            states,
            initial,
            origin: span(file, decorator.range),
        };
        if machine.as_ref().is_some_and(|previous| {
            previous.states != next.states || previous.initial != next.initial
        }) {
            if report_errors {
                protocol_declaration_error(
                    result,
                    file,
                    decorator.range,
                    "conflicting state_machine declarations",
                );
            }
            return None;
        }
        machine = Some(next);
    }
    machine
}

fn protocol_transition_for_method(
    file: &SourceFile,
    decorators: &[AstExpr],
    bindings: &BTreeMap<String, String>,
    machine: Option<&ProtocolMachine>,
    result: &mut Inspection,
    report_errors: bool,
) -> Option<ProtocolTransition> {
    let mut transition: Option<ProtocolTransition> = None;
    for decorator in decorators {
        let Some((function, arguments, keywords)) = call_parts(decorator) else {
            continue;
        };
        if external_codepact_identity(function, bindings).as_deref()
            != Some("external:codepact.transition")
        {
            continue;
        }
        let mut invalid = || {
            report_errors.then(|| {
                protocol_declaration_error(
                    result,
                    file,
                    decorator.range,
                    "transition requires literal from_state and to_state values",
                )
            });
            None
        };
        if !arguments.is_empty()
            || keywords.iter().any(|(name, _)| {
                !matches!(
                    name.as_deref(),
                    Some("from_state" | "to_state" | "on_raise" | "on_cancel")
                )
            })
        {
            return invalid();
        }
        let Some(from_values) =
            named_keyword(keywords, "from_state").and_then(literal_state_values)
        else {
            return invalid();
        };
        let from_states = from_values.iter().cloned().collect::<BTreeSet<_>>();
        let Some(normal_state) =
            named_keyword(keywords, "to_state").and_then(|value| match &value.kind {
                AstExprKind::String(name) => Some(name.clone()),
                _ => None,
            })
        else {
            return invalid();
        };
        let state_option = |name| {
            named_keyword(keywords, name).and_then(|value| match &value.kind {
                AstExprKind::String(state) => Some(state.clone()),
                _ => None,
            })
        };
        let exceptional_state = state_option("on_raise");
        let cancelled_state = state_option("on_cancel");
        let valid = machine.is_some_and(|machine| {
            !from_states.is_empty()
                && from_states.len() == from_values.len()
                && from_states
                    .iter()
                    .all(|state| machine.states.contains(state))
                && machine.states.contains(&normal_state)
                && exceptional_state
                    .as_ref()
                    .is_none_or(|state| machine.states.contains(state))
                && cancelled_state
                    .as_ref()
                    .is_none_or(|state| machine.states.contains(state))
        });
        if !valid {
            if report_errors {
                protocol_declaration_error(
                    result,
                    file,
                    decorator.range,
                    "transition references a missing state_machine or undeclared state",
                );
            }
            return None;
        }
        let next = ProtocolTransition {
            from_states,
            normal_state,
            exceptional_state,
            cancelled_state,
            origin: span(file, decorator.range),
        };
        if transition.as_ref().is_some_and(|previous| {
            previous.from_states != next.from_states
                || previous.normal_state != next.normal_state
                || previous.exceptional_state != next.exceptional_state
                || previous.cancelled_state != next.cancelled_state
        }) {
            if report_errors {
                protocol_declaration_error(
                    result,
                    file,
                    decorator.range,
                    "conflicting transition declarations",
                );
            }
            return None;
        }
        transition = Some(next);
    }
    transition
}

fn is_awaited_call(expr: &AstExpr, file: &SourceFile, state: &State) -> bool {
    state
        .awaited_call
        .as_ref()
        .is_some_and(|(path, range)| path == &file.path && range == &expr.range)
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

fn expression_may_return_resource(expression: &AstExpr) -> bool {
    match &expression.kind {
        AstExprKind::NoneLiteral
        | AstExprKind::String(_)
        | AstExprKind::Boolean(_)
        | AstExprKind::EvaluatedSelect { .. }
        | AstExprKind::Primitive => false,
        AstExprKind::Tuple(items) => items.iter().any(expression_may_return_resource),
        AstExprKind::Name(_)
        | AstExprKind::Attribute(_, _)
        | AstExprKind::Subscript(_, _)
        | AstExprKind::Call { .. }
        | AstExprKind::Await(_)
        | AstExprKind::Yield(_)
        | AstExprKind::Other
        | AstExprKind::OtherCalls(..)
        | AstExprKind::Collection(_)
        | AstExprKind::Starred(_)
        | AstExprKind::Closure(_)
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
        FlowStatement::Match { cases, .. } => cases
            .iter()
            .any(|case| statements_may_return_resource(&case.body)),
        FlowStatement::Try {
            body,
            handlers,
            alternate,
            finalbody,
            ..
        } => {
            statements_may_return_resource(body)
                || handlers
                    .iter()
                    .any(|handler| statements_may_return_resource(&handler.body))
                || statements_may_return_resource(alternate)
                || statements_may_return_resource(finalbody)
        }
        FlowStatement::Loop {
            body, alternate, ..
        } => statements_may_return_resource(body) || statements_may_return_resource(alternate),
        FlowStatement::With { body, .. } => statements_may_return_resource(body),
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
    if tasks::returns_task_handle_or_status(expression, env)
        || asyncio::returns_task(expression, env)
        || process::returns_status(expression, env)
        || outcomes::call_contract(expression, env, functions).is_some()
    {
        return false;
    }
    if async_protocols::is_constructor(expression, env) {
        return true;
    }
    if async_protocols::is_method(expression, env) {
        return false;
    }
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

fn weaken(state: &mut State, objects: impl Iterator<Item = String>) {
    for object in objects {
        let is_protocol = state
            .resources
            .get(&object)
            .is_some_and(|resource| state.protocol_machines.contains_key(&resource.base_symbol));
        *state.objects.entry(object.clone()).or_default() |= UNKNOWN;
        if is_protocol {
            state.protocol_unknown.insert(object);
        }
    }
    for object in state.escaped.clone() {
        let is_protocol = state
            .resources
            .get(&object)
            .is_some_and(|resource| state.protocol_machines.contains_key(&resource.base_symbol));
        *state.objects.entry(object.clone()).or_default() |= UNKNOWN;
        if is_protocol {
            state.protocol_unknown.insert(object);
        }
    }
}

fn closed_mask(previous: u8, escaped: bool) -> u8 {
    // A successful call to the declared close method establishes its normal
    // postcondition even when an earlier non-escaping path made the prior
    // lifecycle state unknown. If an opaque actor retained the resource, keep
    // the prior alternatives because it may still act after this close.
    if escaped {
        previous | CLOSED
    } else {
        CLOSED
    }
}

fn modeled_close_for_object(
    state: &State,
    object: &str,
    method: &str,
) -> Option<ResourceEffectModel> {
    let resource = state.resources.get(object)?;
    let lifecycle = resource.lifecycle.as_ref()?;
    if lifecycle.closed_by != method {
        return None;
    }
    state
        .models
        .resolve(&resource.base_symbol, method, &state.target_python_version)
        .cloned()
}

fn modeled_close_for_call(
    expression: &AstExpr,
    state: &State,
) -> Option<(String, ResourceEffectModel)> {
    let (receiver, method, _, _) = called_method(expression)?;
    let receiver = expr_name(receiver)?;
    if state.unknown_places.contains(receiver) {
        return None;
    }
    let targets = state.places.get(receiver)?;
    if targets.len() != 1 {
        return None;
    }
    let object = targets.iter().next()?.clone();
    modeled_close_for_object(state, &object, method).map(|model| (object, model))
}

fn apply_lifecycle_postcondition(
    state: &mut State,
    object: &str,
    postcondition: LifecyclePostcondition,
    close_origin: Option<Span>,
) {
    let previous = state.objects.get(object).copied().unwrap_or(UNKNOWN);
    apply_lifecycle_postcondition_from(state, object, previous, postcondition, close_origin);
}

fn apply_lifecycle_postcondition_from(
    state: &mut State,
    object: &str,
    previous: u8,
    postcondition: LifecyclePostcondition,
    close_origin: Option<Span>,
) {
    let next = match postcondition {
        LifecyclePostcondition::Preserve => previous,
        LifecyclePostcondition::Open => OPEN,
        LifecyclePostcondition::Closed => CLOSED,
        LifecyclePostcondition::Unknown => previous | UNKNOWN,
    };
    state.objects.insert(object.to_owned(), next);
    if postcondition == LifecyclePostcondition::Closed {
        if let Some(origin) = close_origin {
            state.close_origins.insert(object.to_owned(), origin);
        }
    }
}

type FunctionMap = BTreeMap<String, FunctionBody>;

fn assigned_names(statements: &[FlowStatement]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_assigned_names(statements, &mut names);
    names
}

fn synthetic_name(state: &State, role: &str, range: SourceRange, index: usize) -> String {
    let base = format!("__codepact_{role}_{}_{}", range.start, index);
    let mut candidate = base.clone();
    let mut suffix = 0usize;
    while state.places.contains_key(&candidate)
        || state.unknown_places.contains(&candidate)
        || state.shadowed_functions.contains(&candidate)
        || state.shadowed_classes.contains(&candidate)
        || state.function_bindings.contains_key(&candidate)
        || state.class_bindings.contains_key(&candidate)
    {
        suffix = suffix.saturating_add(1);
        candidate = format!("{base}_{suffix}");
    }
    candidate
}

fn protocol_guard_identity(file: &SourceFile, origin: &Span) -> String {
    if origin.file != file.path {
        return format!(
            "{}:{}:{}:{}:{}",
            origin.file, origin.line, origin.column, origin.end_line, origin.end_column
        );
    }
    crate::adoption::review_site_anchor(
        &file.source,
        origin,
        &mut crate::adoption::ReviewAnchorCache::default(),
    )
}

fn collect_assigned_names(statements: &[FlowStatement], names: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            FlowStatement::Assign { name, .. } | FlowStatement::EscapingAssign { name, .. } => {
                names.insert(name.clone());
            }
            FlowStatement::AugmentedStore { target, .. } => {
                if let Some(name) = expr_name(target) {
                    names.insert(name.to_owned());
                }
            }
            FlowStatement::If {
                body, alternate, ..
            } => {
                collect_assigned_names(body, names);
                collect_assigned_names(alternate, names);
            }
            FlowStatement::Match { cases, .. } => {
                for case in cases {
                    collect_pattern_names(&case.pattern, names);
                    collect_assigned_names(&case.body, names);
                }
            }
            FlowStatement::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                collect_assigned_names(body, names);
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        names.insert(name.clone());
                    }
                    collect_assigned_names(&handler.body, names);
                }
                collect_assigned_names(alternate, names);
                collect_assigned_names(finalbody, names);
            }
            FlowStatement::Loop {
                target,
                body,
                alternate,
                ..
            } => {
                if let Some(target) = target {
                    names.insert(target.clone());
                }
                collect_assigned_names(body, names);
                collect_assigned_names(alternate, names);
            }
            FlowStatement::With { items, body, .. } => {
                for target in items.iter().filter_map(|item| item.target.as_ref()) {
                    names.insert(target.clone());
                }
                collect_assigned_names(body, names);
            }
            _ => {}
        }
    }
}

fn collect_pattern_names(pattern: &crate::frontend::AstPattern, names: &mut BTreeSet<String>) {
    use crate::frontend::AstPattern as Pattern;
    match pattern {
        Pattern::Class {
            positional,
            keywords,
            ..
        } => {
            for pattern in positional {
                collect_pattern_names(pattern, names);
            }
            for (_, pattern) in keywords {
                collect_pattern_names(pattern, names);
            }
        }
        Pattern::As { pattern, name, .. } => {
            if let Some(name) = name {
                names.insert(name.clone());
            }
            if let Some(pattern) = pattern {
                collect_pattern_names(pattern, names);
            }
        }
        Pattern::Or { patterns, .. } => {
            for pattern in patterns {
                collect_pattern_names(pattern, names);
            }
        }
        Pattern::Capture {
            name: Some(name), ..
        } => {
            names.insert(name.clone());
        }
        Pattern::Capture { name: None, .. } | Pattern::Unsupported { .. } => {}
    }
}

fn copy_object_metadata(child: &mut State, parent: &State, objects: &BTreeSet<String>) {
    child.protocol_machines = parent.protocol_machines.clone();
    child
        .async_resources
        .copy_from(&parent.async_resources, objects);
    if let (Some(child), Some(parent)) = (child.ownership.as_mut(), parent.ownership.as_ref()) {
        child
            .async_protocols
            .copy_from(&parent.async_protocols, objects);
    }
    for object in objects {
        if let (Some(child), Some(parent)) = (child.ownership.as_mut(), parent.ownership.as_ref()) {
            child.copy_transfers(parent, object);
        }
        if let Some(ownership) = child.ownership.as_mut() {
            ownership.inherit_capture_sites(parent, object);
        }
        if let Some(mask) = parent.objects.get(object) {
            child.objects.insert(object.clone(), *mask);
        }
        if let Some(resource) = parent.resources.get(object) {
            child.resources.insert(object.clone(), resource.clone());
        }
        if let Some(states) = parent.protocol_states.get(object) {
            child.protocol_states.insert(object.clone(), states.clone());
        }
        if parent.protocol_unknown.contains(object) {
            child.protocol_unknown.insert(object.clone());
        }
        if parent.borrowed.contains(object) {
            child.borrowed.insert(object.clone());
        }
        if parent.exclusive.contains(object) {
            child.exclusive.insert(object.clone());
            if let Some(origin) = parent.exclusive_origins.get(object) {
                child
                    .exclusive_origins
                    .insert(object.clone(), origin.clone());
            }
            if let Some(views) = parent.exclusive_views.get(object) {
                child.exclusive_views.insert(object.clone(), views.clone());
            }
        }
        if parent.forbidden_borrowed_closes.contains(object) {
            child.forbidden_borrowed_closes.insert(object.clone());
        }
        if parent.ambiguous_borrowed_owned_aliases.contains(object) {
            child
                .ambiguous_borrowed_owned_aliases
                .insert(object.clone());
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
        if let Some(origin) = parent.duty_origins.get(object) {
            child.duty_origins.insert(object.clone(), origin.clone());
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
            contract_identity(definition.parameter_contracts.get(parameter)),
        );
    }
}

fn check_protocol_precondition(
    definition: &FunctionBody,
    object: &str,
    env: &State,
    file: &SourceFile,
    call_range: SourceRange,
    result: &mut Inspection,
) -> (BTreeSet<String>, bool) {
    let Some(required_states) = definition
        .protocol_transition
        .as_ref()
        .map(|transition| &transition.from_states)
        .or(definition.protocol_precondition.as_ref())
    else {
        return (BTreeSet::new(), false);
    };
    let states = env.protocol_states.get(object).cloned().unwrap_or_default();
    let unknown = env.protocol_unknown.contains(object) || states.is_empty();
    let has_satisfied = states.iter().any(|state| required_states.contains(state));
    let has_violated = states.iter().any(|state| !required_states.contains(state));
    if has_violated {
        let machine_origin = env
            .resources
            .get(object)
            .and_then(|resource| env.protocol_machines.get(&resource.base_symbol))
            .map(|machine| machine.origin.clone());
        let mut related = definition
            .protocol_transition
            .as_ref()
            .map(|transition| transition.origin.clone())
            .or_else(|| definition.protocol_precondition_origin.clone())
            .into_iter()
            .collect::<Vec<_>>();
        if let Some(origin) = machine_origin {
            if !related.contains(&origin) {
                related.push(origin);
            }
        }
        report(
            result,
            "TP2102",
            "operation may be called outside its declared protocol state",
            span(file, call_range),
            related,
            if has_satisfied || unknown {
                "possible"
            } else {
                "definite"
            },
            contract_identity(env.resources.get(object)),
        );
    }
    if unknown {
        boundary(
            result,
            file,
            call_range,
            "protocol precondition cannot be established on every path",
        );
    }
    (states, unknown)
}

fn root_place_name(expression: &AstExpr) -> Option<&str> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(name),
        AstExprKind::Attribute(base, _) | AstExprKind::Subscript(base, _) => root_place_name(base),
        _ => None,
    }
}

fn report_exclusive_alias_access(
    env: &mut State,
    alias: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    message: &str,
) {
    let Some(targets) = env.places.get(alias).cloned() else {
        return;
    };
    for object in targets {
        if !env.exclusive.contains(&object)
            || env
                .exclusive_views
                .get(&object)
                .is_some_and(|views| views.contains(alias))
        {
            continue;
        }
        report(
            result,
            "TP2101",
            message,
            span(file, range),
            env.exclusive_origins
                .get(&object)
                .cloned()
                .into_iter()
                .collect(),
            "definite",
            contract_identity(env.resources.get(&object)),
        );
        *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
        if env
            .resources
            .get(&object)
            .is_some_and(|resource| env.protocol_machines.contains_key(&resource.base_symbol))
        {
            env.protocol_unknown.insert(object.clone());
            boundary(
                result,
                file,
                range,
                "competing alias access makes the protocol state unknown",
            );
        }
    }
}

struct ProtocolCompletionUpdate<'a> {
    object: &'a str,
    incoming: &'a BTreeSet<String>,
    incoming_unknown: bool,
    transition: &'a ProtocolTransition,
    completion: &'a CompletionKind,
    file: &'a SourceFile,
    call_range: SourceRange,
}

fn apply_protocol_completion(
    state: &mut State,
    update: ProtocolCompletionUpdate<'_>,
    result: &mut Inspection,
) {
    let ProtocolCompletionUpdate {
        object,
        incoming,
        incoming_unknown,
        transition,
        completion,
        file,
        call_range,
    } = update;
    let postcondition = match completion {
        CompletionKind::Fallthrough | CompletionKind::Return(_) => {
            Some(transition.normal_state.as_str())
        }
        CompletionKind::Raise(_) => transition.exceptional_state.as_deref(),
        CompletionKind::Cancelled(_) => transition.cancelled_state.as_deref(),
        CompletionKind::Break | CompletionKind::Continue => None,
    };
    let mut outcomes = BTreeSet::new();
    let mut unknown = incoming_unknown || incoming.is_empty();
    for state_name in incoming {
        if transition.from_states.contains(state_name) {
            if let Some(postcondition) = postcondition {
                outcomes.insert(postcondition.to_owned());
            } else {
                unknown = true;
            }
        } else {
            unknown = true;
        }
    }
    // A declared per-exit state is the contract for that completion edge.
    // It may re-establish a known state after unsupported body effects, while
    // an unknown entry state or a failed precondition still prevents proof.
    if postcondition.is_none() || unknown {
        unknown |= state.protocol_unknown.contains(object);
    }
    state.protocol_states.insert(object.to_owned(), outcomes);
    if unknown {
        state.protocol_unknown.insert(object.to_owned());
        boundary(
            result,
            file,
            call_range,
            "protocol completion state is unknown for this outcome",
        );
    } else {
        state.protocol_unknown.remove(object);
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
        call_site,
        caller,
    } = invocation;
    let (module_function_bindings, module_class_bindings) =
        module_bindings_for_child(parent, definition);
    let mut child = State {
        ownership: OwnershipState::for_child(parent, false),
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
        loop_depth: parent.loop_depth,
        awaited_call: None,
        allocation_context: {
            let mut context = parent.allocation_context.clone();
            context.push(call_site);
            context
        },
        target_python_version: parent.target_python_version.clone(),
        models: parent.models.clone(),
        scope_module: definition.file.module.clone(),
        async_context: definition.is_async || parent.async_context,
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
        .definitely_bound_names
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
        .insert(receiver.clone(), BTreeSet::from([object.to_owned()]));
    copy_object_metadata(&mut child, parent, &BTreeSet::from([object.to_owned()]));
    child.objects.insert(object.to_owned(), initial_mask);
    if child.exclusive.contains(object) {
        child
            .exclusive_views
            .entry(object.to_owned())
            .or_default()
            .insert(receiver);
    }
    let bindings = bind_arguments(&definition.signature, arguments, keywords, true)
        .expect("method arguments were validated before execution");
    for (parameter, argument) in bindings {
        if let Some(actual) = expr_name(argument) {
            if let Some(targets) = parent.places.get(actual) {
                child.places.insert(parameter.clone(), targets.clone());
                copy_object_metadata(&mut child, parent, targets);
                if parent.unknown_places.contains(actual) {
                    child.unknown_places.insert(parameter.clone());
                }
            }
        }
        if parent.ownership.is_some() {
            ownership::verify_call_binding(
                definition,
                &parameter,
                argument,
                parent,
                &mut child,
                &span(caller, argument.range),
                context.result,
            );
        }
    }
    guarded_access::inherit(parent, &mut child);
    let returned = execute(
        &definition.statements,
        &mut child,
        &definition.file,
        &definition.contracts,
        context.functions,
        context.result,
        0,
    );
    let raised = returned.raises;
    let mut normal = join_normal_exits(
        child,
        returned
            .returns
            .into_iter()
            .map(|exit| exit.state)
            .collect(),
    );
    normal
        .pending_raises
        .extend(raised.into_iter().map(|mut exit| {
            exit.state.pending_raises.clear();
            exit.state.alive = true;
            exit
        }));
    normal
}

fn apply_call_object_effects(
    parent: &mut State,
    child: &State,
    extra_objects: &BTreeSet<String>,
    object: &str,
) {
    parent
        .async_resources
        .copy_io_handle(&child.async_resources, object);
    if let (Some(parent), Some(child)) = (parent.ownership.as_mut(), child.ownership.as_ref()) {
        parent.copy_transfers(child, object);
    }
    if let Some(mask) = child.objects.get(object) {
        parent.objects.insert(object.to_owned(), *mask);
    }
    if let Some(resource) = child.resources.get(object) {
        parent.resources.insert(object.to_owned(), resource.clone());
    }
    if let Some(states) = child.protocol_states.get(object) {
        parent
            .protocol_states
            .insert(object.to_owned(), states.clone());
    }
    if child.protocol_unknown.contains(object) {
        parent.protocol_unknown.insert(object.to_owned());
    } else {
        parent.protocol_unknown.remove(object);
    }
    if let Some(origin) = child.close_origins.get(object) {
        parent
            .close_origins
            .insert(object.to_owned(), origin.clone());
    }
    if !parent.duty_origins.contains_key(object) && extra_objects.contains(object) {
        if let Some(origin) = child.duty_origins.get(object) {
            parent
                .duty_origins
                .insert(object.to_owned(), origin.clone());
        }
    }
    if child.escaped.contains(object) {
        parent.escaped.insert(object.to_owned());
    }
}

fn apply_call_effects(parent: &mut State, child: &State, extra_objects: &BTreeSet<String>) {
    if let (Some(parent), Some(child)) = (parent.ownership.as_mut(), child.ownership.as_ref()) {
        parent.apply_runtime_effects(child);
        parent
            .async_protocols
            .apply(&child.async_protocols, extra_objects);
    }
    // A call child contains the caller-reachable arguments and escaped objects,
    // plus resources created locally by that function. Transfer only keys that
    // already belong to the caller or cross the boundary as an explicit result.
    // Walking every caller object and cloning its Contract made repeated helper
    // calls quadratic for large resource sets.
    let candidates = child
        .objects
        .keys()
        .chain(child.resources.keys())
        .chain(child.protocol_states.keys())
        .chain(child.protocol_unknown.iter())
        .chain(child.close_origins.keys())
        .chain(child.escaped.iter())
        .map(String::as_str)
        .chain(extra_objects.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    for object in candidates {
        if parent.objects.contains_key(object) || extra_objects.contains(object) {
            if child.forbidden_borrowed_closes.contains(object) {
                if child.escaped.contains(object) {
                    parent.escaped.insert(object.to_owned());
                }
                continue;
            }
            apply_call_object_effects(parent, child, extra_objects, object);
        }
    }
    parent.pending_return_targets = child.pending_return_targets.clone();
    parent.coroutine_budget_overflow = child
        .coroutine_budget_overflow
        .or(parent.coroutine_budget_overflow);
}

fn statement_range(statement: &FlowStatement) -> SourceRange {
    match statement {
        FlowStatement::Pass { range }
        | FlowStatement::Assign { range, .. }
        | FlowStatement::EscapingAssign { range, .. }
        | FlowStatement::AnnotatedDeclaration { range, .. }
        | FlowStatement::Expression { range, .. }
        | FlowStatement::If { range, .. }
        | FlowStatement::Match { range, .. }
        | FlowStatement::Try { range, .. }
        | FlowStatement::Loop { range, .. }
        | FlowStatement::With { range, .. }
        | FlowStatement::Return { range, .. }
        | FlowStatement::Raise { range, .. }
        | FlowStatement::Break { range }
        | FlowStatement::Continue { range }
        | FlowStatement::BindFunction { range, .. }
        | FlowStatement::Store { range, .. }
        | FlowStatement::AugmentedStore { range, .. }
        | FlowStatement::ClosureDefinition { range, .. }
        | FlowStatement::DeleteLocal { range, .. }
        | FlowStatement::Unsupported { range } => *range,
    }
}

pub(crate) struct FlowOptions<'a> {
    pub(crate) python_version: &'a str,
    pub(crate) models: &'a FlowModelCatalog,
    pub(crate) cyclic_modules: &'a BTreeSet<String>,
    pub(crate) ownership: crate::Ownership,
}

pub(crate) fn check_file_bodies(
    target_path: &str,
    files: &[Arc<SourceFile>],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
    options: FlowOptions<'_>,
) {
    let FlowOptions {
        python_version,
        models,
        cyclic_modules,
        ownership,
    } = options;
    let classes_with_unresolved_bases = files
        .iter()
        .flat_map(|file| {
            summaries
                .get(&file.path)
                .and_then(|summary| summary.declarations.as_ref())
                .into_iter()
                .flatten()
                .filter_map(move |declaration| match declaration {
                    Declaration::Class {
                        name,
                        has_unresolved_base_classes: true,
                        ..
                    } => Some(format!("{}.{}", file.module, name)),
                    _ => None,
                })
        })
        .collect::<BTreeSet<_>>();
    let supplied_modules = files
        .iter()
        .map(|file| file.module.clone())
        .collect::<BTreeSet<_>>();
    let mut project_functions = ProjectFunctionMap::new();
    let mut historical_functions = FunctionMap::new();
    let mut outcome_declarations = BTreeMap::new();
    let mut class_generations = BTreeMap::<(String, String), usize>::new();
    for file in files {
        if file.path.starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
            && models
                .foundations
                .as_ref()
                .is_none_or(|inputs| !inputs.providers.contains_key(&file.module))
        {
            continue;
        }
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        let outcome_contracts = outcomes::scan_declarations(declarations, &supplied_modules);
        if file.path == target_path {
            outcome_contracts.report_issues(result, file);
        }
        let module_contracts: Arc<[Contract]> = contracts_for_module(
            file,
            files,
            summaries,
            result,
            models.foundations.as_deref(),
        )
        .into();
        for (declaration_index, declaration) in declarations.iter().enumerate() {
            match declaration {
                Declaration::Function {
                    name,
                    parameters,
                    signature,
                    body,
                    is_async,
                    is_generator,
                    decorators,
                    must_use_decorator,
                    range,
                    unsupported,
                    ..
                } => {
                    let async_generator = (*is_async
                        && *is_generator
                        && ownership.is_enabled()
                        && models.foundations.is_some())
                    .then(|| {
                        async_resources::definition(
                            body,
                            decorators,
                            &declarations[..declaration_index],
                            &supplied_modules,
                        )
                    })
                    .flatten()
                    .map(Arc::new);
                    if *unsupported
                        || (async_generator.is_none()
                            && ((!decorators.is_empty() && !*must_use_decorator) || *is_generator))
                    {
                        continue;
                    }
                    let binding_key = format!("{}::{}:{}", file.module, name, range.start);
                    let function = FunctionBody {
                        file: Arc::clone(file),
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
                        return_access: module_contracts
                            .iter()
                            .find(|contract| {
                                contract.display_name == format!("{name}.return")
                                    && contract.lifecycle.is_some()
                            })
                            .map(|contract| contract.access.clone()),
                        outcome_return: outcome_contracts.returns.get(&range.start).copied(),
                        statements: Arc::from(body.clone()),
                        supported_binding: true,
                        is_async: *is_async,
                        async_generator,
                        protocol_transition: None,
                        protocol_precondition: None,
                        protocol_precondition_origin: None,
                        binding_key: Some(binding_key.clone()),
                        module_bindings: Arc::new(ModuleBindings::default()),
                        publication: Vec::new(),
                        modeled_factory: None,
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
                        if method.has_parameter_annotations
                            && !ownership.includes_advanced_contracts()
                        {
                            boundary(
                                result,
                                file,
                                method.range,
                                "method parameter annotations are not modeled",
                            );
                        }
                        let method_key = format!("@{class_identity}.{}", method.name);
                        let definition = FunctionBody {
                            file: Arc::clone(file),
                            parameters: method.parameters.clone(),
                            signature: method.signature.clone(),
                            contracts: Arc::clone(&module_contracts),
                            parameter_contracts: BTreeMap::new(),
                            return_access: None,
                            outcome_return: outcome_contracts
                                .returns
                                .get(&method.range.start)
                                .copied(),
                            statements: Arc::from(method.body.clone()),
                            supported_binding: (!method.decorated || method.must_use_decorator)
                                && !method.is_generator,
                            is_async: method.is_async,
                            async_generator: None,
                            protocol_transition: None,
                            protocol_precondition: None,
                            protocol_precondition_origin: None,
                            binding_key: Some(format!(
                                "{}::{}:{}:{}",
                                file.module, method_key, range.start, method.range.start
                            )),
                            module_bindings: Arc::new(ModuleBindings::default()),
                            publication: Vec::new(),
                            modeled_factory: None,
                        };
                        historical_functions
                            .insert(definition.binding_key.clone().unwrap(), definition.clone());
                        project_functions.insert((file.module.clone(), method_key), definition);
                    }
                }
                _ => {}
            }
        }
        outcome_declarations.insert(file.path.clone(), outcome_contracts.returns);
    }
    let mut qualified_functions: FunctionMap = project_functions
        .iter()
        .map(|((module, name), body)| (format!("{module}::{name}"), body.clone()))
        .collect();
    qualified_functions.extend(historical_functions);
    let published = providers::public_functions(models, &project_functions);
    for provider in files {
        if provider.path.starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
            && models
                .foundations
                .as_ref()
                .is_none_or(|inputs| !inputs.providers.contains_key(&provider.module))
        {
            continue;
        }
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
                    &mut BTreeSet::new(),
                    &resolution::FunctionLookup {
                        files,
                        summaries,
                        functions: &project_functions,
                        cyclic_modules,
                        published: &published,
                        requester: &provider.module,
                        foundations: models.foundations.as_deref(),
                    },
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
                Declaration::Function { name, range, .. } => {
                    let key = format!("{}::{}:{}", provider.module, name, range.start);
                    final_bindings.insert(
                        name.clone(),
                        qualified_functions.contains_key(&key).then_some(key),
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
    if let Some(inputs) = &models.foundations {
        qualified_functions.retain(|key, _| {
            let Some((module, name)) = key.split_once("::") else {
                return true;
            };
            !inputs.providers.contains_key(module) || name.starts_with('@') || name.contains(':')
        });
        for ((module, name), definition) in &published {
            qualified_functions.insert(format!("{module}::{name}"), definition.clone());
            qualified_functions.insert(format!("provider:{module}.{name}"), definition.clone());
        }
    }
    let mut external_exports = BTreeMap::<String, BTreeMap<String, String>>::new();
    for _ in 0..=files.len() {
        let mut changed = false;
        for provider in files {
            if ownership.is_enabled() && cyclic_modules.contains(&provider.module) {
                continue;
            }
            let Some(declarations) = summaries
                .get(&provider.path)
                .and_then(|summary| summary.declarations.as_ref())
            else {
                continue;
            };
            let mut bindings =
                external_module_bindings(declarations, &external_exports, &supplied_modules);
            if let Some(inputs) = &models.foundations {
                providers::register_external_bindings(
                    provider,
                    declarations,
                    &mut bindings,
                    inputs,
                );
            }
            if ownership.is_enabled() {
                bindings.retain(|_, binding| {
                    !binding
                        .strip_prefix("module:")
                        .is_some_and(|module| cyclic_modules.contains(module))
                });
            }
            if external_exports.get(&provider.module) != Some(&bindings) {
                external_exports.insert(provider.module.clone(), bindings);
                changed = true;
            }
        }
        if !changed {
            break;
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
        let mut function_bindings = final_function_bindings(
            &file.module,
            &qualified_functions,
            declarations,
            &supplied_modules,
        );
        if let Some(external) = external_exports.get(&file.module) {
            function_bindings.extend(external.clone());
        }
        if ownership.is_enabled() {
            function_bindings.retain(|_, binding| {
                !binding
                    .strip_prefix("module:")
                    .is_some_and(|module| cyclic_modules.contains(module))
            });
        }
        module_bindings.insert(
            file.path.clone(),
            Arc::new(ModuleBindings {
                functions: function_bindings,
                classes: final_class_bindings(
                    file,
                    declarations,
                    files,
                    summaries,
                    result,
                    models.foundations.as_deref(),
                ),
            }),
        );
    }
    let mut protocol_machines = BTreeMap::<String, ProtocolMachine>::new();
    let mut protocol_classes = BTreeMap::<(String, String), Contract>::new();
    let mut protocol_transitions = BTreeMap::<String, Option<ProtocolTransition>>::new();
    let mut protocol_preconditions = BTreeMap::<String, (BTreeSet<String>, Span)>::new();
    let mut transition_supported = BTreeSet::<String>::new();
    let mut protocol_annotation_supported = BTreeSet::<String>::new();
    if ownership.includes_advanced_contracts() {
        for file in files {
            let Some(declarations) = summaries
                .get(&file.path)
                .and_then(|summary| summary.declarations.as_ref())
            else {
                continue;
            };
            for declaration in declarations {
                let Declaration::Class {
                    name,
                    decorators,
                    methods,
                    range,
                    ..
                } = declaration
                else {
                    continue;
                };
                let declaration_index = declarations
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, declaration))
                    .unwrap_or(declarations.len());
                let bindings = external_module_bindings(
                    &declarations[..declaration_index],
                    &external_exports,
                    &supplied_modules,
                );
                let decorator_ranges = decorators
                    .iter()
                    .map(|decorator| decorator.range)
                    .collect::<Vec<_>>();
                let resource =
                    class_resource_contract(file, name, &decorator_ranges, &result.contracts);
                let machine = protocol_machine_for_class(
                    file,
                    decorators,
                    &bindings,
                    result,
                    file.path == target_path,
                );
                let class_symbol = resource
                    .as_ref()
                    .map(|contract| contract.base_symbol.clone())
                    .unwrap_or_else(|| format!("{}.{}", file.module, name));
                if let Some(machine) = &machine {
                    protocol_machines.insert(class_symbol.clone(), machine.clone());
                    let contract = resource.unwrap_or_else(|| Contract {
                        schema_version: 0,
                        base_symbol: class_symbol.clone(),
                        display_name: name.clone(),
                        lifecycle: None,
                        access: "unspecified".into(),
                        source: machine.origin.clone(),
                        provenance: vec![crate::Provenance {
                            origin: "python-state-machine-decorator".into(),
                            trust: "declared-unverified".into(),
                        }],
                    });
                    protocol_classes.insert((file.module.clone(), name.clone()), contract);
                }
                let class_identity = class_symbol
                    .rsplit_once('.')
                    .map(|(_, class)| class.to_owned())
                    .unwrap_or_else(|| name.clone());
                for method in methods {
                    let has_transition = method.decorators.iter().any(|decorator| {
                        call_parts(decorator).is_some_and(|(function, _, _)| {
                            external_codepact_identity(function, &bindings).as_deref()
                                == Some("external:codepact.transition")
                        })
                    });
                    let binding_key = format!(
                        "{}::@{}.{}:{}:{}",
                        file.module, class_identity, method.name, range.start, method.range.start
                    );
                    let mut transition = protocol_transition_for_method(
                        file,
                        &method.decorators,
                        &bindings,
                        machine.as_ref(),
                        result,
                        file.path == target_path,
                    );
                    let mut requirement = protocol_precondition_for_method(
                        file,
                        method,
                        &bindings,
                        machine.as_ref(),
                        result,
                        file.path == target_path,
                    );
                    if !has_transition && requirement.is_none() {
                        continue;
                    }
                    if transition
                        .as_ref()
                        .zip(requirement.as_ref())
                        .is_some_and(|(transition, (states, _))| transition.from_states != *states)
                    {
                        if file.path == target_path {
                            result.coverage.status = "incomplete".into();
                            result.diagnostics.push(Diagnostic {
                                semantic_identity: Vec::new(),
                                code: "TP4002".into(),
                                severity: "error".into(),
                                message: "State receiver annotation conflicts with transition precondition".into(),
                                primary: requirement.as_ref().unwrap().1.clone(),
                                related: vec![transition.as_ref().unwrap().origin.clone()],
                                certainty: "definite".into(),
                            });
                        }
                        transition = None;
                        requirement = None;
                    }
                    if let Some(requirement) = requirement {
                        protocol_preconditions.insert(binding_key.clone(), requirement);
                        if let Some(receiver) = method.parameters.first() {
                            if !method.parameter_annotations.is_empty()
                                && method
                                    .parameter_annotations
                                    .iter()
                                    .all(|(name, _)| name == receiver)
                            {
                                protocol_annotation_supported.insert(binding_key.clone());
                            }
                        }
                    }
                    let transition_only_decorators = method.decorators.iter().all(|decorator| {
                        call_parts(decorator).is_some_and(|(function, _, _)| {
                            external_codepact_identity(function, &bindings).as_deref()
                                == Some("external:codepact.transition")
                        })
                    });
                    if transition.is_some() && transition_only_decorators && !method.is_generator {
                        transition_supported.insert(binding_key.clone());
                    }
                    protocol_transitions.insert(binding_key, transition);
                }
            }
        }

        let mut protocol_exports = BTreeMap::<String, BTreeMap<String, Contract>>::new();
        for _ in 0..=files.len() {
            let mut changed = false;
            for file in files {
                let Some(declarations) = summaries
                    .get(&file.path)
                    .and_then(|summary| summary.declarations.as_ref())
                else {
                    continue;
                };
                let mut exports = BTreeMap::new();
                for ((module, name), contract) in &protocol_classes {
                    if module == &file.module {
                        exports.insert(name.clone(), contract.clone());
                    }
                }
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
                        if let Some(contract) = protocol_exports
                            .get(module)
                            .and_then(|module| module.get(&import.name))
                        {
                            let local = import.alias.as_deref().unwrap_or(&import.name);
                            let mut contract = contract.clone();
                            contract.display_name = local.to_owned();
                            exports.insert(local.to_owned(), contract);
                        }
                    }
                }
                if protocol_exports.get(&file.module) != Some(&exports) {
                    protocol_exports.insert(file.module.clone(), exports);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for file in files {
            if let (Some(exports), Some(bindings)) = (
                protocol_exports.get(&file.module),
                module_bindings.get_mut(&file.path),
            ) {
                let classes = &mut Arc::make_mut(bindings).classes;
                for (name, contract) in exports {
                    classes.insert(name.clone(), Some(contract.clone()));
                }
            }
        }
        for function in qualified_functions.values_mut() {
            let Some(binding_key) = function.binding_key.as_ref() else {
                continue;
            };
            if let Some(transition) = protocol_transitions.get(binding_key) {
                function.protocol_transition = transition.clone();
            }
            if let Some((states, origin)) = protocol_preconditions.get(binding_key) {
                function.protocol_precondition = Some(states.clone());
                function.protocol_precondition_origin = Some(origin.clone());
            }
            if transition_supported.contains(binding_key) {
                function.supported_binding = true;
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
        for declaration in declarations {
            let Declaration::Class {
                name,
                decorators,
                methods,
                range,
                ..
            } = declaration
            else {
                continue;
            };
            let decorator_ranges = decorators
                .iter()
                .map(|decorator| decorator.range)
                .collect::<Vec<_>>();
            let resource =
                class_resource_contract(file, name, &decorator_ranges, &result.contracts);
            let class_identity = resource
                .as_ref()
                .and_then(|contract| contract.base_symbol.rsplit_once('.'))
                .map(|(_, class)| class.to_owned())
                .unwrap_or_else(|| name.clone());
            for method in methods {
                if !method.has_parameter_annotations {
                    continue;
                }
                let binding_key = format!(
                    "{}::@{}.{}:{}:{}",
                    file.module, class_identity, method.name, range.start, method.range.start
                );
                if !protocol_annotation_supported.contains(&binding_key) {
                    boundary(
                        result,
                        file,
                        method.range,
                        "method parameter annotations are not modeled",
                    );
                }
            }
        }
    }
    for function in qualified_functions.values_mut() {
        if let Some(bindings) = module_bindings.get(&function.file.path) {
            function.module_bindings = Arc::clone(bindings);
        }
    }
    let functions = &qualified_functions;
    for file in files {
        if file.path != target_path {
            continue;
        }
        let Some(declarations) = summaries
            .get(&file.path)
            .and_then(|summary| summary.declarations.as_ref())
        else {
            continue;
        };
        if let Some(foundations) = &models.foundations {
            guarded_access::validate_coroutines(
                &foundations.async_guards,
                declarations,
                file,
                result,
            );
        }
        let contracts = contracts_for_module(
            file,
            files,
            summaries,
            result,
            models.foundations.as_deref(),
        );
        let final_classes = module_bindings
            .get(&file.path)
            .map(|bindings| bindings.classes.clone())
            .unwrap_or_else(|| {
                final_class_bindings(
                    file,
                    declarations,
                    files,
                    summaries,
                    result,
                    models.foundations.as_deref(),
                )
            });
        let mut final_functions =
            final_function_bindings(&file.module, functions, declarations, &supplied_modules);
        if let Some(external) = external_exports.get(&file.module) {
            final_functions.extend(external.clone());
        }
        if ownership.is_enabled() {
            final_functions.retain(|_, binding| {
                !binding
                    .strip_prefix("module:")
                    .is_some_and(|module| cyclic_modules.contains(module))
            });
        }
        let (custom_loop, custom_wait) = if models.foundations.is_some() {
            runtime::module_runtime_mutations(declarations, &final_functions)
        } else {
            (false, false)
        };
        for declaration in declarations {
            let Declaration::Function {
                name,
                parameters,
                returns,
                body,
                range,
                is_async,
                is_generator,
                ..
            } = declaration
            else {
                continue;
            };
            if *is_generator
                && *is_async
                && functions
                    .get(&format!("{}::{}:{}", file.module, name, range.start))
                    .is_some_and(|body| body.async_generator.is_some())
            {
                continue;
            }
            let mut env = State {
                ownership: OwnershipState::new(
                    ownership,
                    contracts.iter().any(|contract| {
                        contract.display_name == format!("{name}.return")
                            && contract.lifecycle.is_some()
                            && contract.access == "owned"
                    }),
                ),
                alive: true,
                async_context: *is_async,
                scope_module: file.module.clone(),
                target_python_version: python_version.to_owned(),
                models: Arc::new(models.clone()),
                protocol_machines: protocol_machines.clone(),
                function_bindings: final_functions.clone(),
                function_bindings_complete: true,
                class_bindings: final_classes.clone(),
                class_bindings_complete: true,
                module_function_bindings: final_functions.clone(),
                module_class_bindings: final_classes.clone(),
                ..State::default()
            };
            if let Some(ownership) = env.ownership.as_mut() {
                ownership.incompatible_task_factory = custom_loop;
                ownership.processes.custom_wait = custom_wait;
            }
            env.shadowed_functions
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            env.definitely_bound_names
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            env.async_guard_models =
                guarded_access::prepare(&env, name, body, file, *range, result);
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
                if !env.objects.contains_key(&object) && env.objects.len() >= MAX_FLOW_OBJECTS {
                    env.unknown_places.insert(parameter.name.clone());
                    boundary(
                        result,
                        file,
                        *range,
                        "tracked resource object budget exceeded; lifecycle state is unknown",
                    );
                    continue;
                }
                env.objects.insert(object.clone(), OPEN);
                env.resources
                    .insert(object.clone(), parameter_contract.clone());
                env.places
                    .insert(parameter.name.clone(), BTreeSet::from([object.clone()]));
                if parameter_contract.access == "shared" {
                    env.borrowed.insert(object.clone());
                    env.borrowed_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                } else if parameter_contract.access == "owned" {
                    env.duty_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                    if let Some(ownership) = env.ownership.as_mut() {
                        ownership.own(&parameter.name, &BTreeSet::from([object]));
                    }
                } else if parameter_contract.access == "exclusive"
                    && ownership.includes_advanced_contracts()
                {
                    env.borrowed.insert(object.clone());
                    env.borrowed_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                    env.exclusive.insert(object.clone());
                    env.exclusive_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                    env.exclusive_views
                        .entry(object.clone())
                        .or_default()
                        .insert(parameter.name.clone());
                }
            }
            let return_access = contracts
                .iter()
                .find(|contract| {
                    contract.display_name == format!("{name}.return")
                        && contract.lifecycle.is_some()
                })
                .map(|contract| contract.access.as_str());
            let flow_outcomes = execute(body, &mut env, file, &contracts, functions, result, 0);
            let marked = outcome_declarations
                .get(&file.path)
                .is_some_and(|returns| returns.contains_key(&range.start));
            outcomes::report_scope(
                &env,
                &flow_outcomes,
                file,
                *range,
                result,
                marked,
                &outcomes::Values::new(),
            );
            work::report_scope(&env, &flow_outcomes, file, *range, result);
            async_protocols::report_scope(&env, &flow_outcomes, file, *range, result);
            let suggest_owned =
                suggests_owned_handoff(&flow_outcomes.returns, env.alive, return_access);
            let exits = flow_outcomes.all_abrupt().cloned().collect::<Vec<_>>();
            for exit in grouped_exits(&exits) {
                match &exit.kind {
                    CompletionKind::Return(_)
                    | CompletionKind::Raise(_)
                    | CompletionKind::Cancelled(_) => {
                        report_cleanup_exit(
                            result,
                            file,
                            &exit,
                            return_access,
                            *range,
                            returns.as_ref().map(|annotation| annotation.range),
                            suggest_owned,
                            functions,
                            &classes_with_unresolved_bases,
                        );
                    }
                    CompletionKind::Fallthrough => unreachable!(),
                    CompletionKind::Break | CompletionKind::Continue => {
                        boundary(
                            result,
                            file,
                            exit.range,
                            "loop control exits without a matching modeled loop",
                        );
                    }
                }
            }
            if env.alive {
                report_cleanup_exit(
                    result,
                    file,
                    &FlowExit {
                        state: env.clone(),
                        range: *range,
                        kind: CompletionKind::Fallthrough,
                    },
                    return_access,
                    *range,
                    returns.as_ref().map(|annotation| annotation.range),
                    suggest_owned,
                    functions,
                    &classes_with_unresolved_bases,
                );
            }
            let _ = name;
        }
        for declaration in declarations {
            let Declaration::Class {
                name,
                decorators,
                methods,
                range,
                ..
            } = declaration
            else {
                continue;
            };
            let decorator_ranges = decorators
                .iter()
                .map(|decorator| decorator.range)
                .collect::<Vec<_>>();
            let class_resource = module_bindings
                .get(&file.path)
                .and_then(|bindings| bindings.classes.get(name))
                .cloned()
                .flatten()
                .or_else(|| {
                    class_resource_contract(file, name, &decorator_ranges, &result.contracts)
                });
            for method in methods {
                if method.decorated && !method.must_use_decorator {
                    continue;
                }
                let mut env = State {
                    ownership: OwnershipState::new(ownership, false),
                    alive: true,
                    async_context: method.is_async,
                    scope_module: file.module.clone(),
                    target_python_version: python_version.to_owned(),
                    models: Arc::new(models.clone()),
                    protocol_machines: protocol_machines.clone(),
                    function_bindings: final_functions.clone(),
                    function_bindings_complete: true,
                    class_bindings: final_classes.clone(),
                    class_bindings_complete: true,
                    module_function_bindings: final_functions.clone(),
                    module_class_bindings: final_classes.clone(),
                    ..State::default()
                };
                if let Some(ownership) = env.ownership.as_mut() {
                    ownership.incompatible_task_factory = custom_loop;
                    ownership.processes.custom_wait = custom_wait;
                }
                env.shadowed_functions
                    .extend(method.parameters.iter().cloned());
                env.definitely_bound_names
                    .extend(method.parameters.iter().cloned());
                env.shadowed_functions.extend(assigned_names(&method.body));
                env.shadowed_classes
                    .extend(method.parameters.iter().cloned());
                env.shadowed_classes.extend(assigned_names(&method.body));
                if let Some(contract) = &class_resource {
                    let self_name = method
                        .parameters
                        .first()
                        .map(String::as_str)
                        .unwrap_or("self");
                    let object = format!(
                        "{}:{}:{}:{}",
                        file.path, range.start, method.range.start, self_name
                    );
                    env.objects.insert(object.clone(), OPEN);
                    env.resources.insert(object.clone(), contract.clone());
                    env.places
                        .insert(self_name.to_owned(), BTreeSet::from([object]));
                    let (module, class) = contract
                        .base_symbol
                        .rsplit_once('.')
                        .unwrap_or((&file.module, name.as_str()));
                    env.call_stack.insert(format!("@{class}.{}", method.name));
                    env.scope_module = module.to_owned();
                }
                let outcomes = execute(
                    &method.body,
                    &mut env,
                    file,
                    &contracts,
                    functions,
                    result,
                    0,
                );
                let marked = outcome_declarations
                    .get(&file.path)
                    .is_some_and(|returns| returns.contains_key(&method.range.start));
                self::outcomes::report_scope(
                    &env,
                    &outcomes,
                    file,
                    method.range,
                    result,
                    marked,
                    &self::outcomes::Values::new(),
                );
                work::report_scope(&env, &outcomes, file, method.range, result);
                async_protocols::report_scope(&env, &outcomes, file, method.range, result);
                let suggest_owned = suggests_owned_handoff(&outcomes.returns, env.alive, None);
                let exits = outcomes.all_abrupt().cloned().collect::<Vec<_>>();
                for exit in grouped_exits(&exits) {
                    match &exit.kind {
                        CompletionKind::Return(_)
                        | CompletionKind::Raise(_)
                        | CompletionKind::Cancelled(_) => {
                            report_cleanup_exit(
                                result,
                                file,
                                &exit,
                                None,
                                method.range,
                                method.returns.as_ref().map(|annotation| annotation.range),
                                suggest_owned,
                                functions,
                                &classes_with_unresolved_bases,
                            );
                        }
                        CompletionKind::Fallthrough => unreachable!(),
                        CompletionKind::Break | CompletionKind::Continue => boundary(
                            result,
                            file,
                            exit.range,
                            "loop control exits without a matching modeled loop",
                        ),
                    }
                }
                if env.alive {
                    report_cleanup_exit(
                        result,
                        file,
                        &FlowExit {
                            state: env.clone(),
                            range: method.range,
                            kind: CompletionKind::Fallthrough,
                        },
                        None,
                        method.range,
                        method.returns.as_ref().map(|annotation| annotation.range),
                        suggest_owned,
                        functions,
                        &classes_with_unresolved_bases,
                    );
                }
            }
        }
        let mut module = State {
            ownership: OwnershipState::new(ownership, false),
            alive: true,
            scope_module: file.module.clone(),
            target_python_version: python_version.to_owned(),
            models: Arc::new(models.clone()),
            protocol_machines: protocol_machines.clone(),
            function_bindings_complete: true,
            class_bindings_complete: true,
            module_scope: true,
            module_class_bindings: final_classes.clone(),
            ..State::default()
        };
        for declaration in declarations {
            if let (Some(ownership), Declaration::Import { names, range }) =
                (module.ownership.as_mut(), declaration)
            {
                for import in names {
                    let local = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.name.split('.').next().unwrap_or_default());
                    if supplied_modules.contains(&import.name) {
                        let prefix = if cyclic_modules.contains(&import.name) {
                            "unresolved-module"
                        } else {
                            "module"
                        };
                        ownership.imported_callables.insert(
                            (range.start, local.to_owned()),
                            format!("{prefix}:{}", import.name),
                        );
                    }
                }
            }
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
                if let Some(ownership) = module.ownership.as_mut().filter(|_| *level == 0) {
                    let external = providers::binding_for_import(
                        models.foundations.as_deref(),
                        &file.module,
                        source_module,
                        &import.name,
                    )
                    .or_else(|| {
                        ownership::external_member(source_module, &import.name)
                            .filter(|_| !supplied_modules.contains(source_module))
                            .map(str::to_owned)
                    })
                    .or_else(|| {
                        external_exports
                            .get(source_module)
                            .and_then(|bindings| bindings.get(&import.name))
                            .cloned()
                    });
                    if let Some(external) = external {
                        ownership
                            .imported_callables
                            .insert((range.start, local.to_owned()), external);
                    }
                }
                let imported = if *level == 0 {
                    resolve_imported_contracts(
                        source_module,
                        &import.name,
                        files,
                        summaries,
                        result,
                        &mut BTreeSet::new(),
                        (&file.module, models.foundations.as_deref()),
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
            let known_context_headers = if async_resources::applicable(&module).is_ok() {
                declarations
                    .iter()
                    .filter_map(|declaration| {
                        let Declaration::Function {
                            name,
                            range,
                            parameters,
                            returns,
                            signature,
                            ..
                        } = declaration
                        else {
                            return None;
                        };
                        // Retain the existing boundary for eager evaluation of
                        // annotations or non-literal default expressions.
                        if returns.is_some()
                            || parameters
                                .iter()
                                .any(|parameter| parameter.annotation.is_some())
                            || !signature.unknown_defaults.is_empty()
                        {
                            return None;
                        }
                        functions
                            .get(&format!("{}::{}:{}", file.module, name, range.start))
                            .and_then(|function| function.async_generator.as_ref())
                            .filter(|generator| generator.context)
                            .map(|_| range.start)
                    })
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::new()
            };
            let modeled_module_body;
            let body = if known_context_headers.is_empty() {
                &summary.module_body[..]
            } else {
                modeled_module_body = summary
                    .module_body
                    .iter()
                    .filter(|statement| {
                        !matches!(statement, FlowStatement::Unsupported { range }
                        if known_context_headers.contains(&range.start))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                &modeled_module_body
            };
            let outcomes = execute(body, &mut module, file, &contracts, functions, result, 0);
            report_module_scope_lifetimes(result, &module, &outcomes);
            report_unawaited_coroutines(&module, file, result);
            if let Some(last) = summary.module_body.last() {
                let range = statement_range(last);
                async_protocols::report_scope(&module, &outcomes, file, range, result);
                if module.alive {
                    process::report_protocol_scope(&module, file, range, result);
                }
                for exit in outcomes.all_abrupt() {
                    process::report_protocol_scope(&exit.state, file, exit.range, result);
                }
                self::outcomes::report_scope(
                    &module,
                    &outcomes,
                    file,
                    statement_range(last),
                    result,
                    false,
                    &self::outcomes::Values::new(),
                );
            }
        }
    }
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

    fn source_with_flow_budget(source: &str, budget: usize) -> Inspection {
        let file = SourceFile::new("module.py", "module", source);
        let parsed = crate::frontend::parse_frontend(source, "module.py", "module", "3.11")
            .expect("the bounded reference fixture parses");
        let decorator_span = parsed
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Class {
                    name, decorators, ..
                } if name == "C" => decorators.first().map(|decorator| decorator.range),
                _ => None,
            })
            .map(|range| span(&file, range))
            .expect("the resource class has a source decorator");
        let summaries = BTreeMap::from([(
            file.path.clone(),
            crate::db::ParseSummary {
                ok: true,
                error: None,
                error_offset: None,
                declarations: Some(parsed.declarations),
                module_body: parsed.module_body,
                absolute_imports: parsed.absolute_imports,
            },
        )]);
        let mut result = inspection();
        result.contracts.push(Contract {
            schema_version: 1,
            base_symbol: "module.C".into(),
            display_name: "C".into(),
            lifecycle: Some(Lifecycle {
                closed_by: "close".into(),
            }),
            access: "unspecified".into(),
            source: decorator_span,
            provenance: vec![Provenance {
                origin: "fixture".into(),
                trust: "declared-unverified".into(),
            }],
        });
        reset_flow_budget(budget);
        let file = Arc::new(file);
        check_file_bodies(
            &file.path,
            std::slice::from_ref(&file),
            &summaries,
            &mut result,
            FlowOptions {
                python_version: "3.11",
                models: &FlowModelCatalog::default(),
                cyclic_modules: &BTreeSet::new(),
                ownership: crate::Ownership::Disabled,
            },
        );
        result
    }

    #[test]
    fn low_flow_budget_preserves_a_visible_duty_against_a_finite_high_budget_reference() {
        let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    resource.close()\n";
        let reference = source_with_flow_budget(source, 100);
        assert_eq!(
            reference.coverage.status, "complete-for-supported-subset",
            "{reference:#?}"
        );
        assert!(!reference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"));

        let bounded = source_with_flow_budget(source, 2);
        assert_eq!(bounded.coverage.status, "incomplete", "{bounded:#?}");
        assert!(
            bounded
                .coverage
                .reasons
                .iter()
                .any(|reason| { reason.contains("lifecycle flow work budget exceeded") }),
            "{bounded:#?}"
        );
        assert!(
            bounded.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
            }),
            "the exhausted budget cannot claim the duty was discharged: {bounded:#?}"
        );
        assert_eq!(source_with_flow_budget(source, 2), bounded);
        assert_eq!(source_with_flow_budget(source, 100), reference);
    }

    #[test]
    fn unknown_context_unwind_worklist_obeys_the_flow_budget() {
        let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef cleanup():\n    resource = C()\n    with UnknownManager(), UnknownManager(), UnknownManager(), UnknownManager():\n        raise RuntimeError()\n";
        let bounded = source_with_flow_budget(source, 2);

        assert_eq!(bounded.coverage.status, "incomplete", "{bounded:#?}");
        assert!(bounded
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("lifecycle flow work budget exceeded") }));
        assert!(bounded.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }));
    }

    #[test]
    fn exception_matching_uses_known_builtin_ancestry_and_keeps_custom_types_unknown() {
        let state = State::default();
        let handler = |name: &str| AstExpr {
            range: SourceRange {
                start: 0,
                end: name.len(),
            },
            kind: AstExprKind::Name(name.into()),
        };
        let qualified_builtin_spelling = AstExpr {
            range: SourceRange { start: 0, end: 19 },
            kind: AstExprKind::Attribute(
                Box::new(AstExpr {
                    range: SourceRange { start: 0, end: 7 },
                    kind: AstExprKind::Name("errors".into()),
                }),
                "ValueError".into(),
            ),
        };
        assert_eq!(
            exception_handler_match(
                Some("ValueError"),
                Some(&qualified_builtin_spelling),
                &state
            ),
            ExceptionMatch::Possible,
            "a qualified attribute must not inherit a built-in identity from its short name"
        );
        let qualified_base_exception_spelling = AstExpr {
            range: SourceRange { start: 0, end: 21 },
            kind: AstExprKind::Attribute(
                Box::new(AstExpr {
                    range: SourceRange { start: 0, end: 7 },
                    kind: AstExprKind::Name("errors".into()),
                }),
                "BaseException".into(),
            ),
        };
        assert_eq!(
            exception_handler_match(
                Some("RuntimeError"),
                Some(&qualified_base_exception_spelling),
                &state
            ),
            ExceptionMatch::Possible,
            "a qualified attribute must not inherit BaseException's catch-all behavior"
        );
        assert_eq!(
            exception_handler_match(Some("RuntimeError"), Some(&handler("ValueError")), &state),
            ExceptionMatch::No
        );
        assert_eq!(
            exception_handler_match(
                Some("UnicodeDecodeError"),
                Some(&handler("ValueError")),
                &state
            ),
            ExceptionMatch::Definite
        );
        assert_eq!(
            exception_handler_match(Some("CancelledError"), Some(&handler("Exception")), &state),
            ExceptionMatch::No
        );
        assert_eq!(
            exception_handler_match(
                Some("CleanupError"),
                Some(&handler("BaseException")),
                &state
            ),
            ExceptionMatch::Definite
        );
        assert_eq!(
            exception_handler_match(Some("CleanupError"), Some(&handler("CleanupError")), &state),
            ExceptionMatch::Possible
        );

        let mut shadowed = State::default();
        shadowed.shadowed_classes.insert("BaseException".into());
        assert_eq!(
            exception_handler_match(
                Some("RuntimeError"),
                Some(&handler("BaseException")),
                &shadowed
            ),
            ExceptionMatch::Possible
        );
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
            annotation: None,
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
                annotation: None,
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

    fn fixture_close_model(
        method: &str,
        repeat_close: RepeatClosePolicy,
        exceptional: Option<LifecyclePostcondition>,
        cancelled: Option<LifecyclePostcondition>,
        asynchronous: bool,
        post_close_methods: &[&str],
    ) -> ResourceEffectModel {
        ResourceEffectModel {
            id: format!("fixture-{}", method),
            qualified_symbol: "model.Connection".into(),
            close_method: method.into(),
            source_revision: "fixture-v1".into(),
            source_url: "fixture://model.Connection".into(),
            supported_python_versions: BTreeSet::from(["3.11".into()]),
            platform_assumptions: vec!["platform-independent fixture".into()],
            argument_aliases: Vec::new(),
            result_alias: None,
            duty_handoff: "none".into(),
            suppression: "not-a-context-manager".into(),
            asynchronous,
            normal: Some(LifecyclePostcondition::Closed),
            exceptional,
            cancelled,
            repeat_close,
            post_close_methods: post_close_methods
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
            trust: "reviewed-fixture".into(),
            reviewer: "fixture-test".into(),
            valid_cases: vec!["normal close applies its postcondition".into()],
            invalid_cases: vec!["requires-open close rejects a closed resource".into()],
            unknown_cases: vec!["unknown postcondition weakens the lifecycle".into()],
        }
    }

    fn modeled_resource_state(mask: u8, method: &str, model: Option<ResourceEffectModel>) -> State {
        let file = "model.py";
        let resource = Contract {
            schema_version: 1,
            base_symbol: "model.Connection".into(),
            display_name: "Connection".into(),
            lifecycle: Some(Lifecycle {
                closed_by: method.into(),
            }),
            access: "unspecified".into(),
            source: Span {
                file: file.into(),
                line: 1,
                column: 0,
                end_line: 1,
                end_column: 1,
            },
            provenance: vec![Provenance {
                origin: "fixture".into(),
                trust: "declared-unverified".into(),
            }],
        };
        let mut state = State {
            alive: true,
            target_python_version: "3.11".into(),
            ..State::default()
        };
        state
            .places
            .insert("resource".into(), BTreeSet::from(["object".into()]));
        state.objects.insert("object".into(), mask);
        state.resources.insert("object".into(), resource);
        state.duty_origins.insert(
            "object".into(),
            Span {
                file: file.into(),
                line: 2,
                column: 4,
                end_line: 2,
                end_column: 12,
            },
        );
        state.close_origins.insert(
            "object".into(),
            Span {
                file: file.into(),
                line: 3,
                column: 4,
                end_line: 3,
                end_column: 18,
            },
        );
        if let Some(model) = model {
            let mut catalog = FlowModelCatalog::default();
            catalog.insert(model).expect("fixture model is unambiguous");
            state.models = Arc::new(catalog);
        }
        state
    }

    fn method_call(receiver: &str, method: &str) -> AstExpr {
        AstExpr {
            range: SourceRange { start: 0, end: 6 },
            kind: AstExprKind::Call {
                function: Box::new(AstExpr {
                    range: SourceRange { start: 0, end: 6 },
                    kind: AstExprKind::Attribute(
                        Box::new(AstExpr {
                            range: SourceRange {
                                start: 0,
                                end: receiver.len(),
                            },
                            kind: AstExprKind::Name(receiver.into()),
                        }),
                        method.into(),
                    ),
                }),
                arguments: Vec::new(),
                keywords: Vec::new(),
            },
        }
    }

    fn run_fixture_call(expression: &AstExpr, state: &mut State, result: &mut Inspection) {
        let file = SourceFile::new("model.py", "model", "resource.close()\n");
        consume_expr(
            expression,
            state,
            &file,
            &[],
            &FunctionMap::new(),
            result,
            0,
        );
    }

    #[test]
    fn reviewed_close_models_apply_exceptional_postconditions_per_outcome() {
        for (postcondition, expected_mask, is_incomplete) in [
            (LifecyclePostcondition::Closed, CLOSED, false),
            (LifecyclePostcondition::Open, OPEN, false),
            (LifecyclePostcondition::Preserve, OPEN, false),
            (LifecyclePostcondition::Unknown, OPEN | UNKNOWN, true),
        ] {
            let model = fixture_close_model(
                "close",
                RepeatClosePolicy::Unspecified,
                Some(postcondition),
                None,
                false,
                &[],
            );
            let mut state = modeled_resource_state(OPEN, "close", Some(model));
            let mut result = inspection();

            run_fixture_call(&method_call("resource", "close"), &mut state, &mut result);

            assert_eq!(state.objects.get("object"), Some(&CLOSED));
            assert_eq!(state.pending_raises.len(), 1);
            assert_eq!(
                state.pending_raises[0].state.objects.get("object"),
                Some(&expected_mask)
            );
            assert_eq!(result.coverage.status == "incomplete", is_incomplete);
        }
    }

    #[test]
    fn reviewed_repeat_close_policies_distinguish_idempotent_and_requires_open() {
        let idempotent = fixture_close_model(
            "close",
            RepeatClosePolicy::Idempotent,
            None,
            None,
            false,
            &[],
        );
        let mut state = modeled_resource_state(CLOSED, "close", Some(idempotent));
        let mut result = inspection();
        run_fixture_call(&method_call("resource", "close"), &mut state, &mut result);
        assert_eq!(state.objects.get("object"), Some(&CLOSED));
        assert!(result.diagnostics.is_empty(), "{result:#?}");
        assert_eq!(result.coverage.status, "complete-for-supported-subset");

        let requires_open = fixture_close_model(
            "close",
            RepeatClosePolicy::RequiresOpen,
            None,
            None,
            false,
            &[],
        );
        let mut state = modeled_resource_state(CLOSED, "close", Some(requires_open));
        let mut result = inspection();
        run_fixture_call(&method_call("resource", "close"), &mut state, &mut result);
        assert_eq!(state.pending_raises.len(), 1);
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1001"
                && diagnostic.message == "resource close requires an open resource"
                && diagnostic.certainty == "definite"
        }));
        assert_eq!(result.coverage.status, "complete-for-supported-subset");
    }

    #[test]
    fn reviewed_post_close_allowlist_skips_only_the_closed_use_precondition() {
        let model = fixture_close_model(
            "close",
            RepeatClosePolicy::Idempotent,
            None,
            None,
            false,
            &["status"],
        );
        let mut state = modeled_resource_state(CLOSED, "close", Some(model));
        let mut result = inspection();

        run_fixture_call(&method_call("resource", "status"), &mut state, &mut result);

        assert!(!result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001" && diagnostic.message.contains("unknown method effect")
        }));
        assert_eq!(state.objects.get("object"), Some(&(CLOSED | UNKNOWN)));
    }

    #[test]
    fn reviewed_async_close_applies_its_cancellation_postcondition() {
        for (postcondition, expected_mask, is_incomplete) in [
            (LifecyclePostcondition::Closed, CLOSED, false),
            (LifecyclePostcondition::Open, OPEN, false),
            (LifecyclePostcondition::Preserve, OPEN, false),
            (LifecyclePostcondition::Unknown, OPEN | UNKNOWN, true),
        ] {
            let model = fixture_close_model(
                "aclose",
                RepeatClosePolicy::Unspecified,
                None,
                Some(postcondition),
                true,
                &[],
            );
            let mut state = modeled_resource_state(OPEN, "aclose", Some(model));
            let mut result = inspection();
            let call = method_call("resource", "aclose");
            let awaited = AstExpr {
                range: call.range,
                kind: AstExprKind::Await(Box::new(call)),
            };

            run_fixture_call(&awaited, &mut state, &mut result);

            assert_eq!(state.objects.get("object"), Some(&CLOSED));
            let cancelled = state
                .pending_raises
                .iter()
                .find(|exit| exit.kind == CompletionKind::Cancelled(None))
                .expect("the model declares a cancellation outcome");
            assert_eq!(cancelled.state.objects.get("object"), Some(&expected_mask));
            assert_eq!(result.coverage.status == "incomplete", is_incomplete);
            assert!(!result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP9001"
                    && diagnostic
                        .message
                        .contains("cancellation may interrupt the awaited operation")
            }));
        }
    }
}

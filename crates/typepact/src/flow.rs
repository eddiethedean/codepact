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

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const MAX_CALL_DEPTH: usize = 8;
const MAX_CONTROL_FLOW_DEPTH: usize = 64;
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
    return_access: Option<String>,
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
    loop_depth: usize,
    function_bindings: BTreeMap<String, String>,
    function_bindings_complete: bool,
    class_bindings: BTreeMap<String, Option<Contract>>,
    module_function_bindings: BTreeMap<String, String>,
    module_class_bindings: BTreeMap<String, Option<Contract>>,
    imported_classes: BTreeMap<String, Option<Contract>>,
    class_bindings_complete: bool,
    module_scope: bool,
    scope_module: String,
    target_python_version: String,
    coroutines: BTreeMap<String, CoroutineRecord>,
    coroutine_aliases: BTreeMap<String, String>,
    unknown_coroutine_aliases: BTreeSet<String>,
    pending_coroutine_result: Option<String>,
    close_origins: BTreeMap<String, Span>,
    duty_origins: BTreeMap<String, Span>,
    awaiting_coroutine: bool,
    pending_return_targets: Option<BTreeSet<String>>,
    pending_raises: Vec<FlowExit>,
    alive: bool,
}

#[derive(Clone)]
struct CoroutineRecord {
    invocation: AstExpr,
    captures: BTreeMap<String, CapturedCoroutinePlace>,
    range: SourceRange,
    consumed: bool,
    ambiguous: bool,
}

#[derive(Clone, Default)]
struct CapturedCoroutinePlace {
    targets: BTreeSet<String>,
    unknown: bool,
}

#[derive(Clone)]
enum CompletionKind {
    Fallthrough,
    Return(Option<AstExpr>),
    Raise(Option<AstExpr>),
    Cancelled,
    Break,
    Continue,
}

#[derive(Clone)]
struct FlowExit {
    state: State,
    range: SourceRange,
    kind: CompletionKind,
}

#[derive(Clone)]
struct ContextCleanup {
    receiver: String,
    method: &'static str,
    asynchronous: bool,
    range: SourceRange,
}

#[derive(Default)]
struct FlowResult {
    returns: Vec<FlowExit>,
    raises: Vec<FlowExit>,
    breaks: Vec<FlowExit>,
    continues: Vec<FlowExit>,
}

impl FlowResult {
    fn append(&mut self, mut other: Self) {
        self.returns.append(&mut other.returns);
        self.raises.append(&mut other.raises);
        self.breaks.append(&mut other.breaks);
        self.continues.append(&mut other.continues);
    }

    fn all_abrupt(&self) -> impl Iterator<Item = &FlowExit> {
        self.returns
            .iter()
            .chain(&self.raises)
            .chain(&self.breaks)
            .chain(&self.continues)
    }
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
            queue_raise(
                env,
                range,
                Some(AstExpr {
                    range,
                    kind: AstExprKind::Name("TypeError".into()),
                }),
            );
            "invalid call argument binding ends normal lifecycle flow"
        }
        BindingFailure::Unknown => {
            let affected: Vec<_> = env.places.values().flatten().cloned().collect();
            weaken(env, affected.into_iter());
            env.escaped.extend(env.places.values().flatten().cloned());
            queue_raise(env, range, None);
            "call argument binding is not modeled"
        }
    };
    boundary(result, file, range, message);
}

fn expression_may_return_resource(expression: &AstExpr) -> bool {
    match &expression.kind {
        AstExprKind::NoneLiteral
        | AstExprKind::String(_)
        | AstExprKind::Boolean(_)
        | AstExprKind::Primitive => false,
        AstExprKind::Tuple(items) => items.iter().any(expression_may_return_resource),
        AstExprKind::Name(_)
        | AstExprKind::Attribute(_, _)
        | AstExprKind::Subscript(_, _)
        | AstExprKind::Call { .. }
        | AstExprKind::Await(_)
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
    let returned_targets: BTreeSet<_> = left
        .pending_return_targets
        .iter()
        .chain(right.pending_return_targets.iter())
        .flatten()
        .cloned()
        .collect();
    joined.pending_return_targets = if returned_targets.is_empty() {
        None
    } else {
        Some(returned_targets)
    };
    joined.pending_coroutine_result =
        if left.pending_coroutine_result == right.pending_coroutine_result {
            left.pending_coroutine_result.clone()
        } else {
            None
        };
    joined
        .unknown_coroutine_aliases
        .extend(right.unknown_coroutine_aliases.iter().cloned());
    let coroutine_aliases: BTreeSet<_> = left
        .coroutine_aliases
        .keys()
        .chain(right.coroutine_aliases.keys())
        .cloned()
        .collect();
    for alias in coroutine_aliases {
        if left.coroutine_aliases.get(&alias) == right.coroutine_aliases.get(&alias) {
            if left.coroutine_aliases.contains_key(&alias) {
                joined
                    .coroutine_aliases
                    .insert(alias.clone(), left.coroutine_aliases[&alias].clone());
            }
        } else {
            joined.coroutine_aliases.remove(&alias);
            joined.unknown_coroutine_aliases.insert(alias);
        }
    }
    let coroutine_ids: BTreeSet<_> = left
        .coroutines
        .keys()
        .chain(right.coroutines.keys())
        .cloned()
        .collect();
    for id in coroutine_ids {
        match (left.coroutines.get(&id), right.coroutines.get(&id)) {
            (Some(left_record), Some(right_record)) => {
                let record = joined
                    .coroutines
                    .entry(id)
                    .or_insert_with(|| left_record.clone());
                if left_record.consumed != right_record.consumed
                    || left_record.invocation != right_record.invocation
                {
                    record.ambiguous = true;
                }
                record.ambiguous |= right_record.ambiguous;
                record.consumed &= right_record.consumed;
                for (capture, right_place) in &right_record.captures {
                    let place = record.captures.entry(capture.clone()).or_default();
                    place.targets.extend(right_place.targets.iter().cloned());
                    place.unknown |= right_place.unknown;
                }
            }
            (Some(left_record), None) => {
                let mut record = left_record.clone();
                record.ambiguous = true;
                joined.coroutines.insert(id, record);
            }
            (None, Some(right_record)) => {
                let mut record = right_record.clone();
                record.ambiguous = true;
                joined.coroutines.insert(id, record);
            }
            (None, None) => unreachable!(),
        }
    }
    joined.pending_raises = merge_pending_raises(&left.pending_raises, &right.pending_raises);
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
        let path_optional = !left.objects.contains_key(object) && state & OPEN != 0;
        *joined.objects.entry(object.clone()).or_default() |=
            *state | if path_optional { UNKNOWN } else { 0 };
    }
    for (object, state) in &left.objects {
        if !right.objects.contains_key(object) && state & OPEN != 0 {
            *joined.objects.entry(object.clone()).or_default() |= UNKNOWN;
        }
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
    for (object, origin) in &right.duty_origins {
        joined
            .duty_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.alive |= right.alive;
    joined.loop_depth = left.loop_depth.max(right.loop_depth);
    (joined, effects)
}

fn completion_tag(kind: &CompletionKind) -> u8 {
    match kind {
        CompletionKind::Fallthrough => 0,
        CompletionKind::Return(_) => 1,
        CompletionKind::Raise(_) => 2,
        CompletionKind::Cancelled => 3,
        CompletionKind::Break => 4,
        CompletionKind::Continue => 5,
    }
}

fn merge_pending_raises(left: &[FlowExit], right: &[FlowExit]) -> Vec<FlowExit> {
    let mut merged = left.to_vec();
    for exit in right {
        let key = (exit.range.start, exit.range.end, completion_tag(&exit.kind));
        if let Some(previous) = merged.iter_mut().find(|previous| {
            (
                previous.range.start,
                previous.range.end,
                completion_tag(&previous.kind),
            ) == key
        }) {
            previous.state = merge(&previous.state, &exit.state).0;
            match (&previous.kind, &exit.kind) {
                (CompletionKind::Raise(Some(left)), CompletionKind::Raise(Some(right)))
                    if left != right =>
                {
                    previous.kind = CompletionKind::Raise(None);
                }
                _ => {}
            }
        } else {
            merged.push(exit.clone());
        }
    }
    merged
}

fn take_pending_raises(state: &mut State) -> Vec<FlowExit> {
    std::mem::take(&mut state.pending_raises)
}

fn queue_raise(state: &mut State, range: SourceRange, exception: Option<AstExpr>) {
    let mut raised_state = state.clone();
    raised_state.alive = true;
    raised_state.pending_raises.clear();
    state.pending_raises.push(FlowExit {
        state: raised_state,
        range,
        kind: CompletionKind::Raise(exception),
    });
}

fn project_call_exit(parent: &State, child: &FlowExit, call_range: SourceRange) -> FlowExit {
    let mut state = parent.clone();
    state.alive = true;
    state.pending_raises.clear();
    state.pending_return_targets = None;
    for object in parent.objects.keys() {
        if let Some(mask) = child.state.objects.get(object) {
            state.objects.insert(object.clone(), *mask);
        }
        if let Some(resource) = child.state.resources.get(object) {
            state.resources.insert(object.clone(), resource.clone());
        }
        if let Some(origin) = child.state.close_origins.get(object) {
            state.close_origins.insert(object.clone(), origin.clone());
        }
        if child.state.escaped.contains(object) {
            state.escaped.insert(object.clone());
        }
    }
    FlowExit {
        state,
        range: call_range,
        kind: child.kind.clone(),
    }
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
    if let Some(existing) = result
        .diagnostics
        .iter_mut()
        .find(|diagnostic| diagnostic.code == code && diagnostic.primary == primary)
    {
        // The same source operation can be reached along several exits. Keep a
        // definite finding only when every analyzed path makes it definite.
        if existing.certainty != certainty {
            existing.certainty = "possible".into();
            existing.severity = "warning".into();
        }
        for related_span in related {
            if !existing.related.contains(&related_span) {
                existing.related.push(related_span);
            }
        }
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

fn report_cleanup_exit(
    result: &mut Inspection,
    file: &SourceFile,
    exit: &FlowExit,
    return_access: Option<&str>,
) {
    report_unawaited_coroutines(&exit.state, file, result);
    let exit_span = span(file, exit.range);
    let mut returned_owned = BTreeSet::new();
    let mut escaped_return = BTreeSet::new();
    if let CompletionKind::Return(value) = &exit.kind {
        if let Some(name) = value.as_ref().and_then(expr_name) {
            if !exit.state.unknown_places.contains(name) {
                if return_access == Some("owned") {
                    returned_owned.extend(
                        exit.state
                            .places
                            .get(name)
                            .into_iter()
                            .flatten()
                            .filter(|object| {
                                exit.state.objects.get(*object).copied().unwrap_or(UNKNOWN) == OPEN
                                    && !exit.state.borrowed.contains(*object)
                            })
                            .cloned(),
                    );
                } else {
                    escaped_return
                        .extend(exit.state.places.get(name).into_iter().flatten().cloned());
                }
            }
        }
        if let Some(targets) = &exit.state.pending_return_targets {
            if return_access == Some("owned") {
                returned_owned.extend(
                    targets
                        .iter()
                        .filter(|object| {
                            exit.state.objects.get(*object).copied().unwrap_or(UNKNOWN) == OPEN
                                && !exit.state.borrowed.contains(*object)
                        })
                        .cloned(),
                );
            } else {
                escaped_return.extend(targets.iter().cloned());
            }
        }
    }
    if !escaped_return.is_empty() {
        boundary(
            result,
            file,
            exit.range,
            "resource returned without an Owned contract; duty transfer is unknown",
        );
    }
    if matches!(exit.kind, CompletionKind::Return(_)) && return_access == Some("owned") {
        let returned_targets = exit
            .state
            .pending_return_targets
            .iter()
            .flatten()
            .cloned()
            .chain(match &exit.kind {
                CompletionKind::Return(Some(value)) => expr_name(value)
                    .filter(|name| !exit.state.unknown_places.contains(*name))
                    .into_iter()
                    .flat_map(|name| exit.state.places.get(name).into_iter().flatten().cloned())
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<BTreeSet<_>>();
        for object in &returned_targets {
            let mask = exit.state.objects.get(object).copied().unwrap_or(UNKNOWN);
            if exit.state.borrowed.contains(object) {
                report(
                    result,
                    "TP1003",
                    "a Borrowed resource cannot be returned as Owned",
                    exit_span.clone(),
                    exit.state
                        .borrowed_origins
                        .get(object)
                        .cloned()
                        .into_iter()
                        .collect(),
                    "definite",
                );
                continue;
            }
            if mask & CLOSED != 0 {
                report(
                    result,
                    "TP1003",
                    "an Owned return must return an open resource",
                    exit_span.clone(),
                    exit.state
                        .close_origins
                        .get(object)
                        .cloned()
                        .into_iter()
                        .collect(),
                    if mask & (OPEN | UNKNOWN) == 0 {
                        "definite"
                    } else {
                        "possible"
                    },
                );
            }
            if mask & UNKNOWN != 0 {
                boundary(
                    result,
                    file,
                    exit.range,
                    "Owned return state is unknown; handoff is not established",
                );
            }
        }
    }
    let exit_kind = match &exit.kind {
        CompletionKind::Fallthrough => "fallthrough",
        CompletionKind::Return(_) => "return",
        CompletionKind::Raise(_) => "raise",
        CompletionKind::Cancelled => "cancelled",
        CompletionKind::Break => "break",
        CompletionKind::Continue => "continue",
    };
    for (object, origin) in &exit.state.duty_origins {
        if returned_owned.contains(object) || escaped_return.contains(object) {
            continue;
        }
        let mask = exit.state.objects.get(object).copied().unwrap_or(UNKNOWN);
        if mask & OPEN == 0 {
            if mask & UNKNOWN != 0 {
                boundary(
                    result,
                    file,
                    exit.range,
                    "cleanup duty may escape with unknown resource state",
                );
            }
            continue;
        }
        let certainty = if mask & (CLOSED | UNKNOWN) == 0 {
            "definite"
        } else {
            "possible"
        };
        let mut related = vec![origin.clone()];
        if let Some(close) = exit.state.close_origins.get(object) {
            related.push(close.clone());
        }
        if let Some(existing) = result
            .diagnostics
            .iter_mut()
            .find(|diagnostic| diagnostic.code == "TP1002" && diagnostic.primary == exit_span)
        {
            for span in related {
                if !existing.related.contains(&span) {
                    existing.related.push(span);
                }
            }
            if certainty == "definite" {
                existing.certainty = "definite".into();
                existing.severity = "error".into();
            }
        } else {
            report(
                result,
                "TP1002",
                &format!("owned resource cleanup obligation remains open on {exit_kind}"),
                exit_span.clone(),
                related,
                certainty,
            );
        }
    }
}

fn grouped_exits(exits: &[FlowExit]) -> Vec<FlowExit> {
    let mut groups = BTreeMap::<(usize, usize, u8), FlowExit>::new();
    for exit in exits {
        let kind = match exit.kind {
            CompletionKind::Fallthrough => 0,
            CompletionKind::Return(_) => 1,
            CompletionKind::Raise(_) => 2,
            CompletionKind::Cancelled => 3,
            CompletionKind::Break => 4,
            CompletionKind::Continue => 5,
        };
        let key = (exit.range.start, exit.range.end, kind);
        if let Some(existing) = groups.get_mut(&key) {
            existing.state = merge(&existing.state, &exit.state).0;
            existing.state.alive = true;
            if let (CompletionKind::Raise(Some(left)), CompletionKind::Raise(Some(right))) =
                (&existing.kind, &exit.kind)
            {
                if left != right {
                    existing.kind = CompletionKind::Raise(None);
                }
            }
        } else {
            groups.insert(key, exit.clone());
        }
    }
    groups.into_values().collect()
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

fn closed_mask(previous: u8) -> u8 {
    if previous & UNKNOWN != 0 {
        previous | CLOSED
    } else {
        CLOSED
    }
}

type FunctionMap = BTreeMap<String, FunctionBody>;

fn assigned_names(statements: &[FlowStatement]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_assigned_names(statements, &mut names);
    names
}

fn synthetic_name(state: &State, role: &str, range: SourceRange, index: usize) -> String {
    let base = format!("__typepact_{role}_{}_{}", range.start, index);
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
        loop_depth: parent.loop_depth,
        awaiting_coroutine: parent.awaiting_coroutine,
        target_python_version: parent.target_python_version.clone(),
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

fn apply_call_effects(parent: &mut State, child: &State, extra_objects: &BTreeSet<String>) {
    let mut transferred = parent.objects.keys().cloned().collect::<BTreeSet<_>>();
    transferred.extend(extra_objects.iter().cloned());
    for object in transferred {
        if let Some(mask) = child.objects.get(&object) {
            parent.objects.insert(object.clone(), *mask);
        }
        if let Some(resource) = child.resources.get(&object) {
            parent.resources.insert(object.clone(), resource.clone());
        }
        if let Some(origin) = child.close_origins.get(&object) {
            parent.close_origins.insert(object.clone(), origin.clone());
        }
        if !parent.duty_origins.contains_key(&object) && extra_objects.contains(&object) {
            if let Some(origin) = child.duty_origins.get(&object) {
                parent.duty_origins.insert(object.clone(), origin.clone());
            }
        }
        if child.escaped.contains(&object) {
            parent.escaped.insert(object);
        }
    }
    parent.pending_return_targets = child.pending_return_targets.clone();
}

fn resolved_await_call(
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

fn final_function_bindings(
    module: &str,
    functions: &FunctionMap,
    declarations: &[Declaration],
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
                ..
            } => {
                for import in names {
                    let local = import.alias.as_deref().unwrap_or(&import.name);
                    if source == "contextlib"
                        && matches!(import.name.as_str(), "closing" | "aclosing")
                    {
                        bindings.insert(
                            local.to_owned(),
                            format!("external:contextlib.{}", import.name),
                        );
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
                    if import.name == "contextlib" {
                        bindings.insert(local.to_owned(), "external:contextlib".into());
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

fn external_module_bindings(
    declarations: &[Declaration],
    exports: &BTreeMap<String, BTreeMap<String, String>>,
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
                    let external = if module == "contextlib"
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
                    if import.name == "contextlib" {
                        bindings.insert(local.to_owned(), "external:contextlib".into());
                    } else {
                        bindings.remove(local);
                    }
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
    let Some(targets) = env.places.get(name) else {
        let unknown = env.unknown_places.contains(name);
        let synthetic = format!("__typepact_coro_{}_{}", id, *next_capture);
        *next_capture = next_capture.saturating_add(1);
        captures.insert(
            synthetic.clone(),
            CapturedCoroutinePlace {
                targets: BTreeSet::new(),
                unknown,
            },
        );
        expression.kind = AstExprKind::Name(synthetic);
        return unknown;
    };
    let unknown = env.unknown_places.contains(name);
    let synthetic = format!("__typepact_coro_{}_{}", id, *next_capture);
    *next_capture = next_capture.saturating_add(1);
    captures.insert(
        synthetic.clone(),
        CapturedCoroutinePlace {
            targets: targets.clone(),
            unknown,
        },
    );
    expression.kind = AstExprKind::Name(synthetic);
    unknown
}

fn register_pending_coroutine(
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
                let synthetic = format!("__typepact_coro_func_{id}");
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
        captures,
        range: expression.range,
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
        }
        boundary(
            result,
            file,
            expression.range,
            "repeated coroutine creation at one source site has unknown instance multiplicity",
        );
    } else {
        env.coroutines.insert(id.clone(), record);
    }
    env.pending_coroutine_result = Some(id);
}

fn report_unawaited_coroutines(state: &State, file: &SourceFile, result: &mut Inspection) {
    for coroutine in state
        .coroutines
        .values()
        .filter(|coroutine| !coroutine.consumed)
    {
        boundary(
            result,
            file,
            coroutine.range,
            "created coroutine is not awaited before this scope exit; captured resource effects remain unresolved",
        );
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
    if let AstExprKind::Await(awaited) = &expr.kind {
        let coroutine_id = expr_name(awaited)
            .and_then(|name| env.coroutine_aliases.get(name))
            .cloned();
        let coroutine = coroutine_id
            .as_ref()
            .and_then(|id| env.coroutines.get(id))
            .cloned();
        let capture_names = coroutine
            .as_ref()
            .map(|record| record.captures.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let resolved = coroutine.is_some() || resolved_await_call(awaited, file, env, functions);
        if let (Some(id), Some(record)) = (&coroutine_id, &coroutine) {
            if record.consumed {
                boundary(
                    result,
                    file,
                    expr.range,
                    "coroutine is awaited more than once; repeat-await behavior is unsupported",
                );
                for object in record.captures.values().flat_map(|place| &place.targets) {
                    *env.objects.entry(object.clone()).or_default() |= OPEN | CLOSED | UNKNOWN;
                }
                queue_raise(env, expr.range, None);
                return;
            }
            for (capture, place) in &record.captures {
                if !place.targets.is_empty() || place.unknown {
                    env.places.insert(capture.clone(), place.targets.clone());
                }
                if place.unknown {
                    env.unknown_places.insert(capture.clone());
                }
            }
            if record.ambiguous {
                boundary(
                    result,
                    file,
                    expr.range,
                    "coroutine execution or captures differ across paths; effects are widened",
                );
            }
            let _ = id;
        }
        let before = env.clone();
        env.awaiting_coroutine = true;
        if let Some(record) = &coroutine {
            consume_expr(
                &record.invocation,
                env,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
        } else {
            consume_expr(awaited, env, file, contracts, functions, result, depth + 1);
        }
        env.awaiting_coroutine = before.awaiting_coroutine;
        if let Some(id) = &coroutine_id {
            if let Some(record) = env.coroutines.get_mut(id) {
                record.consumed = true;
            }
            if coroutine.as_ref().is_some_and(|record| record.ambiguous) {
                for object in coroutine
                    .iter()
                    .flat_map(|record| record.captures.values())
                    .flat_map(|place| &place.targets)
                {
                    *env.objects.entry(object.clone()).or_default() |= OPEN | CLOSED | UNKNOWN;
                }
            }
        }
        if before.alive && (resolved || env.alive) {
            let mut cancelled = before;
            cancelled.awaiting_coroutine = env.awaiting_coroutine;
            cancelled.alive = true;
            cancelled.pending_raises.clear();
            if let Some(id) = &coroutine_id {
                if let Some(record) = cancelled.coroutines.get_mut(id) {
                    record.consumed = true;
                }
            }
            let affected = cancelled
                .places
                .values()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            weaken(&mut cancelled, affected.into_iter());
            for capture in &capture_names {
                cancelled.places.remove(capture);
                cancelled.unknown_places.remove(capture);
            }
            env.pending_raises.push(FlowExit {
                state: cancelled,
                range: expr.range,
                kind: CompletionKind::Cancelled,
            });
            boundary(
                result,
                file,
                expr.range,
                "cancellation may interrupt the awaited operation or its cleanup",
            );
        }
        for capture in &capture_names {
            env.places.remove(capture);
            env.unknown_places.remove(capture);
            for exit in &mut env.pending_raises {
                exit.state.places.remove(capture);
                exit.state.unknown_places.remove(capture);
            }
        }
        if !resolved && env.alive {
            let affected = env.places.values().flatten().cloned().collect::<Vec<_>>();
            weaken(env, affected.into_iter());
            boundary(
                result,
                file,
                expr.range,
                "await target is not a resolved direct coroutine or reviewed async method",
            );
            queue_raise(env, expr.range, None);
        }
        return;
    }
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
                let mut branch_raises = Vec::new();
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
                        if definition.is_some_and(|definition| definition.is_async)
                            && !env.awaiting_coroutine
                        {
                            if uncertain_receiver {
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
                                    "async method coroutine has an uncertain receiver alias",
                                );
                            } else {
                                register_pending_coroutine(
                                    expr,
                                    env,
                                    file,
                                    result,
                                    definition
                                        .and_then(|definition| definition.binding_key.clone()),
                                );
                            }
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
                            let raised = child
                                .pending_raises
                                .iter()
                                .map(|exit| project_call_exit(env, exit, expr.range))
                                .collect::<Vec<_>>();
                            env.pending_raises.extend(raised);
                            apply_call_effects(env, &child, &BTreeSet::new());
                            if is_close && child.alive {
                                env.close_origins
                                    .insert(object.clone(), span(file, expr.range));
                                env.objects.insert(object.clone(), closed_mask(mask));
                            }
                            if is_close {
                                let mut exceptional = env.clone();
                                exceptional.alive = true;
                                exceptional.pending_raises.clear();
                                exceptional.objects.insert(
                                    object.clone(),
                                    mask | env.objects.get(object).copied().unwrap_or(UNKNOWN)
                                        | UNKNOWN,
                                );
                                env.pending_raises.push(FlowExit {
                                    state: exceptional,
                                    range: expr.range,
                                    kind: CompletionKind::Raise(None),
                                });
                                boundary(
                                    result,
                                    file,
                                    expr.range,
                                    "close exceptional postcondition is not modeled",
                                );
                            }
                            if !child.alive {
                                env.alive = false;
                            }
                        } else if is_close {
                            env.close_origins
                                .insert(object.clone(), span(file, expr.range));
                            env.objects.insert(object.clone(), closed_mask(mask));
                            let mut exceptional = env.clone();
                            exceptional.alive = true;
                            exceptional.pending_raises.clear();
                            exceptional.objects.insert(object.clone(), mask | UNKNOWN);
                            env.pending_raises.push(FlowExit {
                                state: exceptional,
                                range: expr.range,
                                kind: CompletionKind::Raise(None),
                            });
                            boundary(
                                result,
                                file,
                                expr.range,
                                "close exceptional postcondition is not modeled",
                            );
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
                            queue_raise(env, expr.range, None);
                        }
                    })();
                    branch_raises.extend(alternative.pending_raises.iter().cloned());
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
                env.pending_raises = merge_pending_raises(&env.pending_raises, &branch_raises);
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
                queue_raise(env, expr.range, None);
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
            queue_raise(env, expr.range, None);
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
                if definition.is_async && !env.awaiting_coroutine {
                    register_pending_coroutine(
                        expr,
                        env,
                        file,
                        result,
                        definition.binding_key.clone(),
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
                    loop_depth: env.loop_depth,
                    awaiting_coroutine: env.awaiting_coroutine,
                    target_python_version: env.target_python_version.clone(),
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
                            if let Some(contract) = definition.parameter_contracts.get(&parameter) {
                                for object in targets {
                                    if contract.access == "owned" {
                                        child
                                            .duty_origins
                                            .entry(object.clone())
                                            .or_insert_with(|| contract.source.clone());
                                    } else if contract.access == "shared" {
                                        child.borrowed.insert(object.clone());
                                        child
                                            .borrowed_origins
                                            .entry(object.clone())
                                            .or_insert_with(|| contract.source.clone());
                                    }
                                }
                            }
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
                let mut owned_return_targets = BTreeSet::new();
                if definition.return_access.as_deref() == Some("owned") {
                    for exit in &returned.returns {
                        let CompletionKind::Return(value) = &exit.kind else {
                            continue;
                        };
                        if let Some(name) = value.as_ref().and_then(expr_name) {
                            if !exit.state.unknown_places.contains(name) {
                                owned_return_targets.extend(
                                    exit.state.places.get(name).into_iter().flatten().cloned(),
                                );
                            }
                        }
                        owned_return_targets
                            .extend(exit.state.pending_return_targets.iter().flatten().cloned());
                    }
                }
                let propagated_raises = returned
                    .raises
                    .iter()
                    .map(|exit| project_call_exit(env, exit, expr.range))
                    .collect::<Vec<_>>();
                env.pending_raises.extend(propagated_raises);
                let child = join_normal_exits(
                    child,
                    returned
                        .returns
                        .into_iter()
                        .map(|exit| exit.state)
                        .collect(),
                );
                let mut child = child;
                if !owned_return_targets.is_empty() {
                    child.pending_return_targets = Some(owned_return_targets.clone());
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
                apply_call_effects(env, &child, &owned_return_targets);
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
                queue_raise(env, expr.range, None);
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
            queue_raise(env, expr.range, None);
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
) -> FlowResult {
    let mut outcomes = FlowResult::default();
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
            outcomes.raises.extend(take_pending_raises(env));
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
            let left_outcomes = execute(
                body,
                &mut left,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            let right_outcomes = execute(
                alternate,
                &mut right,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            outcomes.append(left_outcomes);
            outcomes.append(right_outcomes);
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
        } else {
            match statement {
                FlowStatement::Try {
                    body,
                    handlers,
                    alternate,
                    finalbody,
                    range,
                } => {
                    let try_outcomes = execute_try(
                        body,
                        handlers,
                        alternate,
                        finalbody,
                        *range,
                        env,
                        file,
                        contracts,
                        functions,
                        result,
                        depth + 1,
                    );
                    outcomes.append(try_outcomes);
                }
                FlowStatement::Loop {
                    test,
                    iterator,
                    target,
                    body,
                    alternate,
                    range,
                } => {
                    let loop_outcomes = execute_loop(
                        test.as_ref(),
                        iterator.as_ref(),
                        target.as_deref(),
                        body,
                        alternate,
                        *range,
                        env,
                        file,
                        contracts,
                        functions,
                        result,
                        depth + 1,
                    );
                    outcomes.append(loop_outcomes);
                }
                FlowStatement::With {
                    items,
                    body,
                    asynchronous,
                    range,
                } => {
                    let with_outcomes = execute_with(
                        items,
                        body,
                        *asynchronous,
                        *range,
                        env,
                        file,
                        contracts,
                        functions,
                        result,
                        depth + 1,
                    );
                    outcomes.append(with_outcomes);
                }
                _ => {
                    let returned = execute_non_if_statement(
                        statement, env, file, contracts, functions, result,
                    );
                    outcomes.raises.extend(take_pending_raises(env));
                    if let Some(returned) = returned {
                        outcomes.returns.push(FlowExit {
                            state: returned,
                            range: statement_range(statement),
                            kind: match statement {
                                FlowStatement::Return { value, .. } => {
                                    CompletionKind::Return(value.clone())
                                }
                                _ => unreachable!("only return produces a returned state"),
                            },
                        });
                    } else if !env.alive {
                        let exit = FlowExit {
                            state: {
                                let mut state = env.clone();
                                state.alive = true;
                                state
                            },
                            range: statement_range(statement),
                            kind: match statement {
                                FlowStatement::Raise { exception, .. } => {
                                    CompletionKind::Raise(exception.clone())
                                }
                                FlowStatement::Break { .. } => CompletionKind::Break,
                                FlowStatement::Continue { .. } => CompletionKind::Continue,
                                _ => continue,
                            },
                        };
                        match exit.kind {
                            CompletionKind::Raise(_) => outcomes.raises.push(exit),
                            CompletionKind::Cancelled => outcomes.raises.push(exit),
                            CompletionKind::Break => outcomes.breaks.push(exit),
                            CompletionKind::Continue => outcomes.continues.push(exit),
                            CompletionKind::Fallthrough | CompletionKind::Return(_) => {
                                unreachable!()
                            }
                        }
                    }
                }
            }
        }
    }
    outcomes
}

const MAX_LOOP_ITERATIONS: usize = 16;

fn push_exit(outcomes: &mut FlowResult, exit: FlowExit) {
    match &exit.kind {
        CompletionKind::Fallthrough => unreachable!("fallthrough is reported at function exit"),
        CompletionKind::Return(_) => outcomes.returns.push(exit),
        CompletionKind::Raise(_) => outcomes.raises.push(exit),
        CompletionKind::Cancelled => outcomes.raises.push(exit),
        CompletionKind::Break => outcomes.breaks.push(exit),
        CompletionKind::Continue => outcomes.continues.push(exit),
    }
}

fn join_states(mut states: Vec<State>) -> Option<State> {
    let mut joined = states.pop()?;
    for state in states {
        joined = merge(&joined, &state).0;
    }
    Some(joined)
}

// These interpreter routines keep the active source, semantic inputs, budgeted
// state, and mutable result explicit at each control-flow boundary.
#[allow(clippy::too_many_arguments)]
fn execute_finally_for_exit(
    exit: FlowExit,
    finalbody: &[FlowStatement],
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
    outcomes: &mut FlowResult,
) {
    let mut state = exit.state;
    state.alive = true;
    let finally_outcomes = execute(
        finalbody,
        &mut state,
        file,
        contracts,
        functions,
        result,
        depth + 1,
    );
    if state.alive {
        push_exit(outcomes, FlowExit { state, ..exit });
    }
    outcomes.append(finally_outcomes);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExceptionMatch {
    No,
    Possible,
    Definite,
}

fn expression_exception_names(expression: &AstExpr) -> Option<Vec<&str>> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(vec![name]),
        AstExprKind::Attribute(_, name) => Some(vec![name]),
        AstExprKind::Tuple(items) => items
            .iter()
            .map(expression_exception_names)
            .collect::<Option<Vec<_>>>()
            .map(|items| items.into_iter().flatten().collect()),
        _ => None,
    }
}

fn raised_exception_name(exit: &FlowExit) -> Option<&str> {
    if matches!(exit.kind, CompletionKind::Cancelled) {
        return Some("CancelledError");
    }
    let CompletionKind::Raise(Some(expression)) = &exit.kind else {
        return None;
    };
    match &expression.kind {
        AstExprKind::Call { function, .. } => expr_name(function),
        _ => expr_name(expression),
    }
}

fn known_builtin_exception(name: &str) -> bool {
    matches!(
        name,
        "ArithmeticError"
            | "AssertionError"
            | "AttributeError"
            | "BaseException"
            | "BlockingIOError"
            | "BrokenPipeError"
            | "BufferError"
            | "BytesWarning"
            | "ChildProcessError"
            | "ConnectionError"
            | "DeprecationWarning"
            | "EOFError"
            | "EncodingWarning"
            | "Exception"
            | "FileExistsError"
            | "FileNotFoundError"
            | "FloatingPointError"
            | "FutureWarning"
            | "GeneratorExit"
            | "ImportError"
            | "ImportWarning"
            | "IndentationError"
            | "IndexError"
            | "InterruptedError"
            | "IsADirectoryError"
            | "KeyError"
            | "KeyboardInterrupt"
            | "LookupError"
            | "MemoryError"
            | "ModuleNotFoundError"
            | "NameError"
            | "NotADirectoryError"
            | "NotImplementedError"
            | "OSError"
            | "OverflowError"
            | "PendingDeprecationWarning"
            | "PermissionError"
            | "ProcessLookupError"
            | "RecursionError"
            | "ReferenceError"
            | "ResourceWarning"
            | "RuntimeError"
            | "RuntimeWarning"
            | "StopAsyncIteration"
            | "StopIteration"
            | "SyntaxError"
            | "SyntaxWarning"
            | "SystemError"
            | "SystemExit"
            | "TabError"
            | "TimeoutError"
            | "TypeError"
            | "UnboundLocalError"
            | "UnicodeDecodeError"
            | "UnicodeEncodeError"
            | "UnicodeError"
            | "UnicodeTranslateError"
            | "UnicodeWarning"
            | "UserWarning"
            | "ValueError"
            | "Warning"
            | "ZeroDivisionError"
    )
}

fn exception_handler_match(exception: Option<&str>, handler: Option<&AstExpr>) -> ExceptionMatch {
    let Some(handler) = handler else {
        return ExceptionMatch::Definite;
    };
    let Some(caught) = expression_exception_names(handler) else {
        return ExceptionMatch::Possible;
    };
    let mut possible = false;
    for caught in caught {
        if caught == "BaseException" {
            return ExceptionMatch::Definite;
        }
        if exception == Some(caught) {
            if known_builtin_exception(caught) {
                return ExceptionMatch::Definite;
            }
            possible = true;
            continue;
        }
        if caught == "Exception" {
            match exception {
                Some("CancelledError" | "KeyboardInterrupt" | "SystemExit" | "GeneratorExit") => {}
                Some(name) if known_builtin_exception(name) => return ExceptionMatch::Definite,
                Some(_) | None => possible = true,
            }
        } else {
            possible = true;
        }
    }
    if possible {
        ExceptionMatch::Possible
    } else {
        ExceptionMatch::No
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_try(
    body: &[FlowStatement],
    handlers: &[FlowHandler],
    alternate: &[FlowStatement],
    finalbody: &[FlowStatement],
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> FlowResult {
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
        boundary(result, file, range, "try nesting budget exceeded");
        return FlowResult::default();
    }
    let mut pending = FlowResult::default();
    let mut normal_states = Vec::new();
    let mut body_state = env.clone();
    body_state.control_depth = body_state.control_depth.saturating_add(1);
    let body_outcomes = execute(
        body,
        &mut body_state,
        file,
        contracts,
        functions,
        result,
        depth + 1,
    );
    pending.returns.extend(body_outcomes.returns);
    pending.breaks.extend(body_outcomes.breaks);
    pending.continues.extend(body_outcomes.continues);
    if body_state.alive {
        let mut else_state = body_state;
        let else_outcomes = execute(
            alternate,
            &mut else_state,
            file,
            contracts,
            functions,
            result,
            depth + 1,
        );
        pending.append(else_outcomes);
        if else_state.alive {
            normal_states.push(else_state);
        }
    }

    for raised in body_outcomes.raises {
        if handlers.is_empty() {
            pending.raises.push(raised);
            continue;
        }
        let exception_name = raised_exception_name(&raised);
        let mut may_remain_unhandled = true;
        let mut uncertain_match = false;
        for handler in handlers {
            if !may_remain_unhandled {
                break;
            }
            let matching = exception_handler_match(exception_name, handler.exception.as_ref());
            if matching == ExceptionMatch::No {
                continue;
            }
            if matching == ExceptionMatch::Possible {
                uncertain_match = true;
                boundary(
                    result,
                    file,
                    handler.range,
                    "exception type may or may not match this handler",
                );
            }
            let mut handler_state = raised.state.clone();
            handler_state.alive = true;
            handler_state.control_depth = handler_state.control_depth.saturating_add(1);
            if let Some(name) = &handler.name {
                handler_state.places.remove(name);
                handler_state.unknown_places.insert(name.clone());
                handler_state.shadowed_functions.insert(name.clone());
                handler_state.shadowed_classes.insert(name.clone());
            }
            let handler_outcomes = execute(
                &handler.body,
                &mut handler_state,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            pending.append(handler_outcomes);
            if handler_state.alive {
                normal_states.push(handler_state);
            }
            may_remain_unhandled = matching != ExceptionMatch::Definite;
        }
        if may_remain_unhandled {
            if exception_name.is_none() {
                boundary(
                    result,
                    file,
                    raised.range,
                    "exception type is unknown; handler matching is incomplete",
                );
            }
            let mut raised = raised;
            if uncertain_match {
                let duties = raised
                    .state
                    .duty_origins
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                weaken(&mut raised.state, duties.into_iter());
            }
            pending.raises.push(raised);
        }
    }

    let mut outcomes = FlowResult::default();
    if let Some(mut joined) = join_states(normal_states) {
        joined.control_depth = env.control_depth;
        *env = joined;
        if !finalbody.is_empty() {
            let finally_outcomes = execute(
                finalbody,
                env,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            outcomes.append(finally_outcomes);
        }
    } else {
        env.alive = false;
    }
    if finalbody.is_empty() {
        outcomes.append(pending);
    } else {
        for exit in pending.all_abrupt().cloned().collect::<Vec<_>>() {
            execute_finally_for_exit(
                exit,
                finalbody,
                file,
                contracts,
                functions,
                result,
                depth,
                &mut outcomes,
            );
        }
    }
    outcomes
}

#[allow(clippy::too_many_arguments)]
fn execute_loop(
    test: Option<&AstExpr>,
    iterator: Option<&AstExpr>,
    target: Option<&str>,
    body: &[FlowStatement],
    alternate: &[FlowStatement],
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> FlowResult {
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
        boundary(result, file, range, "loop nesting budget exceeded");
        return FlowResult::default();
    }
    let mut outcomes = FlowResult::default();
    if let Some(iterator) = iterator {
        consume_expr(iterator, env, file, contracts, functions, result, 0);
        outcomes.raises.extend(take_pending_raises(env));
        let affected = env.places.values().flatten().cloned().collect::<Vec<_>>();
        weaken(env, affected.into_iter());
        boundary(
            result,
            file,
            iterator.range,
            "for-iterator effects and element aliases are not modeled",
        );
        queue_raise(env, iterator.range, None);
        outcomes.raises.extend(take_pending_raises(env));
    }
    if !env.alive {
        return outcomes;
    }
    if matches!(
        test.map(|test| &test.kind),
        Some(AstExprKind::Boolean(false))
    ) {
        return execute(
            alternate,
            env,
            file,
            contracts,
            functions,
            result,
            depth + 1,
        );
    }
    let always_true = matches!(
        test.map(|test| &test.kind),
        Some(AstExprKind::Boolean(true))
    );
    let entry = env.clone();
    let mut head = entry.clone();
    let mut breaks = Vec::new();
    let mut converged = false;
    for _ in 0..MAX_LOOP_ITERATIONS {
        let mut iteration = head.clone();
        iteration.alive = true;
        if let Some(test) = test {
            consume_expr(test, &mut iteration, file, contracts, functions, result, 0);
            outcomes.raises.extend(take_pending_raises(&mut iteration));
        }
        if let Some(target) = target {
            iteration.places.remove(target);
            iteration.unknown_places.insert(target.to_owned());
        }
        if !iteration.alive {
            outcomes.raises.push(FlowExit {
                state: iteration,
                range,
                kind: CompletionKind::Raise(None),
            });
            break;
        }
        iteration.control_depth = iteration.control_depth.saturating_add(1);
        iteration.loop_depth = iteration.loop_depth.saturating_add(1);
        let iteration_outcomes = execute(
            body,
            &mut iteration,
            file,
            contracts,
            functions,
            result,
            depth + 1,
        );
        outcomes.returns.extend(iteration_outcomes.returns);
        outcomes.raises.extend(iteration_outcomes.raises);
        outcomes
            .breaks
            .extend(iteration_outcomes.breaks.iter().cloned());
        outcomes
            .continues
            .extend(iteration_outcomes.continues.iter().cloned());
        breaks.extend(iteration_outcomes.breaks.into_iter().map(|mut exit| {
            exit.state.control_depth = env.control_depth;
            exit.state.loop_depth = env.loop_depth;
            exit.state
        }));
        let mut back_edges = Vec::new();
        if iteration.alive {
            iteration.control_depth = env.control_depth;
            iteration.loop_depth = env.loop_depth;
            back_edges.push(iteration);
        }
        back_edges.extend(iteration_outcomes.continues.into_iter().map(|mut exit| {
            exit.state.control_depth = env.control_depth;
            exit.state.loop_depth = env.loop_depth;
            exit.state
        }));
        let Some(back_edge) = join_states(back_edges) else {
            converged = true;
            break;
        };
        let next = merge(&entry, &back_edge).0;
        if next.objects == head.objects
            && next.places == head.places
            && next.unknown_places == head.unknown_places
            && next.escaped == head.escaped
        {
            head = next;
            converged = true;
            break;
        }
        head = next;
    }
    if !converged {
        let affected = head.places.values().flatten().cloned().collect::<Vec<_>>();
        weaken(&mut head, affected.into_iter());
        boundary(result, file, range, "loop fixed-point budget exceeded");
    }
    let mut normal_states = breaks;
    if !always_true {
        let mut natural_exit = head;
        natural_exit.alive = true;
        if let Some(test) = test {
            consume_expr(
                test,
                &mut natural_exit,
                file,
                contracts,
                functions,
                result,
                0,
            );
            outcomes
                .raises
                .extend(take_pending_raises(&mut natural_exit));
        }
        if natural_exit.alive {
            let alternate_outcomes = execute(
                alternate,
                &mut natural_exit,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            outcomes.append(alternate_outcomes);
            if natural_exit.alive {
                normal_states.push(natural_exit);
            }
        } else {
            outcomes.raises.push(FlowExit {
                state: natural_exit,
                range,
                kind: CompletionKind::Raise(None),
            });
        }
    }
    if let Some(mut joined) = join_states(normal_states) {
        joined.alive = true;
        *env = joined;
    } else {
        env.alive = false;
    }
    outcomes.breaks.clear();
    outcomes.continues.clear();
    outcomes
}

#[allow(clippy::too_many_arguments)]
fn execute_with(
    items: &[FlowWithItem],
    body: &[FlowStatement],
    asynchronous: bool,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> FlowResult {
    #[derive(Clone)]
    struct Path {
        state: State,
        completion: Option<(CompletionKind, SourceRange)>,
    }

    fn unwind(
        mut path: Path,
        cleanups: &[ContextCleanup],
        remaining: usize,
        file: &SourceFile,
        contracts: &[Contract],
        functions: &FunctionMap,
        result: &mut Inspection,
    ) -> Vec<Path> {
        if remaining == 0 {
            return vec![path];
        }
        let cleanup = &cleanups[remaining - 1];
        path.state.alive = true;
        path.state.pending_raises.clear();
        run_context_cleanup(cleanup, &mut path.state, file, contracts, functions, result);
        let raises = take_pending_raises(&mut path.state);
        let had_raises = !raises.is_empty();
        let mut paths = Vec::new();
        for raised in raises {
            paths.extend(unwind(
                Path {
                    state: raised.state,
                    completion: Some((raised.kind, raised.range)),
                },
                cleanups,
                remaining - 1,
                file,
                contracts,
                functions,
                result,
            ));
        }
        if path.state.alive {
            paths.extend(unwind(
                path,
                cleanups,
                remaining - 1,
                file,
                contracts,
                functions,
                result,
            ));
        } else if !had_raises {
            paths.extend(unwind(
                Path {
                    state: path.state.clone(),
                    completion: Some((CompletionKind::Raise(None), cleanup.range)),
                },
                cleanups,
                remaining - 1,
                file,
                contracts,
                functions,
                result,
            ));
        }
        paths
    }

    let mut cleanups = Vec::new();
    let mut uncertain = BTreeSet::new();
    let mut entry_raises = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !env.alive {
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            break;
        }
        let Some(method) = contextlib_cleanup_method(&item.context, env).filter(|method| {
            (asynchronous && *method == "aclose") || (!asynchronous && *method == "close")
        }) else {
            consume_expr(&item.context, env, file, contracts, functions, result, 0);
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            uncertain.extend(env.places.values().flatten().cloned());
            if let Some(target) = &item.target {
                env.places.remove(target);
                env.unknown_places.insert(target.clone());
                env.shadowed_functions.insert(target.clone());
                env.shadowed_classes.insert(target.clone());
            }
            boundary(
                result,
                file,
                item.range,
                if asynchronous {
                    "asynchronous context-manager cleanup has no qualified model"
                } else {
                    "context-manager cleanup has no qualified model"
                },
            );
            continue;
        };

        let AstExprKind::Call {
            arguments,
            keywords,
            ..
        } = &item.context.kind
        else {
            unreachable!("contextlib cleanup recognition requires a call")
        };
        let object = match (arguments.as_slice(), keywords.as_slice()) {
            ([object], []) => Some(object),
            ([], [(Some(name), object)]) if name == "thing" => Some(object),
            _ => None,
        };
        let Some(object) = object else {
            for argument in arguments {
                consume_expr(argument, env, file, contracts, functions, result, 0);
            }
            for (_, argument) in keywords {
                consume_expr(argument, env, file, contracts, functions, result, 0);
            }
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            uncertain.extend(env.places.values().flatten().cloned());
            boundary(
                result,
                file,
                item.range,
                "contextlib cleanup call has unsupported arguments",
            );
            queue_raise(
                env,
                item.range,
                Some(AstExpr {
                    range: item.range,
                    kind: AstExprKind::Name("TypeError".into()),
                }),
            );
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            env.alive = false;
            break;
        };
        let receiver = synthetic_name(env, "with", item.range, index);
        execute_non_if_statement(
            &FlowStatement::Assign {
                name: receiver.clone(),
                value: object.clone(),
                range: object.range,
            },
            env,
            file,
            contracts,
            functions,
            result,
        );
        entry_raises.extend(
            take_pending_raises(env)
                .into_iter()
                .map(|exit| (exit, cleanups.len())),
        );
        if !env.alive {
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            break;
        }
        if !env.places.contains_key(&receiver) || env.unknown_places.contains(&receiver) {
            uncertain.extend(env.places.values().flatten().cloned());
            boundary(
                result,
                file,
                item.range,
                "contextlib cleanup receiver is not a resolved tracked resource",
            );
        } else {
            if item.unsupported_target {
                boundary(
                    result,
                    file,
                    item.range,
                    "context-manager target destructuring is not modeled",
                );
            }
            if let Some(target) = &item.target {
                if let Some(targets) = env.places.get(&receiver).cloned() {
                    env.places.insert(target.clone(), targets);
                    env.unknown_places.remove(target);
                    env.shadowed_functions.insert(target.clone());
                    env.shadowed_classes.insert(target.clone());
                }
            }
            cleanups.push(ContextCleanup {
                receiver,
                method,
                asynchronous: method == "aclose",
                range: item.range,
            });
        }
    }

    let mut body_outcomes = if env.alive {
        execute(body, env, file, contracts, functions, result, depth + 1)
    } else {
        FlowResult::default()
    };
    let mut paths = Vec::new();
    if env.alive {
        paths.push((
            Path {
                state: env.clone(),
                completion: None,
            },
            cleanups.len(),
        ));
    }
    for exit in body_outcomes.all_abrupt().cloned() {
        paths.push((
            Path {
                state: exit.state,
                completion: Some((exit.kind, exit.range)),
            },
            cleanups.len(),
        ));
    }
    for (exit, entered) in entry_raises {
        paths.push((
            Path {
                state: exit.state,
                completion: Some((exit.kind, exit.range)),
            },
            entered,
        ));
    }

    let mut outcomes = FlowResult::default();
    let mut normal_states = Vec::new();
    for (path, remaining) in paths {
        for mut path in unwind(
            path, &cleanups, remaining, file, contracts, functions, result,
        ) {
            if !uncertain.is_empty() {
                weaken(&mut path.state, uncertain.iter().cloned());
            }
            path.state.alive = true;
            match path.completion {
                None => normal_states.push(path.state),
                Some((kind, range)) => {
                    let exit = FlowExit {
                        state: path.state,
                        range,
                        kind,
                    };
                    push_exit(&mut outcomes, exit);
                }
            }
        }
    }

    if let Some(mut normal) = join_states(normal_states) {
        normal.alive = true;
        *env = normal;
    } else {
        env.alive = false;
    }
    body_outcomes.returns.clear();
    body_outcomes.raises.clear();
    body_outcomes.breaks.clear();
    body_outcomes.continues.clear();
    let _ = range;
    outcomes
}

fn contextlib_cleanup_method(expression: &AstExpr, env: &State) -> Option<&'static str> {
    if !matches!(env.target_python_version.as_str(), "3.11" | "3.12" | "3.13") {
        return None;
    }
    let AstExprKind::Call { function, .. } = &expression.kind else {
        return None;
    };
    match &function.kind {
        AstExprKind::Name(name) if !env.shadowed_functions.contains(name) => {
            match env.function_bindings.get(name).map(String::as_str) {
                Some("external:contextlib.closing") => Some("close"),
                Some("external:contextlib.aclosing") => Some("aclose"),
                _ => None,
            }
        }
        AstExprKind::Attribute(module, name) => {
            let module_binding = expr_name(module)
                .filter(|module| !env.shadowed_functions.contains(*module))
                .and_then(|module| env.function_bindings.get(module))
                .map(String::as_str);
            match (module_binding, name.as_str()) {
                (Some("external:contextlib"), "closing") => Some("close"),
                (Some("external:contextlib"), "aclosing") => Some("aclose"),
                _ => None,
            }
        }
        _ => None,
    }
}

fn run_context_cleanup(
    cleanup: &ContextCleanup,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let receiver = &cleanup.receiver;
    let method = cleanup.method;
    let asynchronous = cleanup.asynchronous;
    let range = cleanup.range;
    let receiver_expr = AstExpr {
        range,
        kind: AstExprKind::Name(receiver.to_owned()),
    };
    let call = AstExpr {
        range,
        kind: AstExprKind::Call {
            function: Box::new(AstExpr {
                range,
                kind: AstExprKind::Attribute(Box::new(receiver_expr), method.to_owned()),
            }),
            arguments: Vec::new(),
            keywords: Vec::new(),
        },
    };
    if asynchronous {
        consume_expr(
            &AstExpr {
                range,
                kind: AstExprKind::Await(Box::new(call)),
            },
            env,
            file,
            contracts,
            functions,
            result,
            0,
        );
    } else {
        consume_expr(&call, env, file, contracts, functions, result, 0);
    }
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
            env.pending_return_targets = None;
            let source_coroutine = expr_name(value)
                .and_then(|source| env.coroutine_aliases.get(source))
                .cloned();
            env.coroutine_aliases.remove(name);
            env.unknown_coroutine_aliases.remove(name);
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
                        let previous_object_mask = env.objects.get(&object).copied();
                        let repeated_allocation = env.loop_depth > 0
                            && previous_object_mask
                                .is_some_and(|mask| mask & (OPEN | UNKNOWN) != 0);
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
                        env.objects.insert(
                            object.clone(),
                            if repeated_allocation {
                                previous_object_mask.unwrap_or(UNKNOWN) | OPEN | UNKNOWN
                            } else {
                                OPEN
                            },
                        );
                        env.duty_origins.insert(object.clone(), span(file, *range));
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
                                    if !child.pending_raises.is_empty() {
                                        boundary(
                                            result,
                                            file,
                                            *range,
                                            "constructor may fail before acquisition completes; partial initialization is unknown",
                                        );
                                    }
                                    let mut propagated_raises = child
                                        .pending_raises
                                        .iter()
                                        .map(|exit| project_call_exit(env, exit, *range))
                                        .collect::<Vec<_>>();
                                    for exit in &mut propagated_raises {
                                        exit.state.objects.remove(&object);
                                        exit.state.resources.remove(&object);
                                        exit.state.duty_origins.remove(&object);
                                        exit.state.close_origins.remove(&object);
                                    }
                                    env.pending_raises.extend(propagated_raises);
                                    apply_call_effects(env, &child, &BTreeSet::new());
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
                        if repeated_allocation {
                            let current = env.objects.get(&object).copied().unwrap_or(UNKNOWN);
                            env.objects.insert(
                                object.clone(),
                                current | previous_object_mask.unwrap_or(UNKNOWN) | OPEN | UNKNOWN,
                            );
                            boundary(
                                result,
                                file,
                                *range,
                                "repeated loop allocation is summarized with unknown multiplicity",
                            );
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
            if matches!(value.kind, AstExprKind::Call { .. })
                && call_may_return_resource(value, env, file, functions)
            {
                consume_expr(value, env, file, contracts, functions, result, 0);
                if let Some(coroutine) = env.pending_coroutine_result.take() {
                    env.coroutine_aliases.insert(name.clone(), coroutine);
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    return None;
                }
                if let Some(targets) = env.pending_return_targets.take() {
                    env.places.insert(name.clone(), targets);
                    env.unknown_places.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    return None;
                }
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
            if let Some(source) = expr_name(value) {
                if let Some(coroutine) = source_coroutine {
                    env.coroutine_aliases.insert(name.clone(), coroutine);
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                } else if env.unknown_coroutine_aliases.contains(source) {
                    env.coroutine_aliases.remove(name);
                    env.unknown_coroutine_aliases.insert(name.clone());
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                } else if let Some(objects) = env.places.get(source).cloned() {
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
                if let Some(coroutine) = env.pending_coroutine_result.take() {
                    env.coroutine_aliases.insert(name.clone(), coroutine);
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    return None;
                }
                if let Some(targets) = env.pending_return_targets.take() {
                    env.places.insert(name.clone(), targets);
                    env.unknown_places.remove(name);
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    return None;
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
            consume_expr(value, env, file, contracts, functions, result, 0);
            env.pending_coroutine_result = None;
        }
        FlowStatement::Pass { .. } => {}
        FlowStatement::Return { value, .. } => {
            env.pending_return_targets = None;
            if let Some(value) = value {
                let temporary = format!("__typepact_return_{}", value.range.start);
                let constructor = match &value.kind {
                    AstExprKind::Call { function, .. } => expr_name(function).is_some_and(|name| {
                        env.class_bindings.get(name).is_some_and(Option::is_some)
                            && !env.shadowed_classes.contains(name)
                    }),
                    _ => false,
                };
                if constructor {
                    execute_non_if_statement(
                        &FlowStatement::Assign {
                            name: temporary.clone(),
                            value: value.clone(),
                            range: value.range,
                        },
                        env,
                        file,
                        contracts,
                        functions,
                        result,
                    );
                    env.pending_return_targets = env.places.get(&temporary).cloned();
                } else {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                }
            }
            env.pending_coroutine_result = None;
            if !env.alive {
                return None;
            }
            let mut returned = env.clone();
            returned.alive = true;
            env.alive = false;
            return Some(returned);
        }
        FlowStatement::Raise { exception, .. } => {
            if let Some(exception) = exception {
                consume_expr(exception, env, file, contracts, functions, result, 0);
            }
            if !env.alive {
                return None;
            }
            env.alive = false;
        }
        FlowStatement::Break { .. } | FlowStatement::Continue { .. } => {
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
        FlowStatement::If { .. }
        | FlowStatement::Try { .. }
        | FlowStatement::Loop { .. }
        | FlowStatement::With { .. } => {
            unreachable!("compound statements use the stack-bounded dispatcher")
        }
    }
    None
}

fn statement_range(statement: &FlowStatement) -> SourceRange {
    match statement {
        FlowStatement::Pass { range }
        | FlowStatement::Assign { range, .. }
        | FlowStatement::Expression { range, .. }
        | FlowStatement::If { range, .. }
        | FlowStatement::Try { range, .. }
        | FlowStatement::Loop { range, .. }
        | FlowStatement::With { range, .. }
        | FlowStatement::Return { range, .. }
        | FlowStatement::Raise { range, .. }
        | FlowStatement::Break { range }
        | FlowStatement::Continue { range }
        | FlowStatement::BindFunction { range, .. }
        | FlowStatement::Unsupported { range } => *range,
    }
}

fn check_bodies_impl(
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
    only_file: Option<&str>,
    python_version: &str,
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
                        return_access: module_contracts
                            .iter()
                            .find(|contract| {
                                contract.display_name == format!("{name}.return")
                                    && contract.lifecycle.is_some()
                            })
                            .map(|contract| contract.access.clone()),
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
                            return_access: None,
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
    let mut external_exports = BTreeMap::<String, BTreeMap<String, String>>::new();
    for _ in 0..=files.len() {
        let mut changed = false;
        for provider in files {
            let Some(declarations) = summaries
                .get(&provider.path)
                .and_then(|summary| summary.declarations.as_ref())
            else {
                continue;
            };
            let bindings = external_module_bindings(declarations, &external_exports);
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
        let mut function_bindings =
            final_function_bindings(&file.module, &qualified_functions, declarations);
        if let Some(external) = external_exports.get(&file.module) {
            function_bindings.extend(external.clone());
        }
        module_bindings.insert(
            file.path.clone(),
            Arc::new(ModuleBindings {
                functions: function_bindings,
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
        let mut final_functions = final_function_bindings(&file.module, &functions, declarations);
        if let Some(external) = external_exports.get(&file.module) {
            final_functions.extend(external.clone());
        }
        for declaration in declarations {
            let Declaration::Function {
                name,
                parameters,
                body,
                range,
                ..
            } = declaration
            else {
                continue;
            };
            let mut env = State {
                alive: true,
                scope_module: file.module.clone(),
                target_python_version: python_version.to_owned(),
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
                } else if parameter_contract.access == "owned" {
                    env.duty_origins
                        .insert(object.clone(), parameter_contract.source.clone());
                }
            }
            let return_access = contracts
                .iter()
                .find(|contract| {
                    contract.display_name == format!("{name}.return")
                        && contract.lifecycle.is_some()
                })
                .map(|contract| contract.access.as_str());
            let flow_outcomes = execute(body, &mut env, file, &contracts, &functions, result, 0);
            let exits = flow_outcomes.all_abrupt().cloned().collect::<Vec<_>>();
            for exit in grouped_exits(&exits) {
                match &exit.kind {
                    CompletionKind::Return(_)
                    | CompletionKind::Raise(_)
                    | CompletionKind::Cancelled => {
                        report_cleanup_exit(result, file, &exit, return_access);
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
            let class_resource =
                class_resource_contract(file, name, &decorator_ranges, &result.contracts);
            for method in methods {
                if method.decorated {
                    continue;
                }
                let mut env = State {
                    alive: true,
                    scope_module: file.module.clone(),
                    target_python_version: python_version.to_owned(),
                    function_bindings: final_functions.clone(),
                    function_bindings_complete: true,
                    class_bindings: final_classes.clone(),
                    class_bindings_complete: true,
                    module_function_bindings: final_functions.clone(),
                    module_class_bindings: final_classes.clone(),
                    ..State::default()
                };
                env.shadowed_functions
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
                    &functions,
                    result,
                    0,
                );
                let exits = outcomes.all_abrupt().cloned().collect::<Vec<_>>();
                for exit in grouped_exits(&exits) {
                    match &exit.kind {
                        CompletionKind::Return(_)
                        | CompletionKind::Raise(_)
                        | CompletionKind::Cancelled => {
                            report_cleanup_exit(result, file, &exit, None);
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
                    );
                }
            }
        }
        let mut module = State {
            alive: true,
            scope_module: file.module.clone(),
            target_python_version: python_version.to_owned(),
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
            report_unawaited_coroutines(&module, file, result);
        }
    }
}

pub(crate) fn check_file_bodies(
    target_path: &str,
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
    result: &mut Inspection,
    python_version: &str,
) {
    check_bodies_impl(files, summaries, result, Some(target_path), python_version);
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

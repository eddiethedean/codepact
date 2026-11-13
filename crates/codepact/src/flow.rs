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

mod ownership;
mod recursion;
mod tasks;
use ownership::OwnershipState;

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const MAX_CALL_DEPTH: usize = 8;
const MAX_CONTROL_FLOW_DEPTH: usize = 64;
const MAX_ALIAS_TARGETS: usize = 64;
const MAX_FLOW_OBJECTS: usize = 4_096;
const MAX_EXIT_PARTITIONS: usize = 256;
const MAX_STATIC_FOR_ITERATIONS: usize = 16;
const MAX_PENDING_COROUTINES: usize = 128;
pub(crate) const MAX_FLOW_STEPS: usize = 100_000;
const UNKNOWN_EXCEPTION_BINDINGS: &str = "*";
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
    call_site: String,
    caller: &'a SourceFile,
}

type ProjectFunctionMap = BTreeMap<(String, String), FunctionBody>;

#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    ownership: Option<Box<OwnershipState>>,
    places: BTreeMap<String, BTreeSet<String>>,
    unknown_places: BTreeSet<String>,
    possibly_unbound_places: BTreeSet<String>,
    shadowed_classes: BTreeSet<String>,
    shadowed_functions: BTreeSet<String>,
    objects: BTreeMap<String, u8>,
    resources: BTreeMap<String, Contract>,
    borrowed: BTreeSet<String>,
    borrowed_origins: BTreeMap<String, Span>,
    forbidden_borrowed_closes: BTreeSet<String>,
    ambiguous_borrowed_owned_aliases: BTreeSet<String>,
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
    models: Arc<FlowModelCatalog>,
    coroutines: BTreeMap<String, CoroutineRecord>,
    coroutine_budget_overflow: Option<SourceRange>,
    coroutine_aliases: BTreeMap<String, String>,
    unknown_coroutine_aliases: BTreeSet<String>,
    pending_coroutine_result: Option<String>,
    close_origins: BTreeMap<String, Span>,
    duty_origins: BTreeMap<String, Span>,
    awaited_call: Option<(String, SourceRange)>,
    allocation_context: Vec<String>,
    pending_return_targets: Option<BTreeSet<String>>,
    pending_raises: Vec<FlowExit>,
    alive: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct CoroutineRecord {
    invocation: AstExpr,
    file_path: String,
    captures: BTreeMap<String, CapturedCoroutinePlace>,
    range: SourceRange,
    origin: Span,
    consumed: bool,
    ambiguous: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct CapturedCoroutinePlace {
    targets: BTreeSet<String>,
    unknown: bool,
    transferred: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum CompletionKind {
    Fallthrough,
    Return(Option<AstExpr>),
    Raise(Option<AstExpr>),
    Cancelled,
    Break,
    Continue,
}

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone)]
enum ContextCleanupFrame {
    Modeled(ContextCleanup),
    Unknown {
        asynchronous: bool,
        range: SourceRange,
    },
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
}

fn expr_name(expr: &AstExpr) -> Option<&str> {
    if let AstExprKind::Name(name) = &expr.kind {
        Some(name)
    } else {
        None
    }
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
        || arguments.iter().any(|arg| {
            matches!(
                arg.kind,
                AstExprKind::Other
                    | AstExprKind::OtherCalls(..)
                    | AstExprKind::Starred(_)
                    | AstExprKind::DepthLimit
            )
        })
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
    if tasks::returns_task_handle_or_status(expression, env) {
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

fn merge(left: &State, right: &State) -> (State, JoinEffects) {
    let mut joined = left.clone();
    if let (Some(left), Some(right)) = (left.ownership.as_deref(), right.ownership.as_deref()) {
        joined.ownership = Some(Box::new(OwnershipState::join(left, right)));
    }
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
    joined.coroutine_budget_overflow = left
        .coroutine_budget_overflow
        .or(right.coroutine_budget_overflow);
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
                    place.transferred &= right_place.transferred;
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
    while joined.coroutines.len() > MAX_PENDING_COROUTINES {
        let Some(id) = joined
            .coroutines
            .iter()
            .min_by_key(|(id, record)| (!record.consumed, record.range.start, *id))
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        let Some(record) = joined.coroutines.remove(&id) else {
            continue;
        };
        let mut affected = BTreeSet::new();
        for place in record.captures.values() {
            affected.extend(place.targets.iter().cloned());
            if place.unknown {
                affected.extend(joined.objects.keys().cloned());
            }
        }
        for object in affected {
            *joined.objects.entry(object).or_default() |= UNKNOWN;
        }
        for (alias, coroutine) in &joined.coroutine_aliases {
            if coroutine == &id {
                joined.unknown_coroutine_aliases.insert(alias.clone());
            }
        }
        joined
            .coroutine_aliases
            .retain(|_, coroutine| coroutine != &id);
        joined.coroutine_budget_overflow =
            Some(joined.coroutine_budget_overflow.unwrap_or(record.range));
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
    if left.class_bindings.contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        || right
            .class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
    {
        joined
            .class_bindings
            .insert(UNKNOWN_EXCEPTION_BINDINGS.to_owned(), None);
    }
    joined
        .unknown_places
        .extend(right.unknown_places.iter().cloned());
    joined
        .possibly_unbound_places
        .extend(right.possibly_unbound_places.iter().cloned());
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
                    && !right
                        .ownership
                        .as_ref()
                        .is_some_and(|ownership| ownership.definitely_moved(&place))
            } else {
                !left.unknown_places.contains(&place)
                    && !left
                        .ownership
                        .as_ref()
                        .is_some_and(|ownership| ownership.definitely_moved(&place))
            };
            if missing_path_is_unbound {
                joined.possibly_unbound_places.insert(place.clone());
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
    joined
        .forbidden_borrowed_closes
        .extend(right.forbidden_borrowed_closes.iter().cloned());
    joined
        .ambiguous_borrowed_owned_aliases
        .extend(right.ambiguous_borrowed_owned_aliases.iter().cloned());
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
    if let (Some(parent), Some(child)) = (state.ownership.as_mut(), child.state.ownership.as_ref())
    {
        parent.apply_runtime_effects(child);
    }
    state.coroutine_budget_overflow = child
        .state
        .coroutine_budget_overflow
        .or(parent.coroutine_budget_overflow);
    for object in parent.objects.keys() {
        if let (Some(target), Some(source)) =
            (state.ownership.as_mut(), child.state.ownership.as_ref())
        {
            target.copy_transfers(source, object);
        }
        if child.state.forbidden_borrowed_closes.contains(object) {
            if child.state.escaped.contains(object) {
                state.escaped.insert(object.clone());
            }
            continue;
        }
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

fn contract_identity(contract: Option<&Contract>) -> Vec<String> {
    contract
        .into_iter()
        .map(|contract| {
            serde_json::to_string(&(
                &contract.base_symbol,
                &contract.lifecycle,
                &contract.access,
                &contract.provenance,
            ))
            .expect("canonical contract identity serializes")
        })
        .collect()
}

fn report(
    result: &mut Inspection,
    code: &str,
    message: &str,
    primary: Span,
    related: Vec<Span>,
    certainty: &str,
    identity: Vec<String>,
) {
    if let Some(existing) = result.diagnostics.iter_mut().find(|diagnostic| {
        diagnostic.code == code
            && diagnostic.primary == primary
            && (code != "TP1002"
                || (diagnostic.message == message && diagnostic.related.first() == related.first()))
    }) {
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
        existing.semantic_identity.extend(identity);
        existing.semantic_identity.sort();
        existing.semantic_identity.dedup();
        return;
    }
    result.diagnostics.push(Diagnostic {
        semantic_identity: identity,
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
    functions: &FunctionMap,
    classes_with_unresolved_bases: &BTreeSet<String>,
) {
    report_unawaited_coroutines(&exit.state, file, result);
    let exit_span = span(file, exit.range);
    let mut returned_owned = BTreeSet::new();
    let mut escaped_return = BTreeSet::new();
    if let CompletionKind::Return(value) = &exit.kind {
        let targets = exit
            .state
            .pending_return_targets
            .clone()
            .or_else(|| {
                if exit.state.ownership.is_some() {
                    return None;
                }
                value
                    .as_ref()
                    .and_then(expr_name)
                    .filter(|name| !exit.state.unknown_places.contains(*name))
                    .and_then(|name| exit.state.places.get(name).cloned())
            })
            .unwrap_or_default();
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
            escaped_return.extend(targets);
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
            .clone()
            .or_else(|| match &exit.kind {
                CompletionKind::Return(Some(value)) if exit.state.ownership.is_none() => {
                    expr_name(value)
                        .filter(|name| !exit.state.unknown_places.contains(*name))
                        .and_then(|name| exit.state.places.get(name).cloned())
                }
                _ => None,
            })
            .unwrap_or_default();
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
                    contract_identity(exit.state.resources.get(object)),
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
                    contract_identity(exit.state.resources.get(object)),
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
        let resource = exit.state.resources.get(object);
        let has_finalizer = resource
            .is_some_and(|resource| resource_method(resource, "__del__", functions).is_some());
        let has_unresolved_bases = resource
            .is_some_and(|resource| classes_with_unresolved_bases.contains(&resource.base_symbol));
        if has_finalizer || has_unresolved_bases {
            boundary(
                result,
                file,
                exit.range,
                if has_finalizer {
                    "resource finalizer may run implicitly; its cleanup effect is unknown"
                } else {
                    "resource base-class finalizer effects are not modeled"
                },
            );
        }
        let certainty = if mask & (CLOSED | UNKNOWN) == 0 && !has_finalizer && !has_unresolved_bases
        {
            "definite"
        } else {
            "possible"
        };
        let mut related = vec![origin.clone()];
        if let Some(ownership) = exit.state.ownership.as_ref() {
            related.extend(ownership.transfer_sites(object).cloned());
        }
        if let Some(close) = exit.state.close_origins.get(object) {
            related.push(close.clone());
        }
        if let Some(existing) = result.diagnostics.iter_mut().find(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.primary == exit_span
                && diagnostic.message.ends_with(&format!("on {exit_kind}"))
                && diagnostic.related.first() == Some(origin)
        }) {
            existing
                .semantic_identity
                .extend(contract_identity(resource));
            existing.semantic_identity.sort();
            existing.semantic_identity.dedup();
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
                contract_identity(resource),
            );
        }
    }
}

fn report_module_scope_lifetimes(result: &mut Inspection, normal: &State, outcomes: &FlowResult) {
    let mut states = outcomes
        .all_abrupt()
        .map(|exit| &exit.state)
        .collect::<Vec<_>>();
    if normal.alive {
        states.push(normal);
    }

    for state in states {
        let unresolved_origins = state
            .duty_origins
            .iter()
            .filter_map(|(object, origin)| {
                let mask = state.objects.get(object).copied().unwrap_or(UNKNOWN);
                (mask & (OPEN | UNKNOWN) != 0).then(|| origin.clone())
            })
            .collect::<Vec<_>>();
        for origin in unresolved_origins {
            boundary_at_span(
                result,
                origin,
                "module-scope resource lifetime is outside cleanup analysis; module completion is not process shutdown",
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
    boundary_at_span(result, primary, message);
}

fn boundary_at_span(result: &mut Inspection, primary: Span, message: &str) {
    let reason = format!("{}:{}: {message}", primary.file, primary.line);
    result.coverage.status = "incomplete".into();
    if result.coverage.reasons.contains(&reason) {
        return;
    }
    result.coverage.reasons.push(reason);
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
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
    let affected = if let Some(ownership) = env.ownership.as_mut() {
        for group in ownership.groups.values_mut() {
            group.incomplete = true;
        }
        env.objects.keys().cloned().collect::<Vec<_>>()
    } else {
        env.places.values().flatten().cloned().collect::<Vec<_>>()
    };
    weaken(env, affected.into_iter());
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
    recursion::reset();
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

fn collect_assigned_names(statements: &[FlowStatement], names: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            FlowStatement::Assign { name, .. } | FlowStatement::EscapingAssign { name, .. } => {
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
        if parent.borrowed.contains(object) {
            child.borrowed.insert(object.clone());
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
    if let (Some(parent), Some(child)) = (parent.ownership.as_mut(), child.ownership.as_ref()) {
        parent.copy_transfers(child, object);
    }
    if let Some(mask) = child.objects.get(object) {
        parent.objects.insert(object.to_owned(), *mask);
    }
    if let Some(resource) = child.resources.get(object) {
        parent.resources.insert(object.to_owned(), resource.clone());
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

fn await_call_captures(awaited: &AstExpr, state: &State) -> Option<(BTreeSet<String>, bool)> {
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

fn final_function_bindings(
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

fn external_module_bindings(
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

fn resolve_imported_function(
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
        let is_possibly_unbound_name = matches!(
            &argument.kind,
            AstExprKind::Name(name) if env.possibly_unbound_places.contains(name)
        );
        if is_possibly_unbound_name
            || !matches!(
                argument.kind,
                AstExprKind::Name(_)
                    | AstExprKind::String(_)
                    | AstExprKind::Boolean(_)
                    | AstExprKind::NoneLiteral
                    | AstExprKind::Primitive
            )
        {
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

fn report_unawaited_coroutines(state: &State, file: &SourceFile, result: &mut Inspection) {
    if let Some(range) = state.coroutine_budget_overflow {
        boundary(
            result,
            file,
            range,
            "pending coroutine budget exceeded; captured resource effects are unknown",
        );
    }
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
    if let Some(name) = expr_name(expr) {
        report_possibly_unbound_alias(name, expr.range, env, file, result);
    }
    if let Some(ownership) = env.ownership.as_mut() {
        ownership.pending_move = false;
        ownership.pending_task = None;
        env.pending_return_targets = None;
        env.pending_coroutine_result = None;
        if let AstExprKind::Call { function, .. } = &expr.kind {
            if depth <= MAX_CALL_DEPTH && env.call_depth < MAX_CALL_DEPTH {
                if ownership::external_identity(function, env).as_deref() == Some(ownership::MOVE) {
                    if take_flow_step(env, file, expr.range, result) {
                        ownership::move_call(expr, env, file, contracts, functions, result, depth);
                    }
                    return;
                }
                let prepared =
                    ownership::prepare_call(expr, env, file, contracts, functions, result, depth);
                if env.alive && !prepared.invalid_receiver {
                    consume_expr_impl(
                        &prepared.expression,
                        env,
                        file,
                        contracts,
                        functions,
                        result,
                        depth,
                    );
                }
                prepared.release(env);
                return;
            }
        } else if let Some(name) = expr_name(expr) {
            ownership::check_moved(name, env, file, expr.range, result);
            return;
        }
    }
    consume_expr_impl(expr, env, file, contracts, functions, result, depth);
}

fn report_possibly_unbound_alias(
    name: &str,
    range: SourceRange,
    env: &State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    if env.possibly_unbound_places.contains(name) {
        boundary(
            result,
            file,
            range,
            &format!("local resource alias {name} may be unbound on one branch"),
        );
    }
}

fn consume_expr_impl(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    if let AstExprKind::Await(awaited) = &expr.kind {
        tasks::allow_tasks_to_start(env);
        if env.ownership.is_some()
            && matches!(awaited.kind, AstExprKind::Call { .. })
            && resolved_await_call(awaited, file, env, functions)
        {
            let previous_await = env.awaited_call.take();
            consume_expr(awaited, env, file, contracts, functions, result, depth + 1);
            env.awaited_call = previous_await;
            if let Some(id) = env.pending_coroutine_result.take() {
                let alias = format!(":direct-await:{}", expr.range.start);
                env.coroutine_aliases.insert(alias.clone(), id);
                let rewritten = AstExpr {
                    range: expr.range,
                    kind: AstExprKind::Await(Box::new(AstExpr {
                        range: awaited.range,
                        kind: AstExprKind::Name(alias.clone()),
                    })),
                };
                consume_expr_impl(&rewritten, env, file, contracts, functions, result, depth);
                env.coroutine_aliases.remove(&alias);
                return;
            }
            if env.alive {
                boundary(
                    result,
                    file,
                    expr.range,
                    "awaited source call did not establish a coroutine capture",
                );
            }
            return;
        }
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
        let modeled_close = coroutine
            .as_ref()
            .map(|record| modeled_close_for_call(&record.invocation, env))
            .unwrap_or_else(|| modeled_close_for_call(awaited, env));
        let resolved = coroutine.is_some()
            || modeled_close.is_some()
            || resolved_await_call(awaited, file, env, functions);
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
        if env.ownership.is_some() {
            if let Some(record) = &coroutine {
                tasks::restore_captures(record, env);
            }
        }
        let before = env.clone();
        if env.ownership.is_some() {
            if let Some(record) = coroutine_id
                .as_ref()
                .and_then(|id| env.coroutines.get_mut(id))
            {
                record.consumed = true;
            }
        }
        env.awaited_call = coroutine.as_ref().map_or_else(
            || Some((file.path.clone(), awaited.range)),
            |record| Some((record.file_path.clone(), record.invocation.range)),
        );
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
        env.awaited_call = before.awaited_call.clone();
        if let Some(id) = &coroutine_id {
            if let Some(record) = env.coroutines.get_mut(id) {
                record.consumed = true;
            }
            if env.ownership.is_some() {
                for exit in &mut env.pending_raises {
                    if let Some(record) = exit.state.coroutines.get_mut(id) {
                        record.consumed = true;
                    }
                }
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
        let modeled_cancelled = modeled_close
            .as_ref()
            .and_then(|(_, model)| model.cancelled);
        let has_modeled_cancellation = modeled_close.is_some();
        let exact_source_coroutine = env.ownership.is_some() && coroutine.as_ref().is_some_and(|record| {
            !record.ambiguous && matches!(&record.invocation.kind, AstExprKind::Call { function, .. } if expr_name(function).is_some())
        });
        if !exact_source_coroutine
            && before.alive
            && (resolved || env.alive)
            && (!has_modeled_cancellation || modeled_cancelled.is_some())
        {
            let body_affected = env
                .objects
                .iter()
                .filter(|&(object, mask)| {
                    before
                        .objects
                        .get(object)
                        .is_some_and(|previous| previous != mask)
                })
                .map(|(object, _)| object.clone())
                .collect::<BTreeSet<_>>();
            let mut cancelled = before;
            cancelled.alive = true;
            cancelled.pending_raises.clear();
            if let Some(id) = &coroutine_id {
                if let Some(record) = cancelled.coroutines.get_mut(id) {
                    record.consumed = true;
                }
            }
            let direct_captures = if coroutine.is_none() && modeled_close.is_none() && resolved {
                await_call_captures(awaited, &cancelled)
            } else {
                None
            };
            let affected = if let Some(coroutine) = &coroutine {
                let mut affected = coroutine
                    .captures
                    .values()
                    .flat_map(|place| place.targets.iter().cloned())
                    .collect::<BTreeSet<_>>();
                if coroutine.captures.values().any(|place| place.unknown) {
                    affected.extend(cancelled.objects.keys().cloned());
                }
                affected.extend(body_affected.iter().cloned());
                affected
            } else if let Some((object, _)) = &modeled_close {
                let mut affected = BTreeSet::from([object.clone()]);
                affected.extend(body_affected.iter().cloned());
                affected
            } else if let Some((targets, unknown)) = &direct_captures {
                let mut affected = targets.clone();
                if *unknown {
                    affected.extend(cancelled.objects.keys().cloned());
                }
                affected.extend(body_affected.iter().cloned());
                affected
            } else {
                cancelled
                    .places
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<BTreeSet<_>>()
            };
            let modeled_object = modeled_close.as_ref().map(|(object, _)| object.as_str());
            let modeled_pre_mask = modeled_close
                .as_ref()
                .map(|(object, _)| cancelled.objects.get(object).copied().unwrap_or(UNKNOWN));
            if coroutine.is_some() || direct_captures.is_some() {
                for object in affected
                    .into_iter()
                    .filter(|object| Some(object.as_str()) != modeled_object)
                {
                    *cancelled.objects.entry(object).or_default() |= UNKNOWN;
                }
            } else if modeled_close.is_none() {
                weaken(
                    &mut cancelled,
                    affected
                        .into_iter()
                        .filter(|object| Some(object.as_str()) != modeled_object),
                );
            }
            if let (Some((object, _)), Some(previous), Some(postcondition)) =
                (&modeled_close, modeled_pre_mask, modeled_cancelled)
            {
                apply_lifecycle_postcondition_from(
                    &mut cancelled,
                    object,
                    previous,
                    postcondition,
                    (postcondition == LifecyclePostcondition::Closed)
                        .then(|| span(file, expr.range)),
                );
                if postcondition == LifecyclePostcondition::Unknown {
                    boundary(
                        result,
                        file,
                        expr.range,
                        "modeled cancellation postcondition leaves resource state unknown",
                    );
                }
            }
            for capture in &capture_names {
                cancelled.places.remove(capture);
                cancelled.unknown_places.remove(capture);
            }
            env.pending_raises.push(FlowExit {
                state: cancelled,
                range: expr.range,
                kind: CompletionKind::Cancelled,
            });
            if !has_modeled_cancellation {
                boundary(
                    result,
                    file,
                    expr.range,
                    "cancellation may interrupt the awaited operation or its cleanup",
                );
            }
        }
        if let Some(record) = &coroutine {
            tasks::clear_captures(record, env);
            for exit in &mut env.pending_raises {
                tasks::clear_captures(record, &mut exit.state);
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
    if env.ownership.is_none() {
        evaluate_call_arguments(expr, env, file, contracts, functions, result, depth);
    }
    if !env.alive {
        return;
    }
    if let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &expr.kind
    {
        if expr_name(function).is_some_and(|name| is_unshadowed_builtin_exception(name, env)) {
            let modeled_arguments = keywords.is_empty()
                && arguments.iter().all(|argument| {
                    matches!(
                        argument.kind,
                        AstExprKind::String(_)
                            | AstExprKind::Boolean(_)
                            | AstExprKind::Primitive
                            | AstExprKind::NoneLiteral
                    )
                });
            if !modeled_arguments {
                boundary(
                    result,
                    file,
                    expr.range,
                    "computed built-in exception constructor arguments are not modeled",
                );
                queue_raise(env, expr.range, None);
            }
            return;
        }
    }
    if env.ownership.is_some() && tasks::task_call(expr, env, file, result) {
        return;
    }
    if let AstExprKind::Call {
        arguments,
        keywords,
        ..
    } = &expr.kind
    {
        if env.ownership.as_ref().is_some_and(|ownership| {
            !ownership.group_names.is_empty() || !ownership.task_names.is_empty()
        }) {
            let names = arguments
                .iter()
                .chain(keywords.iter().map(|(_, value)| value))
                .filter_map(expr_name)
                .map(str::to_owned)
                .collect();
            ownership::escape_task_handles(env, &names, file, expr.range, result);
        }
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
                        let mut mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
                        let resource = env.resources.get(object).cloned();
                        let is_close = resource
                            .as_ref()
                            .and_then(|resource| resource.lifecycle.as_ref())
                            .is_some_and(|lifecycle| lifecycle.closed_by == method);
                        let effect_model = resource.as_ref().and_then(|resource| {
                            let close_method = resource.lifecycle.as_ref()?.closed_by.as_str();
                            env.models
                                .resolve(
                                    &resource.base_symbol,
                                    close_method,
                                    &env.target_python_version,
                                )
                                .cloned()
                        });
                        let close_model = is_close.then(|| effect_model.clone()).flatten();
                        let post_close_allowed = !is_close
                            && effect_model
                                .as_ref()
                                .is_some_and(|model| model.post_close_methods.contains(method));
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
                        let async_model = is_close
                            && effect_model
                                .as_ref()
                                .is_some_and(|model| model.asynchronous);
                        if (definition.is_some_and(|definition| definition.is_async) || async_model)
                            && !is_awaited_call(expr, file, env)
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
                        if mask & CLOSED != 0 && !is_close && !post_close_allowed {
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
                                contract_identity(resource.as_ref()),
                            );
                        }
                        if is_close {
                            ownership::check_close(
                                env,
                                object,
                                file,
                                expr.range,
                                result,
                                uncertain_receiver,
                            );
                        }
                        if env.borrowed.contains(object) && is_close {
                            env.forbidden_borrowed_closes.insert(object.clone());
                            if !env.ambiguous_borrowed_owned_aliases.contains(object) {
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
                                    contract_identity(resource.as_ref()),
                                );
                            }
                        }
                        if is_close && mask & CLOSED != 0 {
                            match close_model
                                .as_ref()
                                .map(|model| model.repeat_close)
                                .unwrap_or(RepeatClosePolicy::Unspecified)
                            {
                                RepeatClosePolicy::Idempotent => {}
                                RepeatClosePolicy::RequiresOpen => {
                                    let certainty = if mask & (OPEN | UNKNOWN) == 0 {
                                        "definite"
                                    } else {
                                        "possible"
                                    };
                                    report(
                                        result,
                                        "TP1001",
                                        "resource close requires an open resource",
                                        span(file, expr.range),
                                        env.close_origins
                                            .get(object)
                                            .cloned()
                                            .into_iter()
                                            .collect(),
                                        certainty,
                                        contract_identity(resource.as_ref()),
                                    );
                                    let mut closed_path = env.clone();
                                    closed_path.objects.insert(object.clone(), CLOSED);
                                    closed_path.alive = true;
                                    closed_path.pending_raises.clear();
                                    env.pending_raises.push(FlowExit {
                                        state: closed_path,
                                        range: expr.range,
                                        kind: CompletionKind::Raise(None),
                                    });
                                    let remaining = mask & !CLOSED;
                                    if remaining == 0 {
                                        env.alive = false;
                                        return;
                                    }
                                    if remaining & UNKNOWN != 0 {
                                        boundary(
                                            result,
                                            file,
                                            expr.range,
                                            "requires-open repeat close has an unknown lifecycle alternative",
                                        );
                                    }
                                    mask = remaining;
                                    env.objects.insert(object.clone(), remaining);
                                }
                                RepeatClosePolicy::Unspecified => boundary(
                                    result,
                                    file,
                                    expr.range,
                                    "repeat-close behavior is not modeled",
                                ),
                            }
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
                                    caller: file,
                                    call_key: method_key,
                                    call_site: format!(
                                        "{}:{}:{}",
                                        file.path, expr.range.start, expr.range.end
                                    ),
                                },
                                FlowContext { functions, result },
                            );
                            if child
                                .forbidden_borrowed_closes
                                .iter()
                                .any(|object| !env.forbidden_borrowed_closes.contains(object))
                            {
                                boundary(
                                    result,
                                    file,
                                    expr.range,
                                    "resolved method has an invalid Borrowed close effect; lifecycle changes are not applied",
                                );
                            }
                            let mut raised = child
                                .pending_raises
                                .iter()
                                .map(|exit| project_call_exit(env, exit, expr.range))
                                .collect::<Vec<_>>();
                            if let Some(model) = &close_model {
                                if let Some(postcondition) = model.exceptional {
                                    if raised.is_empty() {
                                        let mut exceptional = env.clone();
                                        exceptional.alive = true;
                                        exceptional.pending_raises.clear();
                                        apply_lifecycle_postcondition(
                                            &mut exceptional,
                                            object,
                                            postcondition,
                                            (postcondition == LifecyclePostcondition::Closed)
                                                .then(|| span(file, expr.range)),
                                        );
                                        raised.push(FlowExit {
                                            state: exceptional,
                                            range: expr.range,
                                            kind: CompletionKind::Raise(None),
                                        });
                                    } else {
                                        for exit in &mut raised {
                                            apply_lifecycle_postcondition(
                                                &mut exit.state,
                                                object,
                                                postcondition,
                                                (postcondition == LifecyclePostcondition::Closed)
                                                    .then(|| span(file, expr.range)),
                                            );
                                        }
                                    }
                                    if postcondition == LifecyclePostcondition::Unknown {
                                        boundary(
                                            result,
                                            file,
                                            expr.range,
                                            "modeled exceptional postcondition leaves resource state unknown",
                                        );
                                    }
                                }
                            }
                            env.pending_raises.extend(raised);
                            apply_call_effects(env, &child, &BTreeSet::new());
                            if is_close && child.alive {
                                if let Some(postcondition) =
                                    close_model.as_ref().and_then(|model| model.normal)
                                {
                                    apply_lifecycle_postcondition(
                                        env,
                                        object,
                                        postcondition,
                                        (postcondition == LifecyclePostcondition::Closed)
                                            .then(|| span(file, expr.range)),
                                    );
                                    if postcondition == LifecyclePostcondition::Unknown {
                                        boundary(
                                            result,
                                            file,
                                            expr.range,
                                            "modeled normal postcondition leaves resource state unknown",
                                        );
                                    }
                                } else if close_model.is_some() {
                                    env.alive = false;
                                } else {
                                    env.close_origins
                                        .insert(object.clone(), span(file, expr.range));
                                    env.objects.insert(
                                        object.clone(),
                                        closed_mask(mask, env.escaped.contains(object)),
                                    );
                                }
                            }
                            if !child.alive {
                                env.alive = false;
                            }
                        } else if is_close {
                            if let Some(model) = &close_model {
                                let exceptional_base = env.clone();
                                if let Some(postcondition) = model.normal {
                                    apply_lifecycle_postcondition(
                                        env,
                                        object,
                                        postcondition,
                                        (postcondition == LifecyclePostcondition::Closed)
                                            .then(|| span(file, expr.range)),
                                    );
                                    if postcondition == LifecyclePostcondition::Unknown {
                                        boundary(
                                            result,
                                            file,
                                            expr.range,
                                            "modeled normal postcondition leaves resource state unknown",
                                        );
                                    }
                                } else {
                                    env.alive = false;
                                }
                                if let Some(postcondition) = model.exceptional {
                                    let mut exceptional = exceptional_base;
                                    exceptional.alive = true;
                                    exceptional.pending_raises.clear();
                                    apply_lifecycle_postcondition(
                                        &mut exceptional,
                                        object,
                                        postcondition,
                                        (postcondition == LifecyclePostcondition::Closed)
                                            .then(|| span(file, expr.range)),
                                    );
                                    env.pending_raises.push(FlowExit {
                                        state: exceptional,
                                        range: expr.range,
                                        kind: CompletionKind::Raise(None),
                                    });
                                    if postcondition == LifecyclePostcondition::Unknown {
                                        boundary(
                                            result,
                                            file,
                                            expr.range,
                                            "modeled exceptional postcondition leaves resource state unknown",
                                        );
                                    }
                                }
                            } else {
                                env.close_origins
                                    .insert(object.clone(), span(file, expr.range));
                                env.objects.insert(
                                    object.clone(),
                                    closed_mask(mask, env.escaped.contains(object)),
                                );
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
                            }
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
                            if matches!(diagnostic.code.as_str(), "TP1001" | "TP2006")
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
            let external_binding = if env.shadowed_functions.contains(callee) {
                None
            } else if env.module_scope || env.function_bindings_complete {
                env.function_bindings.get(callee).map(String::as_str)
            } else {
                None
            };
            if external_binding == Some("external:codepact.discard") {
                let value = match (arguments.as_slice(), keywords.as_slice()) {
                    ([value], []) => Some(value),
                    ([], [(name, value)]) if name.as_deref() == Some("value") => Some(value),
                    _ => None,
                };
                for argument in arguments {
                    consume_expr(argument, env, file, contracts, functions, result, depth + 1);
                }
                for (_, argument) in keywords {
                    consume_expr(argument, env, file, contracts, functions, result, depth + 1);
                }
                if value.is_none() {
                    boundary(
                        result,
                        file,
                        expr.range,
                        "codepact.discard call does not match its declared single-value signature",
                    );
                    queue_raise(env, expr.range, None);
                }
                return;
            }
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
                if definition.is_async && !is_awaited_call(expr, file, env) {
                    if env.ownership.is_some() {
                        ownership::check_coroutine_arguments(
                            definition,
                            &bindings,
                            env,
                            &span(file, expr.range),
                            result,
                        );
                    }
                    register_pending_coroutine(
                        expr,
                        env,
                        file,
                        result,
                        definition.binding_key.clone(),
                    );
                    return;
                }
                if env.ownership.is_none() && env.call_stack.contains(callee) {
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
                    ownership: OwnershipState::for_child(
                        env,
                        definition.return_access.as_deref() == Some("owned"),
                    ),
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
                    awaited_call: None,
                    allocation_context: {
                        let mut context = env.allocation_context.clone();
                        context.push(format!(
                            "{}:{}:{}",
                            file.path, expr.range.start, expr.range.end
                        ));
                        context
                    },
                    target_python_version: env.target_python_version.clone(),
                    models: env.models.clone(),
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
                let mut owned_entry_objects = BTreeSet::new();
                let mut borrowed_entry_objects = BTreeSet::new();
                for (parameter, argument) in bindings {
                    if let Some(actual) = expr_name(argument) {
                        if let Some(targets) = env.places.get(actual) {
                            child.places.insert(parameter.clone(), targets.clone());
                            copy_object_metadata(&mut child, env, targets);
                            if let Some(contract) = definition.parameter_contracts.get(&parameter) {
                                for object in targets {
                                    if contract.access == "owned" {
                                        if env.ownership.is_none() {
                                            owned_entry_objects.insert(object.clone());
                                        }
                                        child
                                            .duty_origins
                                            .entry(object.clone())
                                            .or_insert_with(|| contract.source.clone());
                                    } else if contract.access == "shared" {
                                        borrowed_entry_objects.insert(object.clone());
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
                    if env.ownership.is_some()
                        && ownership::verify_call_binding(
                            definition, &parameter, argument, env, &mut child, &call_site, result,
                        )
                    {
                        if let Some(targets) =
                            expr_name(argument).and_then(|name| env.places.get(name))
                        {
                            owned_entry_objects.extend(targets.iter().cloned());
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
                let mut ambiguous_borrowed_owned_aliases = owned_entry_objects
                    .intersection(&borrowed_entry_objects)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                ambiguous_borrowed_owned_aliases.extend(
                    owned_entry_objects
                        .iter()
                        .filter(|object| env.borrowed.contains(*object))
                        .cloned(),
                );
                child
                    .ambiguous_borrowed_owned_aliases
                    .extend(ambiguous_borrowed_owned_aliases.iter().cloned());
                let diagnostic_start = result.diagnostics.len();
                let returned =
                    recursion::execute_function(definition, &mut child, functions, result);
                let mut forbidden_borrowed_closes = child
                    .forbidden_borrowed_closes
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                forbidden_borrowed_closes.extend(
                    returned
                        .all_abrupt()
                        .flat_map(|exit| exit.state.forbidden_borrowed_closes.iter().cloned()),
                );
                forbidden_borrowed_closes
                    .retain(|object| !env.forbidden_borrowed_closes.contains(object));
                let ambiguous_close_effects = forbidden_borrowed_closes
                    .intersection(&ambiguous_borrowed_owned_aliases)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                forbidden_borrowed_closes
                    .retain(|object| !ambiguous_close_effects.contains(object));
                if !forbidden_borrowed_closes.is_empty() {
                    boundary(
                        result,
                        file,
                        expr.range,
                        "callee has an invalid Borrowed close effect; lifecycle changes are not applied",
                    );
                }
                if !ambiguous_close_effects.is_empty() {
                    for object in &ambiguous_close_effects {
                        owned_entry_objects.remove(object);
                        *env.objects.entry(object.clone()).or_default() |= CLOSED | UNKNOWN;
                    }
                    boundary(
                        result,
                        file,
                        expr.range,
                        "same resource passed as Owned and Borrowed has an unresolved close effect",
                    );
                }
                let mut owned_return_targets = BTreeSet::new();
                let mut unresolved_owned_return = false;
                if definition.return_access.as_deref() == Some("owned") {
                    unresolved_owned_return = child.alive;
                    for exit in &returned.returns {
                        let CompletionKind::Return(_) = &exit.kind else {
                            continue;
                        };
                        let targets = exit
                            .state
                            .pending_return_targets
                            .clone()
                            .or_else(|| match &exit.kind {
                                CompletionKind::Return(Some(value))
                                    if exit.state.ownership.is_none() =>
                                {
                                    expr_name(value)
                                        .filter(|name| !exit.state.unknown_places.contains(*name))
                                        .and_then(|name| exit.state.places.get(name).cloned())
                                }
                                _ => None,
                            })
                            .unwrap_or_default();
                        let valid = targets
                            .iter()
                            .filter(|object| {
                                exit.state.objects.get(*object).copied().unwrap_or(UNKNOWN) == OPEN
                                    && !exit.state.borrowed.contains(*object)
                            })
                            .cloned()
                            .collect::<BTreeSet<_>>();
                        unresolved_owned_return |= valid.is_empty() || valid.len() != targets.len();
                        owned_return_targets.extend(valid);
                    }
                }
                let propagated_raises = returned
                    .raises
                    .iter()
                    .map(|exit| project_call_exit(env, exit, expr.range))
                    .map(|mut exit| {
                        if env.ownership.is_none() {
                            for object in &owned_entry_objects {
                                exit.state.duty_origins.remove(object);
                            }
                        }
                        exit
                    })
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
                if env.ownership.is_some() && unresolved_owned_return {
                    weaken(&mut child, owned_return_targets.iter().cloned());
                    boundary(
                        result,
                        file,
                        expr.range,
                        "a normal exit does not establish the declared Owned result",
                    );
                }
                child.pending_return_targets =
                    (!owned_return_targets.is_empty()).then(|| owned_return_targets.clone());
                for object in &owned_entry_objects {
                    if env.ownership.is_none() {
                        env.duty_origins.remove(object);
                    }
                    if !owned_return_targets.contains(object) {
                        child.duty_origins.remove(object);
                    }
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
                if env.ownership.is_some() {
                    affected.extend(ownership::argument_escapes(
                        arguments, keywords, env, file, expr.range, result,
                    ));
                    if let Some(ownership) = env.ownership.as_ref() {
                        affected.extend(
                            ownership
                                .closure_captures(callee)
                                .filter_map(|name| env.places.get(name))
                                .flatten()
                                .cloned(),
                        );
                    }
                }
                weaken(env, affected.iter().cloned());
                env.escaped.extend(affected);
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
        AstExprKind::Collection(items) | AstExprKind::Tuple(items) if env.ownership.is_some() => {
            for item in items {
                let names = ownership::stored_names(item);
                if expr_name(item).is_none() {
                    consume_expr(item, env, file, contracts, functions, result, depth + 1);
                }
                if !env.alive {
                    return;
                }
                ownership::borrow_escape(
                    env,
                    names,
                    file,
                    item.range,
                    result,
                    "borrowed view escapes into a container",
                );
                ownership::escape_pending_value(env, file, item.range, result);
            }
        }
        AstExprKind::Starred(value) if env.ownership.is_some() => {
            consume_expr(value, env, file, contracts, functions, result, depth + 1);
            ownership::escape_pending_value(env, file, expr.range, result);
            boundary(
                result,
                file,
                expr.range,
                "expanded arguments or values have unresolved ownership effects",
            );
        }
        AstExprKind::OtherCalls(..) | AstExprKind::Attribute(..) | AstExprKind::Subscript(..)
            if env.ownership.is_some() =>
        {
            let names = ownership::expression_names(expr);
            let mut affected = env.escaped.clone();
            let mut only_moved = !names.is_empty();
            for name in names {
                only_moved &= ownership::check_moved(&name, env, file, expr.range, result);
                affected.extend(env.places.get(&name).into_iter().flatten().cloned());
            }
            weaken(env, affected.into_iter());
            if !only_moved {
                boundary(
                    result,
                    file,
                    expr.range,
                    "unsupported expression may affect or use a tracked resource",
                );
            }
        }
        AstExprKind::Closure(captures) if env.ownership.is_some() => {
            ownership::borrow_escape(
                env,
                captures.iter().cloned(),
                file,
                expr.range,
                result,
                "borrowed view escapes through a closure",
            );
        }
        AstExprKind::Tuple(items) => {
            for item in items {
                consume_expr(item, env, file, contracts, functions, result, depth + 1);
            }
        }
        AstExprKind::Other
        | AstExprKind::Collection(_)
        | AstExprKind::Starred(_)
        | AstExprKind::Closure(_)
        | AstExprKind::OtherCalls(..)
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
            if let AstExprKind::Boolean(value) = &test.kind {
                let branch = if *value { body } else { alternate };
                let control_depth = env.control_depth;
                env.control_depth += 1;
                outcomes.append(execute(
                    branch,
                    env,
                    file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                ));
                env.control_depth = control_depth;
                ownership::cap_handles(env, file, *range, result);
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
            ownership::cap_handles(env, file, *range, result);
            if reachable {
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
                    if env.alive && !env.module_scope {
                        if let FlowStatement::EscapingAssign { name, range, .. } = statement {
                            let targets = env.places.get(name).cloned().unwrap_or_default();
                            if env.ownership.is_some() {
                                ownership::borrow_escape(
                                    env,
                                    [name.clone()],
                                    file,
                                    *range,
                                    result,
                                    "borrowed view escapes through a global or enclosing binding",
                                );
                            }
                            if !targets.is_empty() || env.unknown_places.contains(name) {
                                env.escaped.extend(targets.iter().cloned());
                                weaken(env, targets.into_iter());
                                boundary(
                                    result,
                                    file,
                                    *range,
                                    "resource assignment escapes to a global or enclosing scope outside function cleanup analysis",
                                );
                            }
                        }
                    }
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
    let pending_return_targets = exit.state.pending_return_targets.clone();
    let preserves_return_value = matches!(exit.kind, CompletionKind::Return(_));
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
        if preserves_return_value {
            state.pending_return_targets = pending_return_targets;
        }
        push_exit(outcomes, FlowExit { state, ..exit });
    }
    outcomes.append(finally_outcomes);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExceptionMatch {
    No,
    Possible,
    Definite,
}

fn expression_exception_names(expression: &AstExpr) -> Option<Vec<&str>> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(vec![name]),
        AstExprKind::Tuple(items) => items
            .iter()
            .map(expression_exception_names)
            .collect::<Option<Vec<_>>>()
            .map(|items| items.into_iter().flatten().collect()),
        _ => None,
    }
}

fn raised_exception_name(exit: &FlowExit) -> Option<&str> {
    completion_exception_name(&exit.kind, &exit.state)
}

fn completion_exception_name<'a>(kind: &'a CompletionKind, state: &State) -> Option<&'a str> {
    if matches!(kind, CompletionKind::Cancelled) {
        return Some("CancelledError");
    }
    let CompletionKind::Raise(Some(expression)) = kind else {
        return None;
    };
    let name = match &expression.kind {
        AstExprKind::Call { function, .. } => expr_name(function),
        _ => expr_name(expression),
    }?;
    is_unshadowed_exception_handler_name(name, state).then_some(name)
}

fn known_builtin_exception(name: &str) -> bool {
    crate::frontend::is_known_builtin_exception_name(name)
}

fn known_exception_for_match(name: &str) -> bool {
    known_builtin_exception(name) || name == "CancelledError"
}

fn builtin_exception_parents(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "CancelledError" => &["BaseException"],
        "BaseException" => &[],
        "Exception" | "BaseExceptionGroup" => &["BaseException"],
        "ExceptionGroup" => &["BaseExceptionGroup", "Exception"],
        "ArithmeticError" | "AssertionError" | "AttributeError" | "BufferError" | "EOFError"
        | "ImportError" | "LookupError" | "MemoryError" | "NameError" | "ReferenceError"
        | "RuntimeError" | "StopAsyncIteration" | "StopIteration" | "SyntaxError"
        | "SystemError" | "TypeError" | "ValueError" | "Warning" => &["Exception"],
        "BlockingIOError" | "ChildProcessError" | "ConnectionError" | "FileExistsError"
        | "FileNotFoundError" | "InterruptedError" | "IsADirectoryError" | "NotADirectoryError"
        | "PermissionError" | "ProcessLookupError" | "TimeoutError" => &["OSError"],
        "BrokenPipeError"
        | "ConnectionAbortedError"
        | "ConnectionRefusedError"
        | "ConnectionResetError" => &["ConnectionError"],
        "FloatingPointError" | "OverflowError" | "ZeroDivisionError" => &["ArithmeticError"],
        "BytesWarning"
        | "DeprecationWarning"
        | "EncodingWarning"
        | "FutureWarning"
        | "ImportWarning"
        | "PendingDeprecationWarning"
        | "ResourceWarning"
        | "RuntimeWarning"
        | "SyntaxWarning"
        | "UnicodeWarning"
        | "UserWarning" => &["Warning"],
        "GeneratorExit" | "KeyboardInterrupt" | "SystemExit" => &["BaseException"],
        "ModuleNotFoundError" => &["ImportError"],
        "IndexError" | "KeyError" => &["LookupError"],
        "NotImplementedError" | "RecursionError" => &["RuntimeError"],
        "IndentationError" => &["SyntaxError"],
        "TabError" => &["IndentationError"],
        "UnboundLocalError" => &["NameError"],
        "UnicodeDecodeError" | "UnicodeEncodeError" | "UnicodeTranslateError" => &["UnicodeError"],
        "UnicodeError" => &["ValueError"],
        "OSError" => &["Exception"],
        name if known_builtin_exception(name) => return None,
        _ => return None,
    })
}

fn builtin_exception_match(exception: &str, caught: &str) -> Option<bool> {
    if !known_exception_for_match(exception) || !known_exception_for_match(caught) {
        return None;
    }
    if exception == caught {
        return Some(true);
    }
    let mut pending = vec![exception];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let parents = builtin_exception_parents(current)?;
        if parents.contains(&caught) {
            return Some(true);
        }
        pending.extend(parents.iter().copied());
    }
    Some(false)
}

fn is_unshadowed_builtin_exception(name: &str, state: &State) -> bool {
    known_builtin_exception(name)
        && !state
            .class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        && !state
            .module_class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        && !state.shadowed_classes.contains(name)
        && !state.shadowed_functions.contains(name)
        && !state.function_bindings.contains_key(name)
        && !state.class_bindings.contains_key(name)
        && !state.imported_classes.contains_key(name)
        && !state.module_function_bindings.contains_key(name)
        && !state.module_class_bindings.contains_key(name)
}

fn is_unshadowed_exception_handler_name(name: &str, state: &State) -> bool {
    if known_builtin_exception(name) {
        return is_unshadowed_builtin_exception(name, state);
    }
    name == "CancelledError"
        && !state
            .class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        && !state
            .module_class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        && !state.shadowed_classes.contains(name)
        && !state.shadowed_functions.contains(name)
        && !state.function_bindings.contains_key(name)
        && !state.class_bindings.contains_key(name)
        && !state.module_function_bindings.contains_key(name)
        && !state.module_class_bindings.contains_key(name)
}

fn exception_handler_match(
    exception: Option<&str>,
    handler: Option<&AstExpr>,
    state: &State,
) -> ExceptionMatch {
    let Some(handler) = handler else {
        return ExceptionMatch::Definite;
    };
    let Some(caught) = expression_exception_names(handler) else {
        return ExceptionMatch::Possible;
    };
    let mut possible = false;
    for caught in caught {
        if caught == "BaseException" && is_unshadowed_exception_handler_name(caught, state) {
            return ExceptionMatch::Definite;
        }
        let matching = is_unshadowed_exception_handler_name(caught, state)
            .then(|| exception.and_then(|exception| builtin_exception_match(exception, caught)))
            .flatten();
        match matching {
            Some(true) => return ExceptionMatch::Definite,
            Some(false) => {}
            None => possible = true,
        }
    }
    if possible {
        ExceptionMatch::Possible
    } else {
        ExceptionMatch::No
    }
}

fn clear_exception_target(state: &mut State, name: &str) {
    state.possibly_unbound_places.remove(name);
    if let Some(ownership) = state.ownership.as_mut() {
        ownership.rebind(name);
    }
    state.places.remove(name);
    state.unknown_places.remove(name);
    state.coroutine_aliases.remove(name);
    state.unknown_coroutine_aliases.remove(name);
    state.function_bindings.remove(name);
    state.class_bindings.remove(name);
    state.shadowed_functions.insert(name.to_owned());
    state.shadowed_classes.insert(name.to_owned());
}

fn clear_exception_target_from_outcomes(outcomes: &mut FlowResult, name: &str) {
    for exit in outcomes
        .returns
        .iter_mut()
        .chain(&mut outcomes.raises)
        .chain(&mut outcomes.breaks)
        .chain(&mut outcomes.continues)
    {
        clear_exception_target(&mut exit.state, name);
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
        let mut unmatched_state = raised.state.clone();
        for handler in handlers {
            if !may_remain_unhandled {
                break;
            }
            let mut dispatch_state = unmatched_state.clone();
            dispatch_state.alive = true;
            dispatch_state.pending_raises.clear();
            if let Some(exception_type) = &handler.exception {
                consume_expr(
                    exception_type,
                    &mut dispatch_state,
                    file,
                    contracts,
                    functions,
                    result,
                    0,
                );
                pending
                    .raises
                    .extend(take_pending_raises(&mut dispatch_state));
            }
            if !dispatch_state.alive {
                may_remain_unhandled = false;
                break;
            }
            let matching = exception_handler_match(
                exception_name,
                handler.exception.as_ref(),
                &dispatch_state,
            );
            if matching == ExceptionMatch::No {
                unmatched_state = dispatch_state;
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
            let mut handler_state = dispatch_state.clone();
            handler_state.control_depth = handler_state.control_depth.saturating_add(1);
            if let Some(name) = &handler.name {
                handler_state.possibly_unbound_places.remove(name);
                if let Some(ownership) = handler_state.ownership.as_mut() {
                    ownership.rebind(name);
                }
                handler_state.places.remove(name);
                handler_state.unknown_places.insert(name.clone());
                handler_state.shadowed_functions.insert(name.clone());
                handler_state.shadowed_classes.insert(name.clone());
            }
            let mut handler_outcomes = execute(
                &handler.body,
                &mut handler_state,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            if let Some(name) = &handler.name {
                clear_exception_target(&mut handler_state, name);
                clear_exception_target_from_outcomes(&mut handler_outcomes, name);
            }
            pending.append(handler_outcomes);
            if handler_state.alive {
                normal_states.push(handler_state);
            }
            may_remain_unhandled = matching != ExceptionMatch::Definite;
            if may_remain_unhandled {
                unmatched_state = dispatch_state;
            }
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
            raised.state = unmatched_state;
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
    if let (Some(iterator), Some(target)) = (iterator, target) {
        if let AstExprKind::Tuple(items) = &iterator.kind {
            if items.iter().all(|item| match &item.kind {
                AstExprKind::Name(name) => {
                    !env.unknown_places.contains(name)
                        && (env.places.contains_key(name)
                            || env.coroutine_aliases.contains_key(name)
                            || env.shadowed_classes.contains(name)
                            || env.shadowed_functions.contains(name)
                            || env.function_bindings.contains_key(name)
                            || env.class_bindings.contains_key(name)
                            || env.module_function_bindings.contains_key(name)
                            || env.module_class_bindings.contains_key(name)
                            || env.imported_classes.contains_key(name))
                }
                AstExprKind::Boolean(_)
                | AstExprKind::String(_)
                | AstExprKind::Primitive
                | AstExprKind::NoneLiteral => true,
                _ => false,
            }) {
                return execute_static_for_loop(
                    items, target, body, alternate, range, env, file, contracts, functions, result,
                    depth,
                );
            }
        }
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
            iteration.possibly_unbound_places.remove(target);
            if let Some(ownership) = iteration.ownership.as_mut() {
                ownership.rebind(target);
            }
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
        if next == head {
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
fn execute_static_for_loop(
    items: &[AstExpr],
    target: &str,
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
    let captured_places = items
        .iter()
        .map(|item| match &item.kind {
            AstExprKind::Name(source) => env.places.get(source).cloned(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let captured_coroutines = items
        .iter()
        .map(|item| match &item.kind {
            AstExprKind::Name(source) => env.coroutine_aliases.get(source).cloned(),
            _ => None,
        })
        .collect::<Vec<_>>();
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
    let mut current = Some(env.clone());
    let mut breaks = Vec::new();
    let mut iterations = 0;
    for (index, item) in items.iter().take(MAX_STATIC_FOR_ITERATIONS).enumerate() {
        let Some(mut iteration) = current.take() else {
            break;
        };
        iterations += 1;
        if matches!(&item.kind, AstExprKind::Name(_)) {
            iteration.possibly_unbound_places.remove(target);
            if let Some(targets) = captured_places[index].clone() {
                iteration.places.insert(target.to_owned(), targets);
                iteration.unknown_places.remove(target);
            } else {
                iteration.places.remove(target);
                iteration.unknown_places.remove(target);
            }
            if let Some(coroutine) = &captured_coroutines[index] {
                if iteration
                    .coroutines
                    .get(coroutine)
                    .is_some_and(|record| !record.consumed)
                {
                    boundary(
                        result,
                        file,
                        item.range,
                        "for-loop tuple contains a pending coroutine alias",
                    );
                    iteration
                        .unknown_coroutine_aliases
                        .insert(target.to_owned());
                } else {
                    iteration
                        .coroutine_aliases
                        .insert(target.to_owned(), coroutine.clone());
                    iteration.unknown_coroutine_aliases.remove(target);
                }
            } else {
                iteration.coroutine_aliases.remove(target);
                iteration.unknown_coroutine_aliases.remove(target);
            }
        } else {
            iteration.possibly_unbound_places.remove(target);
            iteration.places.remove(target);
            iteration.unknown_places.remove(target);
            iteration.coroutine_aliases.remove(target);
            iteration.unknown_coroutine_aliases.remove(target);
        }
        if let Some(ownership) = iteration.ownership.as_mut() {
            ownership.rebind(target);
        }
        ownership::assign_handle(
            target,
            None,
            false,
            &mut iteration,
            file,
            item.range,
            result,
        );
        iteration.shadowed_functions.insert(target.to_owned());
        iteration.shadowed_classes.insert(target.to_owned());
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
        let mut next_states = Vec::new();
        if iteration.alive {
            iteration.control_depth = env.control_depth;
            iteration.loop_depth = env.loop_depth;
            next_states.push(iteration);
        }
        for mut exit in iteration_outcomes.continues {
            exit.state.control_depth = env.control_depth;
            exit.state.loop_depth = env.loop_depth;
            next_states.push(exit.state);
        }
        breaks.extend(iteration_outcomes.breaks.into_iter().map(|mut exit| {
            exit.state.control_depth = env.control_depth;
            exit.state.loop_depth = env.loop_depth;
            exit.state
        }));
        current = join_states(next_states);
    }

    if items.len() > iterations && iterations == MAX_STATIC_FOR_ITERATIONS && current.is_some() {
        if let Some(state) = &mut current {
            weaken(
                state,
                state
                    .duty_origins
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
        }
        boundary(
            result,
            file,
            range,
            "static for-loop iteration budget exceeded",
        );
    }

    let mut normal_states = breaks;
    if let Some(mut natural_exit) = current {
        natural_exit.alive = true;
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
    }
    if let Some(mut joined) = join_states(normal_states) {
        joined.alive = true;
        *env = joined;
    } else {
        env.alive = false;
    }
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

    fn partition_kind(path: &Path) -> u8 {
        match path.completion.as_ref().map(|(kind, _)| kind) {
            None | Some(CompletionKind::Fallthrough) => 0,
            Some(CompletionKind::Return(_)) => 1,
            Some(CompletionKind::Raise(_)) => 2,
            Some(CompletionKind::Cancelled) => 3,
            Some(CompletionKind::Break) => 4,
            Some(CompletionKind::Continue) => 5,
        }
    }

    fn merge_overflow_path(previous: &mut Path, incoming: Path) {
        previous.state = merge(&previous.state, &incoming.state).0;
        previous.state.alive = true;
        match (&mut previous.completion, incoming.completion) {
            (
                Some((CompletionKind::Raise(previous), _)),
                Some((CompletionKind::Raise(incoming), _)),
            ) if previous != &incoming => {
                *previous = None;
            }
            (
                Some((CompletionKind::Return(previous), _)),
                Some((CompletionKind::Return(incoming), _)),
            ) if previous != &incoming => {
                *previous = None;
            }
            _ => {}
        }
    }

    fn schedule_path(
        pending: &mut Vec<(Path, usize)>,
        generated_partitions: &mut usize,
        overflow: &mut BTreeMap<u8, Path>,
        path: Path,
        remaining: usize,
        file: &SourceFile,
        result: &mut Inspection,
        range: SourceRange,
    ) {
        if *generated_partitions < MAX_EXIT_PARTITIONS {
            *generated_partitions += 1;
            pending.push((path, remaining));
            return;
        }

        boundary(
            result,
            file,
            range,
            "context-manager exit partition budget exceeded; remaining cleanup outcomes are unknown",
        );
        let kind = partition_kind(&path);
        if let Some(previous) = overflow.get_mut(&kind) {
            merge_overflow_path(previous, path);
        } else {
            overflow.insert(kind, path);
        }
    }

    fn unwind(
        path: Path,
        cleanups: &[ContextCleanupFrame],
        remaining: usize,
        file: &SourceFile,
        contracts: &[Contract],
        functions: &FunctionMap,
        result: &mut Inspection,
        generated_partitions: &mut usize,
        overflow: &mut BTreeMap<u8, Path>,
    ) -> Vec<Path> {
        let mut pending = Vec::new();
        let mut completed = Vec::new();
        if remaining == 0 {
            return vec![path];
        }
        let initial_cleanup_range = match &cleanups[remaining - 1] {
            ContextCleanupFrame::Modeled(cleanup) => cleanup.range,
            ContextCleanupFrame::Unknown { range, .. } => *range,
        };
        schedule_path(
            &mut pending,
            generated_partitions,
            overflow,
            path,
            remaining,
            file,
            result,
            initial_cleanup_range,
        );
        while let Some((mut path, remaining)) = pending.pop() {
            if remaining == 0 {
                completed.push(path);
                continue;
            }
            let cleanup = cleanups[remaining - 1].clone();
            let cleanup_range = match &cleanup {
                ContextCleanupFrame::Modeled(cleanup) => cleanup.range,
                ContextCleanupFrame::Unknown { range, .. } => *range,
            };
            if !take_flow_step(&mut path.state, file, cleanup_range, result) {
                completed.push(path);
                continue;
            }
            match cleanup {
                ContextCleanupFrame::Modeled(cleanup) => {
                    path.state.alive = true;
                    path.state.pending_raises.clear();
                    run_context_cleanup(
                        &cleanup,
                        path.completion.as_ref().map(|(kind, _)| kind),
                        &mut path.state,
                        file,
                        contracts,
                        functions,
                        result,
                    );
                    let raises = take_pending_raises(&mut path.state);
                    let had_raises = !raises.is_empty();
                    for raised in raises {
                        schedule_path(
                            &mut pending,
                            generated_partitions,
                            overflow,
                            Path {
                                state: raised.state,
                                completion: Some((raised.kind, raised.range)),
                            },
                            remaining - 1,
                            file,
                            result,
                            cleanup_range,
                        );
                    }
                    if path.state.alive {
                        schedule_path(
                            &mut pending,
                            generated_partitions,
                            overflow,
                            path,
                            remaining - 1,
                            file,
                            result,
                            cleanup_range,
                        );
                    } else if !had_raises {
                        schedule_path(
                            &mut pending,
                            generated_partitions,
                            overflow,
                            Path {
                                state: path.state,
                                completion: Some((CompletionKind::Raise(None), cleanup.range)),
                            },
                            remaining - 1,
                            file,
                            result,
                            cleanup_range,
                        );
                    }
                }
                ContextCleanupFrame::Unknown {
                    asynchronous,
                    range,
                } => {
                    path.state.alive = true;
                    path.state.pending_raises.clear();
                    if matches!(
                        path.completion.as_ref().map(|(kind, _)| kind),
                        Some(CompletionKind::Raise(_) | CompletionKind::Cancelled)
                    ) {
                        let mut suppressed = path.clone();
                        suppressed.completion = None;
                        schedule_path(
                            &mut pending,
                            generated_partitions,
                            overflow,
                            suppressed,
                            remaining - 1,
                            file,
                            result,
                            cleanup_range,
                        );
                    }
                    schedule_path(
                        &mut pending,
                        generated_partitions,
                        overflow,
                        Path {
                            state: path.state.clone(),
                            completion: Some((CompletionKind::Raise(None), range)),
                        },
                        remaining - 1,
                        file,
                        result,
                        cleanup_range,
                    );
                    if asynchronous {
                        schedule_path(
                            &mut pending,
                            generated_partitions,
                            overflow,
                            Path {
                                state: path.state.clone(),
                                completion: Some((CompletionKind::Cancelled, range)),
                            },
                            remaining - 1,
                            file,
                            result,
                            cleanup_range,
                        );
                    }
                    schedule_path(
                        &mut pending,
                        generated_partitions,
                        overflow,
                        path,
                        remaining - 1,
                        file,
                        result,
                        cleanup_range,
                    );
                }
            }
        }
        completed
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
        if asynchronous && env.ownership.is_some() && tasks::is_group_context(&item.context, env) {
            if let Some(id) = tasks::enter_group(item, env, file, result) {
                cleanups.push(ContextCleanupFrame::Modeled(ContextCleanup {
                    receiver: id,
                    method: ":task-group-exit",
                    asynchronous: false,
                    range: item.range,
                }));
                continue;
            }
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
                if let Some(ownership) = env.ownership.as_mut() {
                    ownership.rebind(target);
                }
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
            if env.alive {
                entry_raises.push((
                    FlowExit {
                        state: env.clone(),
                        range: item.range,
                        kind: CompletionKind::Raise(None),
                    },
                    cleanups.len(),
                ));
                if asynchronous {
                    entry_raises.push((
                        FlowExit {
                            state: env.clone(),
                            range: item.range,
                            kind: CompletionKind::Cancelled,
                        },
                        cleanups.len(),
                    ));
                }
                cleanups.push(ContextCleanupFrame::Unknown {
                    asynchronous,
                    range: item.range,
                });
            }
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
                annotation: None,
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
                env.possibly_unbound_places.remove(target);
                if let Some(targets) = env.places.get(&receiver).cloned() {
                    env.places.insert(target.clone(), targets);
                    ownership::assign_handle(target, None, false, env, file, item.range, result);
                    env.unknown_places.remove(target);
                    env.shadowed_functions.insert(target.clone());
                    env.shadowed_classes.insert(target.clone());
                }
            }
            cleanups.push(ContextCleanupFrame::Modeled(ContextCleanup {
                receiver,
                method,
                asynchronous: method == "aclose",
                range: item.range,
            }));
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
    let mut unwound_paths = Vec::new();
    let mut generated_partitions = 0;
    let mut overflow_paths = BTreeMap::new();
    for (path, remaining) in paths {
        unwound_paths.extend(unwind(
            path,
            &cleanups,
            remaining,
            file,
            contracts,
            functions,
            result,
            &mut generated_partitions,
            &mut overflow_paths,
        ));
    }
    for (_, mut path) in overflow_paths {
        let affected = path.state.objects.keys().cloned().collect::<Vec<_>>();
        weaken(&mut path.state, affected.into_iter());
        path.state.alive = true;
        unwound_paths.push(path);
    }
    for mut path in unwound_paths {
        if !uncertain.is_empty() {
            let mut affected = uncertain.clone();
            affected.extend(path.state.places.values().flatten().cloned());
            weaken(&mut path.state, affected.into_iter());
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
    incoming: Option<&CompletionKind>,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    if cleanup.method == ":task-group-exit" {
        tasks::join_group(
            &cleanup.receiver,
            cleanup.range,
            incoming,
            env,
            file,
            contracts,
            functions,
            result,
        );
        return;
    }
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
        FlowStatement::Assign {
            name, value, range, ..
        }
        | FlowStatement::EscapingAssign {
            name, value, range, ..
        } => {
            if let Some(source) = expr_name(value) {
                report_possibly_unbound_alias(source, value.range, env, file, result);
            }
            env.pending_return_targets = None;
            if let Some(ownership) = env.ownership.as_mut() {
                ownership.pending_task = None;
                ownership.pending_move = false;
            }
            let source_coroutine = expr_name(value)
                .and_then(|source| env.coroutine_aliases.get(source))
                .cloned();
            env.coroutine_aliases.remove(name);
            env.unknown_coroutine_aliases.remove(name);
            if let (Some(ownership), AstExprKind::Closure(captures)) =
                (env.ownership.as_mut(), &value.kind)
            {
                env.possibly_unbound_places.remove(name);
                ownership.define_closure(name, captures);
                env.places.remove(name);
                env.unknown_places.remove(name);
                env.class_bindings.remove(name);
                env.function_bindings.remove(name);
                env.shadowed_classes.insert(name.clone());
                env.shadowed_functions.insert(name.clone());
                return None;
            }
            let source_function_binding = env
                .ownership
                .as_ref()
                .and_then(|_| ownership::external_identity(value, env));
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
                        let prepared = if env.ownership.is_some() {
                            Some(ownership::prepare_call(
                                value, env, file, contracts, functions, result, 0,
                            ))
                        } else {
                            evaluate_call_arguments(
                                value, env, file, contracts, functions, result, 0,
                            );
                            None
                        };
                        if !env.alive {
                            if let Some(prepared) = prepared {
                                prepared.release(env);
                            }
                            return None;
                        }
                        let value = prepared.as_ref().map_or(value, |call| &call.expression);
                        let new_definition = resource_contract
                            .as_ref()
                            .and_then(|resource| resource_method(resource, "__new__", functions));
                        if new_definition.is_some() {
                            env.possibly_unbound_places.remove(name);
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
                            if let Some(prepared) = prepared {
                                prepared.release(env);
                            }
                            ownership::assign_handle(name, None, false, env, file, *range, result);
                            return None;
                        }
                        let context = env.allocation_context.join("/");
                        let object = format!("{}:{}:{}:{}", file.path, range.start, class, context);
                        let previous_object_mask = env.objects.get(&object).copied();
                        let repeated_allocation = env.loop_depth > 0
                            && previous_object_mask
                                .is_some_and(|mask| mask & (OPEN | UNKNOWN) != 0);
                        if !env.objects.contains_key(&object)
                            && env.objects.len() >= MAX_FLOW_OBJECTS
                        {
                            env.possibly_unbound_places.remove(name);
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
                            if let Some(prepared) = prepared {
                                prepared.release(env);
                            }
                            ownership::assign_handle(name, None, false, env, file, *range, result);
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
                                            caller: file,
                                            call_key: init_key,
                                            call_site: format!(
                                                "{}:{}:{}",
                                                file.path, range.start, range.end
                                            ),
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
                        if let Some(prepared) = prepared {
                            prepared.release(env);
                        }
                        if !env.alive && env.ownership.is_some() {
                            return None;
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
                        env.possibly_unbound_places.remove(name);
                        ownership::assign_handle(name, None, true, env, file, *range, result);
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
                if !env.alive && env.ownership.is_some() {
                    return None;
                }
                env.possibly_unbound_places.remove(name);
                if let Some(coroutine) = env.pending_coroutine_result.take() {
                    if env.coroutines.contains_key(&coroutine) {
                        env.coroutine_aliases.insert(name.clone(), coroutine);
                        env.unknown_coroutine_aliases.remove(name);
                    } else {
                        env.coroutine_aliases.remove(name);
                        env.unknown_coroutine_aliases.insert(name.clone());
                    }
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    ownership::assign_handle(name, None, false, env, file, *range, result);
                    return None;
                }
                if let Some(targets) = env.pending_return_targets.take() {
                    env.places.insert(name.clone(), targets);
                    ownership::assign_handle(name, None, true, env, file, *range, result);
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
                ownership::assign_handle(name, None, false, env, file, *range, result);
                return None;
            }
            if let Some(source) = expr_name(value) {
                ownership::check_moved(source, env, file, value.range, result);
                env.possibly_unbound_places.remove(name);
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
                if !defines_resource_view || env.ownership.is_some() {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                }
                if !env.alive && env.ownership.is_some() {
                    return None;
                }
                env.possibly_unbound_places.remove(name);
                if let Some(coroutine) = env.pending_coroutine_result.take() {
                    if env.coroutines.contains_key(&coroutine) {
                        env.coroutine_aliases.insert(name.clone(), coroutine);
                        env.unknown_coroutine_aliases.remove(name);
                    } else {
                        env.coroutine_aliases.remove(name);
                        env.unknown_coroutine_aliases.insert(name.clone());
                    }
                    env.places.remove(name);
                    env.unknown_places.remove(name);
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    return None;
                }
                if let Some(targets) = env.pending_return_targets.take() {
                    env.places.insert(name.clone(), targets);
                    ownership::assign_handle(name, None, true, env, file, *range, result);
                    env.unknown_places.remove(name);
                    env.shadowed_classes.insert(name.clone());
                    env.class_bindings.remove(name);
                    env.shadowed_functions.insert(name.clone());
                    return None;
                }
                env.places.remove(name);
                env.unknown_places.remove(name);
            }
            ownership::assign_handle(name, expr_name(value), false, env, file, *range, result);
            if let Some(binding) = source_function_binding {
                env.function_bindings.insert(name.clone(), binding);
                env.shadowed_functions.remove(name);
            } else {
                env.shadowed_functions.insert(name.clone());
            }
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
        FlowStatement::Pass { .. } | FlowStatement::AnnotatedDeclaration { .. } => {}
        FlowStatement::Return { value, .. } => {
            env.pending_return_targets = None;
            if let Some(value) = value {
                let temporary = format!("__codepact_return_{}", value.range.start);
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
                            annotation: None,
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
                } else if env.ownership.is_some() && expr_name(value).is_some() {
                    if let Some(name) = expr_name(value) {
                        report_possibly_unbound_alias(name, value.range, env, file, result);
                    }
                    ownership::return_value(value, env, file, result);
                } else if let Some(name) = expr_name(value).filter(|name| {
                    env.places.contains_key(*name) && !env.unknown_places.contains(*name)
                }) {
                    env.pending_return_targets = env.places.get(name).cloned();
                } else {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                }
            }
            if let Some(ownership) = env.ownership.as_ref() {
                if ownership.return_owned && env.pending_return_targets.is_none() && value.is_some()
                {
                    boundary(
                        result,
                        file,
                        statement_range(statement),
                        "Owned return does not establish an open ownership handoff",
                    );
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
        FlowStatement::Raise {
            exception, cause, ..
        } => {
            if let Some(exception) = exception {
                consume_expr(exception, env, file, contracts, functions, result, 0);
            }
            if env.alive {
                if let Some(cause) = cause {
                    consume_expr(cause, env, file, contracts, functions, result, 0);
                }
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
            range,
        } => {
            if let Some(ownership) = env.ownership.as_mut() {
                ownership.rebind(name);
                env.places.remove(name);
                env.unknown_places.remove(name);
                env.coroutine_aliases.remove(name);
                if let Some(external) = ownership
                    .imported_callables
                    .get(&(range.start, name.clone()))
                {
                    env.shadowed_functions.remove(name);
                    env.function_bindings.insert(name.clone(), external.clone());
                    env.class_bindings.remove(name);
                    return None;
                }
            }
            if name == "*" {
                env.class_bindings
                    .insert(UNKNOWN_EXCEPTION_BINDINGS.to_owned(), None);
            }
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
        FlowStatement::Store {
            value,
            target,
            range,
        } if env.ownership.is_some() => {
            let names = ownership::stored_names(value);
            if expr_name(value).is_none() {
                consume_expr(value, env, file, contracts, functions, result, 0);
            }
            if !env.alive {
                return None;
            }
            ownership::borrow_escape(
                env,
                names,
                file,
                *range,
                result,
                "borrowed view escapes through field or container storage",
            );
            ownership::escape_pending_value(env, file, *range, result);
            if let Some(target) = target {
                if let AstExprKind::Attribute(receiver, _) = &target.kind {
                    if let Some(binding) = expr_name(receiver)
                        .and_then(|name| env.function_bindings.get(name))
                        .cloned()
                    {
                        env.ownership
                            .as_mut()
                            .expect("ownership store")
                            .invalidate_module(binding);
                    }
                }
                consume_expr(target, env, file, contracts, functions, result, 0);
            }
            boundary(
                result,
                file,
                *range,
                "field or container storage has unresolved ownership effects",
            );
        }
        FlowStatement::ClosureDefinition {
            name,
            captures,
            range,
        } if env.ownership.is_some() => {
            for capture in captures {
                ownership::check_moved(capture, env, file, *range, result);
            }
            if captures
                .iter()
                .filter_map(|name| env.places.get(name))
                .flatten()
                .next()
                .is_some()
            {
                boundary(
                    result,
                    file,
                    *range,
                    "closure capture retains resource aliases with unresolved invocation effects",
                );
            }
            env.ownership
                .as_mut()
                .expect("closure definition")
                .define_closure(name, captures);
            env.function_bindings.remove(name);
            env.places.remove(name);
            env.unknown_places.remove(name);
            env.shadowed_functions.insert(name.clone());
        }
        FlowStatement::DeleteLocal { names, .. } if env.ownership.is_some() => {
            for name in names {
                env.places.remove(name);
                env.unknown_places.remove(name);
                env.coroutine_aliases.remove(name);
                env.unknown_coroutine_aliases.remove(name);
                env.function_bindings.remove(name);
                env.class_bindings.remove(name);
                env.ownership.as_mut().expect("local deletion").rebind(name);
                env.shadowed_functions.insert(name.clone());
                env.shadowed_classes.insert(name.clone());
            }
        }
        FlowStatement::Unsupported { range }
        | FlowStatement::Store { range, .. }
        | FlowStatement::ClosureDefinition { range, .. }
        | FlowStatement::DeleteLocal { range, .. } => {
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
        | FlowStatement::EscapingAssign { range, .. }
        | FlowStatement::AnnotatedDeclaration { range, .. }
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
        | FlowStatement::Store { range, .. }
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
    files: &[SourceFile],
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
                    is_generator,
                    decorators,
                    must_use_decorator,
                    range,
                    unsupported,
                    ..
                } => {
                    if (!decorators.is_empty() && !*must_use_decorator)
                        || *unsupported
                        || *is_generator
                    {
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
                            supported_binding: (!method.decorated || method.must_use_decorator)
                                && !method.is_generator,
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
                    cyclic_modules,
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
                    must_use_decorator,
                    unsupported,
                    ..
                } => {
                    let key = format!("{}::{}:{}", provider.module, name, range.start);
                    final_bindings.insert(
                        name.clone(),
                        ((decorators.is_empty() || *must_use_decorator)
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
        if file.path != target_path {
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
        let mut final_functions =
            final_function_bindings(&file.module, &functions, declarations, &supplied_modules);
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
                ownership: OwnershipState::new(
                    ownership,
                    contracts.iter().any(|contract| {
                        contract.display_name == format!("{name}.return")
                            && contract.lifecycle.is_some()
                            && contract.access == "owned"
                    }),
                ),
                alive: true,
                scope_module: file.module.clone(),
                target_python_version: python_version.to_owned(),
                models: Arc::new(models.clone()),
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
                        report_cleanup_exit(
                            result,
                            file,
                            &exit,
                            return_access,
                            &functions,
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
                    &functions,
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
            let class_resource =
                class_resource_contract(file, name, &decorator_ranges, &result.contracts);
            for method in methods {
                if method.decorated && !method.must_use_decorator {
                    continue;
                }
                let mut env = State {
                    ownership: OwnershipState::new(ownership, false),
                    alive: true,
                    scope_module: file.module.clone(),
                    target_python_version: python_version.to_owned(),
                    models: Arc::new(models.clone()),
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
                            report_cleanup_exit(
                                result,
                                file,
                                &exit,
                                None,
                                &functions,
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
                        &functions,
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
            function_bindings_complete: true,
            class_bindings_complete: true,
            module_scope: true,
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
                    let external = ownership::external_member(source_module, &import.name)
                        .filter(|_| !supplied_modules.contains(source_module))
                        .map(str::to_owned)
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
            let outcomes = execute(
                &summary.module_body,
                &mut module,
                file,
                &contracts,
                &functions,
                result,
                0,
            );
            report_module_scope_lifetimes(result, &module, &outcomes);
            report_unawaited_coroutines(&module, file, result);
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
        let parsed = crate::frontend::parse_frontend(source, "module", "3.11")
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
                .find(|exit| exit.kind == CompletionKind::Cancelled)
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

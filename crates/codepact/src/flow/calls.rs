//! Call evaluation at the boundary between syntax and the flow engine.
use super::{
    boundary, consume_expr, expr_name, is_unshadowed_builtin_exception, library, outcomes,
    ownership::{cap_handles, check_moved, qualified_function_key},
    process, providers, queue_raise, report_possibly_unbound_alias, runtime, span, tasks, weaken,
    AstExpr, AstExprKind, CallKeyword, CallSignature, Contract, FunctionMap, Inspection,
    SourceFile, SourceRange, State,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub(super) enum BindingFailure {
    Invalid,
    Unknown,
}

pub(super) fn bind_arguments<'a>(
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
                    | AstExprKind::Yield(_)
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

pub(super) fn binding_failure(
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
                | AstExprKind::EvaluatedSqlModelSelect { .. }
        ) {
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
            env.outcomes.remember_argument(argument.range);
        }
    }
}

pub(super) struct PreparedCall {
    pub(super) expression: AstExpr,
    temporaries: Vec<String>,
    pub(super) invalid_receiver: bool,
}

impl PreparedCall {
    pub(super) fn release(self, env: &mut State) {
        fn clear(env: &mut State, names: &[String]) {
            let ownership = env.ownership.as_mut().expect("prepared ownership call");
            for name in names {
                env.outcomes.bind(name, outcomes::Values::new());
                env.places.remove(name);
                env.unknown_places.remove(name);
                env.coroutine_aliases.remove(name);
                env.unknown_coroutine_aliases.remove(name);
                env.function_bindings.remove(name);
                ownership.rebind(name);
                ownership.transfer_arguments.remove(name);
                ownership.argument_origins.remove(name);
            }
        }
        clear(env, &self.temporaries);
        for exit in &mut env.pending_raises {
            clear(&mut exit.state, &self.temporaries);
        }
    }
}

pub(super) fn prepare_call(
    original: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> PreparedCall {
    let mut prepared = PreparedCall {
        expression: original.clone(),
        temporaries: Vec::new(),
        invalid_receiver: false,
    };
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &mut prepared.expression.kind
    else {
        unreachable!("prepare_call requires a call");
    };
    let published = providers::public_name(function, env)
        .map(|public| format!("provider:{public}"))
        .filter(|key| functions.contains_key(key));
    if let Some(qualified) =
        published.filter(|_| matches!(function.kind, AstExprKind::Attribute(..)))
    {
        let name = format!(":callable:{}", original.range.start);
        env.function_bindings.insert(name.clone(), qualified);
        prepared.temporaries.push(name.clone());
        function.kind = AstExprKind::Name(name);
    } else if let AstExprKind::Attribute(receiver, method) = &mut function.kind {
        if let Some(name) = expr_name(receiver).map(str::to_owned) {
            report_possibly_unbound_alias(&name, receiver.range, env, file, result);
            prepared.invalid_receiver = check_moved(&name, env, file, receiver.range, result);
            let qualified = qualified_function_key(&name, method, env)
                .filter(|key| functions.contains_key(key));
            if let Some(qualified) = qualified {
                let name = format!(":callable:{}", original.range.start);
                env.function_bindings.insert(name.clone(), qualified);
                prepared.temporaries.push(name.clone());
                function.kind = AstExprKind::Name(name);
            } else {
                let known_receiver = env.places.contains_key(&name)
                    || env.ownership.as_ref().is_some_and(|ownership| {
                        ownership.group_names.contains_key(&name)
                            || ownership.task_names.contains_key(&name)
                            || ownership.executor_names.contains_key(&name)
                            || ownership.work.has_name(&name)
                            || ownership.processes.has_handle(&name)
                    })
                    || env
                        .function_bindings
                        .get(&name)
                        .is_some_and(|binding| binding.starts_with("external:"));
                if !known_receiver && (!arguments.is_empty() || !keywords.is_empty()) {
                    queue_raise(env, function.range, None);
                }
                if !arguments.is_empty() || !keywords.is_empty() {
                    snapshot_name(receiver, &name, env, file, &mut prepared.temporaries);
                }
            }
        } else {
            consume_expr(receiver, env, file, contracts, functions, result, depth + 1);
            snapshot_result(receiver, env, file, &mut prepared.temporaries);
        }
    } else if let Some(name) = expr_name(function) {
        prepared.invalid_receiver = check_moved(name, env, file, function.range, result);
    } else {
        consume_expr(function, env, file, contracts, functions, result, depth + 1);
    }

    for argument in arguments
        .iter_mut()
        .chain(keywords.iter_mut().map(|(_, value)| value))
    {
        if !env.alive {
            break;
        }
        if let Some(name) = expr_name(argument).map(str::to_owned) {
            report_possibly_unbound_alias(&name, argument.range, env, file, result);
            check_moved(&name, env, file, argument.range, result);
            snapshot_name(argument, &name, env, file, &mut prepared.temporaries);
        } else if !matches!(
            argument.kind,
            AstExprKind::String(_)
                | AstExprKind::Boolean(_)
                | AstExprKind::NoneLiteral
                | AstExprKind::Primitive
                | AstExprKind::EvaluatedSqlModelSelect { .. }
        ) {
            let sqlmodel_select = library::sqlmodel_select_is_scalar(argument, env);
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
            snapshot_result(argument, env, file, &mut prepared.temporaries);
            if argument.kind == AstExprKind::Primitive {
                if let Some(scalar) = sqlmodel_select {
                    argument.kind = AstExprKind::EvaluatedSqlModelSelect { scalar };
                }
            }
        }
    }
    cap_handles(env, file, original.range, result);
    prepared
}

fn argument_name(expression: &AstExpr, temporary_count: usize) -> String {
    // The leading colon keeps these bindings disjoint from Python names.
    format!(":argument:{}:{temporary_count}", expression.range.start)
}

fn snapshot_name(
    expression: &mut AstExpr,
    source: &str,
    env: &mut State,
    file: &SourceFile,
    temporaries: &mut Vec<String>,
) {
    let ownership = env.ownership.as_mut().expect("ownership call");
    let targets = env.places.get(source).cloned();
    let unknown = env.unknown_places.contains(source);
    let coroutine = env.coroutine_aliases.get(source).cloned();
    let closure = ownership.has_closure(source);
    let group = ownership.group_names.get(source).cloned();
    let task = ownership.task_names.get(source).cloned();
    let executor = ownership.executor_names.get(source).cloned();
    let process = ownership.processes.has_handle(source);
    let work = ownership.work.has_name(source);
    let outcome = env.outcomes.read_carrier(source);
    // Captures restored by the coroutine executor already have value identity.
    if ownership.transfer_arguments.contains(source)
        || targets.is_none()
            && !unknown
            && coroutine.is_none()
            && !closure
            && group.is_none()
            && task.is_none()
            && executor.is_none()
            && !process
            && !work
            && outcome.ids.is_empty()
    {
        return;
    }
    let temporary = argument_name(expression, temporaries.len());
    if let Some(targets) = targets {
        env.places.insert(temporary.clone(), targets);
        ownership
            .argument_origins
            .insert(temporary.clone(), span(file, expression.range));
    }
    if unknown {
        env.unknown_places.insert(temporary.clone());
    }
    if let Some(coroutine) = coroutine {
        env.coroutine_aliases.insert(temporary.clone(), coroutine);
    }
    ownership.bind_closure_snapshot(source, &temporary);
    if let Some(group) = group {
        ownership.group_names.insert(temporary.clone(), group);
    }
    if let Some(task) = task {
        ownership.task_names.insert(temporary.clone(), task);
    }
    if let Some(executor) = executor {
        ownership.executor_names.insert(temporary.clone(), executor);
    }
    if process {
        ownership.processes.bind_alias(&temporary, Some(source));
    }
    ownership.work.snapshot(source, &temporary);
    env.outcomes.bind_carrier(&temporary, outcome);
    expression.kind = AstExprKind::Name(temporary.clone());
    temporaries.push(temporary);
}

fn snapshot_result(
    expression: &mut AstExpr,
    env: &mut State,
    file: &SourceFile,
    temporaries: &mut Vec<String>,
) {
    let ownership = env.ownership.as_mut().expect("ownership call");
    let temporary = argument_name(expression, temporaries.len());
    let targets = env.pending_return_targets.take();
    let coroutine = env.pending_coroutine_result.take();
    let task = ownership.pending_task.take();
    let work = ownership.work.snapshot_pending(&temporary);
    let outcome = env.outcomes.take_carrier();
    let moved = std::mem::take(&mut ownership.pending_move);
    if let Some(targets) = targets {
        env.places.insert(temporary.clone(), targets);
        ownership
            .argument_origins
            .insert(temporary.clone(), span(file, expression.range));
        if moved {
            ownership.transfer_arguments.insert(temporary.clone());
        }
    } else if let Some(coroutine) = coroutine {
        env.coroutine_aliases.insert(temporary.clone(), coroutine);
    } else if let Some(task) = task {
        ownership.task_names.insert(temporary.clone(), task);
    } else if !work && outcome.ids.is_empty() {
        // Keep unsupported argument shapes visible to the existing signature binder.
        if matches!(
            expression.kind,
            AstExprKind::Call { .. } | AstExprKind::Await(_)
        ) {
            expression.kind = AstExprKind::Primitive;
        }
        return;
    }
    env.outcomes.bind_carrier(&temporary, outcome);
    expression.kind = AstExprKind::Name(temporary.clone());
    temporaries.push(temporary);
}

/// Whether a model claimed an already-evaluated call. A declined operation
/// keeps its model evidence and boundary instead of falling through to an
/// unrelated model or to generic call analysis.
#[derive(Clone, Copy)]
pub(super) enum ModelCall {
    Unrecognized,
    Declined,
    Applied,
}

impl ModelCall {
    fn claimed(self) -> bool {
        !matches!(self, Self::Unrecognized)
    }
}

pub(super) fn dispatch_runtime_call(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    // Preserve the order of operations: some recognized calls queue failures,
    // and a signal-handler update may invalidate later waits before generic
    // call evaluation continues.
    runtime::open_resource(expr, env, file, result).claimed()
        || super::library::call(expr, env, file, result)
        || super::asyncio::call(expr, env, file, result).claimed()
        || super::work::call(expr, env, file, result)
        || builtin_exception_call(expr, env, file, result)
        || (env.ownership.is_some() && tasks::task_call(expr, env, file, result))
        || (env.ownership.is_some() && super::executor::call(expr, env, file, functions, result))
        || process::call(expr, env, file, functions, result).claimed()
}

fn builtin_exception_call(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
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
            return true;
        }
    }
    false
}

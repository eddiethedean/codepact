use super::*;

fn outcome_constructor_payload<'a>(
    identity: &str,
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
) -> Result<Option<&'a AstExpr>, ()> {
    let field = match identity {
        "external:codepact.Ok" | "external:codepact.Some" => Some("value"),
        "external:codepact.Err" => Some("error"),
        "external:codepact.Nothing" => None,
        _ => return Err(()),
    };
    if field.is_none() {
        return if arguments.is_empty() && keywords.is_empty() {
            Ok(None)
        } else {
            Err(())
        };
    }
    match (arguments, keywords) {
        ([payload], []) => Ok(Some(payload)),
        ([], [(Some(name), payload)]) if Some(name.as_str()) == field => Ok(Some(payload)),
        _ => Err(()),
    }
}

fn expression_contains_tracked_resource(expression: &AstExpr, env: &State) -> bool {
    match &expression.kind {
        AstExprKind::Name(name) => {
            env.unknown_places.contains(name)
                || env
                    .places
                    .get(name)
                    .is_some_and(|places| !places.is_empty())
        }
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            expression_contains_tracked_resource(value, env)
        }
        AstExprKind::Tuple(values) | AstExprKind::Collection(values) => values
            .iter()
            .any(|value| expression_contains_tracked_resource(value, env)),
        AstExprKind::Starred(value) => expression_contains_tracked_resource(value, env),
        _ => false,
    }
}

pub(super) fn report_unawaited_coroutines(
    state: &State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    if let Some(range) = state.coroutine_budget_overflow {
        boundary(
            result,
            file,
            range,
            "pending coroutine budget exceeded; captured resource effects are unknown",
        );
    }
    for (id, coroutine) in state
        .coroutines
        .iter()
        .filter(|(_, coroutine)| !coroutine.consumed)
    {
        let message = if asyncio::is_scheduled(state, id) {
            "scheduled task can outlive this scope without established completion"
        } else {
            "created coroutine is not awaited before this scope exit; captured resource effects remain unresolved"
        };
        boundary(result, file, coroutine.range, message);
    }
}

pub(super) fn consume_expr(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    env.outcomes.clear_value();
    if let Some(name) = expr_name(expr) {
        env.outcomes.set_value(env.outcomes.read_carrier(name));
        report_possibly_unbound_alias(name, expr.range, env, file, result);
    }
    if env.ownership.is_none() {
        if let AstExprKind::Call { function, .. } = &expr.kind {
            if matches!(function.kind, AstExprKind::Attribute(..)) {
                if let Some(public) = providers::public_name(function, env) {
                    let binding = format!("provider:{public}");
                    if functions.contains_key(&binding) {
                        let callee = format!(":provider-call:{}", expr.range.start);
                        let mut rewritten = expr.clone();
                        let AstExprKind::Call { function, .. } = &mut rewritten.kind else {
                            unreachable!()
                        };
                        function.kind = AstExprKind::Name(callee.clone());
                        env.function_bindings.insert(callee.clone(), binding);
                        consume_expr_impl(
                            &rewritten, env, file, contracts, functions, result, depth,
                        );
                        env.function_bindings.remove(&callee);
                        for exit in &mut env.pending_raises {
                            exit.state.function_bindings.remove(&callee);
                        }
                        return;
                    }
                }
            }
        }
    }
    if let Some(ownership) = env.ownership.as_mut() {
        ownership.pending_move = false;
        ownership.pending_task = None;
        ownership.work.clear_pending();
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
                // Call preparation can replace the receiver after evaluating it.
                let changes_task_factory = tasks::sets_task_factory_on_loop(expr, env);
                let prepared =
                    calls::prepare_call(expr, env, file, contracts, functions, result, depth);
                if env.alive && !prepared.invalid_receiver {
                    if changes_task_factory {
                        tasks::invalidate_task_factory(env, file, expr.range, result);
                    }
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

pub(super) fn report_possibly_unbound_alias(
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

pub(super) fn consume_expr_impl(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    if process::shared_view_access(expr, env, file, result) {
        return;
    }
    if let AstExprKind::Yield(value) = &expr.kind {
        if let Some(value) = value {
            consume_expr(value, env, file, contracts, functions, result, depth + 1);
            let names = ownership::expression_names(value);
            if env.ownership.is_some() {
                ownership::borrow_escape(
                    env,
                    names.clone(),
                    file,
                    value.range,
                    result,
                    "borrowed view escapes through a yield",
                );
                ownership::escape_pending_value(env, file, value.range, result);
            }
            let affected = names
                .iter()
                .filter_map(|name| env.places.get(name))
                .flatten()
                .cloned()
                .collect::<BTreeSet<_>>();
            env.escaped.extend(affected.iter().cloned());
            weaken(env, affected.into_iter());
        }
        if env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
        {
            let message = if env.exclusive.is_empty() {
                "generator suspension is not modeled"
            } else {
                "exclusive borrow remains active across yield; suspension safety is unsupported"
            };
            boundary(result, file, expr.range, message);
            for object in &env.exclusive {
                *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
                if env.resources.get(object).is_some_and(|resource| {
                    env.protocol_machines.contains_key(&resource.base_symbol)
                }) {
                    env.protocol_unknown.insert(object.clone());
                }
            }
        }
        return;
    }
    if let AstExprKind::Await(awaited) = &expr.kind {
        if env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
            && !env.exclusive.is_empty()
        {
            boundary(
                result,
                file,
                expr.range,
                "exclusive borrow remains active across await; suspension safety is unsupported",
            );
            for object in &env.exclusive {
                *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
                if env.resources.get(object).is_some_and(|resource| {
                    env.protocol_machines.contains_key(&resource.base_symbol)
                }) {
                    env.protocol_unknown.insert(object.clone());
                }
            }
        }
        if async_protocols::await_call(
            awaited, expr.range, env, file, contracts, functions, result, depth,
        ) {
            return;
        }
        tasks::allow_tasks_to_start(env);
        if async_resources::await_call(awaited, env, file, contracts, functions, result, depth) {
            return;
        }
        if executor::await_bridge(
            awaited, expr.range, env, file, contracts, functions, result, depth,
        ) {
            return;
        }
        if asyncio::await_combination(
            awaited, expr.range, None, env, file, contracts, functions, result,
        ) {
            return;
        }
        if asyncio::await_local(awaited, expr.range, env, file, contracts, functions, result) {
            return;
        }
        if work::await_future(awaited, env, result, file) {
            return;
        }
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
            || super::library::direct_await(awaited, env)
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
        let library_tracks_cancellation = library::tracks_stream_cancellation(
            coroutine
                .as_ref()
                .map_or(awaited, |record| &record.invocation),
            &before,
        );
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
        let exact_source_coroutine = (outcomes::call_contract(awaited, env, functions).is_some() && resolved)
            || env.ownership.is_some() && coroutine.as_ref().is_some_and(|record| {
            !record.ambiguous && matches!(&record.invocation.kind, AstExprKind::Call { function, .. } if expr_name(function).is_some())
        });
        if !exact_source_coroutine
            && !library_tracks_cancellation
            && before.alive
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
            asyncio::queue_deadline_cancellations(env, expr.range, &cancelled, false);
            env.pending_raises.push(FlowExit {
                state: cancelled,
                range: expr.range,
                kind: CompletionKind::Cancelled(None),
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
        env.outcomes.clear_value();
        if outcomes::resolved_discard(function, env) {
            let value = match (arguments.as_slice(), keywords.as_slice()) {
                ([value], [(Some(reason), _)]) if reason == "reason" => Some(value),
                ([], [(Some(a), value), (Some(b), _)]) if a == "value" && b == "reason" => {
                    Some(value)
                }
                ([], [(Some(a), _), (Some(b), value)]) if a == "reason" && b == "value" => {
                    Some(value)
                }
                _ => None,
            };
            let values = value.map_or_else(outcomes::Values::new, |value| {
                outcomes::argument_values(env, value)
            });
            let reason = value.and_then(|_| named_keyword(keywords, "reason"));
            outcomes::discard(env, &values, reason, file, expr.range, result);
            return;
        }
    }
    providers::record_call(expr, env, file, result);
    if asyncio::task_result(expr, env, file, contracts, functions, result) {
        return;
    }
    if calls::dispatch_runtime_call(expr, env, file, functions, result) {
        return;
    }
    if let AstExprKind::Call {
        arguments,
        keywords,
        ..
    } = &expr.kind
    {
        if env.ownership.as_ref().is_some_and(|ownership| {
            !ownership.group_names.is_empty()
                || !ownership.task_names.is_empty()
                || !ownership.executor_names.is_empty()
                || ownership.work.has_names()
                || ownership.processes.has_aliases()
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
        let escaped = std::iter::once(receiver)
            .chain(arguments)
            .chain(keywords.iter().map(|(_, argument)| argument))
            .flat_map(|argument| outcomes::argument_values(env, argument))
            .collect();
        outcomes::escape(env, &escaped, file, expr.range, result);
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
                        if env
                            .ownership
                            .as_ref()
                            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
                            && env.exclusive.contains(object)
                            && !env
                                .exclusive_views
                                .get(object)
                                .is_some_and(|views| views.contains(name))
                        {
                            let origin = env.exclusive_origins.get(object).cloned();
                            report(
                                result,
                                "TP2101",
                                "resource is accessed through an alias while an exclusive borrow is active",
                                span(file, expr.range),
                                origin.into_iter().collect(),
                                if uncertain_receiver { "possible" } else { "definite" },
                                contract_identity(env.resources.get(object)),
                            );
                            mask |= UNKNOWN;
                            env.objects.insert(object.clone(), mask);
                            if env.resources.get(object).is_some_and(|resource| {
                                env.protocol_machines.contains_key(&resource.base_symbol)
                            }) {
                                env.protocol_unknown.insert(object.clone());
                                boundary(
                                    result,
                                    file,
                                    expr.range,
                                    "competing alias access makes the protocol state unknown",
                                );
                            }
                        }
                        let resource = env.resources.get(object).cloned();
                        let is_close = resource
                            .as_ref()
                            .and_then(|resource| resource.lifecycle.as_ref())
                            .is_some_and(|lifecycle| lifecycle.closed_by == method);
                        let effect_model = resource.as_ref().and_then(|resource| {
                            if let Some(model) = providers::project_close(resource, env) {
                                return Some(model);
                            }
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
                            .filter(|resource| {
                                !resource
                                    .provenance
                                    .iter()
                                    .any(|entry| entry.origin.starts_with("project-model:"))
                            })
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
                            if resource
                                .as_ref()
                                .is_some_and(|value| value.base_symbol == runtime::FILE_RESOURCE)
                            {
                                runtime::file_close_use(file, expr.range);
                                if !arguments.is_empty() || !keywords.is_empty() {
                                    binding_failure(
                                        BindingFailure::Invalid,
                                        env,
                                        file,
                                        expr.range,
                                        result,
                                    );
                                    return;
                                }
                            }
                            if close_model
                                .as_ref()
                                .is_some_and(|model| model.id.starts_with("project-model:"))
                            {
                                providers::record_close(
                                    resource.as_ref().unwrap(),
                                    env,
                                    file,
                                    expr.range,
                                );
                                if !arguments.is_empty() || !keywords.is_empty() {
                                    binding_failure(
                                        BindingFailure::Invalid,
                                        env,
                                        file,
                                        expr.range,
                                        result,
                                    );
                                    return;
                                }
                            }
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
                            let advanced = env.ownership.as_ref().is_some_and(|ownership| {
                                ownership.mode.includes_advanced_contracts()
                            });
                            let (protocol_entry_states, protocol_entry_unknown) = if advanced
                                && (definition.protocol_transition.is_some()
                                    || definition.protocol_precondition.is_some())
                            {
                                check_protocol_precondition(
                                    definition, object, env, file, expr.range, result,
                                )
                            } else {
                                (BTreeSet::new(), false)
                            };
                            let initial_mask = env.objects.get(object).copied().unwrap_or(mask);
                            let mut child = execute_method_body(
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
                            if let Some(transition) =
                                definition.protocol_transition.as_ref().filter(|_| advanced)
                            {
                                if child.alive {
                                    apply_protocol_completion(
                                        &mut child,
                                        ProtocolCompletionUpdate {
                                            object,
                                            incoming: &protocol_entry_states,
                                            incoming_unknown: protocol_entry_unknown,
                                            transition,
                                            completion: &CompletionKind::Fallthrough,
                                            file,
                                            call_range: expr.range,
                                        },
                                        result,
                                    );
                                }
                                for exit in &mut child.pending_raises {
                                    apply_protocol_completion(
                                        &mut exit.state,
                                        ProtocolCompletionUpdate {
                                            object,
                                            incoming: &protocol_entry_states,
                                            incoming_unknown: protocol_entry_unknown,
                                            transition,
                                            completion: &exit.kind,
                                            file,
                                            call_range: expr.range,
                                        },
                                        result,
                                    );
                                }
                            }
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
                                } else if resource.as_ref().is_some_and(|resource| {
                                    resource
                                        .source
                                        .file
                                        .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
                                }) && (child.escaped.contains(object)
                                    || child
                                        .objects
                                        .get(object)
                                        .is_none_or(|state| state & UNKNOWN != 0))
                                {
                                    *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
                                    boundary(result, file, expr.range, "installed close body does not establish the receiver cleanup effect");
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
                            } else if resource.as_ref().is_some_and(|resource| {
                                resource
                                    .source
                                    .file
                                    .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
                            }) && env.models.foundations.is_some()
                            {
                                *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
                                queue_raise(env, expr.range, None);
                                boundary(
                                    result,
                                    file,
                                    expr.range,
                                    "installed cleanup declaration has no verified source method",
                                );
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
                        } else if runtime::text_file_io(
                            resource.as_ref(),
                            method,
                            arguments,
                            keywords,
                        ) {
                            queue_raise(env, expr.range, None);
                            if mask == CLOSED {
                                env.alive = false;
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
                let ignored = value.map_or_else(outcomes::Values::new, |value| {
                    outcomes::argument_values(env, value)
                });
                outcomes::lose(env, ignored, file, expr.range, result);
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
            if let Some(identity) = external_binding.filter(|identity| {
                matches!(
                    *identity,
                    "external:codepact.Ok"
                        | "external:codepact.Err"
                        | "external:codepact.Some"
                        | "external:codepact.Nothing"
                )
            }) {
                let payload = outcome_constructor_payload(identity, arguments, keywords);
                for argument in arguments
                    .iter()
                    .chain(keywords.iter().map(|(_, argument)| argument))
                {
                    if !expression_contains_tracked_resource(argument, env) {
                        consume_expr(argument, env, file, contracts, functions, result, depth + 1);
                    }
                }
                match payload {
                    Ok(Some(payload)) if expression_contains_tracked_resource(payload, env) => {
                        boundary(
                            result,
                            file,
                            expr.range,
                            "outcome payload ownership is not tracked",
                        );
                    }
                    Ok(_) => {}
                    Err(()) => queue_raise(
                        env,
                        expr.range,
                        Some(AstExpr {
                            range: expr.range,
                            kind: AstExprKind::Name("TypeError".into()),
                        }),
                    ),
                }
                let escaped = env.outcomes.take_value();
                outcomes::escape(env, &escaped, file, expr.range, result);
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
                if let Some(factory) = &definition.modeled_factory {
                    providers::project_factory(factory, &bindings, env, file, expr.range, result);
                    return;
                }
                providers::checked_call(definition, file, expr.range);
                if definition.is_async && !is_awaited_call(expr, file, env) {
                    if env.ownership.is_some() {
                        ownership::check_coroutine_arguments(
                            definition,
                            &bindings,
                            env,
                            &span(file, expr.range),
                            result,
                        );
                        if env
                            .ownership
                            .as_ref()
                            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
                            && bindings.iter().any(|(parameter, argument)| {
                                definition
                                    .parameter_contracts
                                    .get(parameter)
                                    .is_some_and(|contract| contract.access == "exclusive")
                                    && expr_name(argument)
                                        .and_then(|name| env.places.get(name))
                                        .is_some_and(|targets| !targets.is_empty())
                            })
                        {
                            boundary(
                                result,
                                file,
                                expr.range,
                                "exclusive borrow is captured by an unawaited coroutine; its suspension behavior is unresolved",
                            );
                        }
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
                    async_context: definition.is_async || env.async_context,
                    ..State::default()
                };
                child.call_stack.insert(callee.to_string());
                copy_object_metadata(&mut child, env, &env.escaped.iter().cloned().collect());
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
                let call_site = span(file, expr.range);
                let mut owned_entry_objects = BTreeSet::new();
                let mut borrowed_entry_objects = BTreeSet::new();
                let mut bound_accesses = BTreeMap::<String, (String, Span)>::new();
                let mut reported_exclusive_conflicts = BTreeSet::new();
                for (parameter, argument) in bindings {
                    child.outcomes.import_argument(
                        &env.outcomes,
                        &parameter,
                        outcomes::argument_carrier(env, argument),
                    );
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
                                    if env.ownership.as_ref().is_some_and(|ownership| {
                                        ownership.mode.includes_advanced_contracts()
                                    }) {
                                        let access = contract.access.as_str();
                                        let is_exclusive = access == "exclusive";
                                        let previous = bound_accesses.get(object).cloned();
                                        bound_accesses.entry(object.clone()).or_insert_with(|| {
                                            (access.to_owned(), contract.source.clone())
                                        });
                                        let overlaps = previous.as_ref().is_some_and(
                                            |(previous_access, _)| {
                                                is_exclusive || previous_access == "exclusive"
                                            },
                                        );
                                        let active_conflict = env.exclusive.contains(object)
                                            && (!is_exclusive
                                                || !env
                                                    .exclusive_views
                                                    .get(object)
                                                    .is_some_and(|views| views.contains(actual)));
                                        let shared_conflict = is_exclusive
                                            && env.borrowed.contains(object)
                                            && !env.exclusive.contains(object);
                                        if (overlaps || active_conflict || shared_conflict)
                                            && reported_exclusive_conflicts.insert(object.clone())
                                        {
                                            let origin = previous
                                                .as_ref()
                                                .map(|(_, origin)| origin.clone())
                                                .or_else(|| {
                                                    env.exclusive_origins.get(object).cloned()
                                                })
                                                .or_else(|| {
                                                    env.borrowed_origins.get(object).cloned()
                                                });
                                            report(
                                                result,
                                                "TP2101",
                                                "call arguments overlap an active exclusive borrow",
                                                call_site.clone(),
                                                origin.into_iter().collect(),
                                                "definite",
                                                contract_identity(env.resources.get(object)),
                                            );
                                        }
                                        let nested_region = is_exclusive
                                            && env.exclusive.contains(object)
                                            && env
                                                .exclusive_views
                                                .get(object)
                                                .is_some_and(|views| views.contains(actual));
                                        if nested_region {
                                            boundary(
                                                result,
                                                file,
                                                expr.range,
                                                "nested exclusive borrow regions are not modeled",
                                            );
                                            child.protocol_unknown.insert(object.clone());
                                        }
                                        if is_exclusive {
                                            let capture_sites =
                                                ownership::exclusive_capture_sites(env, object);
                                            let active_sites = capture_sites
                                                .iter()
                                                .filter(|(_, _, scheduled)| *scheduled)
                                                .collect::<Vec<_>>();
                                            if !active_sites.is_empty() {
                                                report(
                                                    result,
                                                    "TP2101",
                                                    "exclusive access overlaps a live task capture",
                                                    call_site.clone(),
                                                    active_sites
                                                        .iter()
                                                        .map(|(origin, _, _)| origin.clone())
                                                        .collect(),
                                                    if active_sites
                                                        .iter()
                                                        .any(|(_, possible, _)| !possible)
                                                    {
                                                        "definite"
                                                    } else {
                                                        "possible"
                                                    },
                                                    contract_identity(env.resources.get(object)),
                                                );
                                                child.protocol_unknown.insert(object.clone());
                                            }
                                            if capture_sites
                                                .iter()
                                                .any(|(_, _, scheduled)| !scheduled)
                                            {
                                                boundary(
                                                    result,
                                                    file,
                                                    expr.range,
                                                    "an unresolved coroutine capture may outlive the exclusive region",
                                                );
                                            }
                                            child.borrowed.insert(object.clone());
                                            child
                                                .borrowed_origins
                                                .insert(object.clone(), contract.source.clone());
                                            child.exclusive.insert(object.clone());
                                            child
                                                .exclusive_origins
                                                .insert(object.clone(), contract.source.clone());
                                            child
                                                .exclusive_views
                                                .entry(object.clone())
                                                .or_default()
                                                .insert(parameter.clone());
                                        }
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
                super::guarded_access::inherit(env, &mut child);
                let diagnostic_start = result.diagnostics.len();
                let inherited_outcomes = outcomes::tracked(&child);
                let mut returned =
                    recursion::execute_function(definition, &mut child, functions, result);
                let definition_range = definition
                    .statements
                    .first()
                    .map_or(expr.range, statement_range);
                outcomes::report_scope(
                    &child,
                    &returned,
                    &definition.file,
                    definition_range,
                    result,
                    definition.outcome_return.is_some(),
                    &inherited_outcomes,
                );
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
                                    && providers::result_matches(
                                        definition,
                                        exit.state.resources.get(*object),
                                    )
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
                if let Some(contract) = definition.outcome_return {
                    let symbol = definition
                        .binding_key
                        .as_deref()
                        .and_then(|key| key.rsplit_once(':').map(|(symbol, _)| symbol))
                        .unwrap_or(callee);
                    let delivery = outcomes::Delivery {
                        contract,
                        file,
                        range: expr.range,
                        symbol,
                        identity: outcomes::delivery_identity(env, file, expr.range),
                    };
                    for exit in &mut returned.returns {
                        outcomes::deliver_return(&mut exit.state, &delivery, result);
                    }
                    if child.alive {
                        boundary(
                            result,
                            file,
                            expr.range,
                            "a MustHandle callable can fall through without delivering an outcome",
                        );
                    }
                }
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
                if !definition.publication.is_empty() && !unresolved_owned_return {
                    for object in &owned_return_targets {
                        if let Some(resource) = child.resources.get_mut(object) {
                            providers::mark_source_verified(resource);
                        }
                    }
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
                if !definition.publication.is_empty()
                    && body_diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code == "TP9001")
                {
                    boundary(
                        result,
                        file,
                        expr.range,
                        "installed contract body has unresolved effects on a feasible path",
                    );
                }
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
                env.outcomes.apply_call(&child.outcomes, true);
                if !child.alive {
                    env.alive = false;
                }
            } else {
                if callee != "print"
                    || env.shadowed_functions.contains(callee)
                    || env.function_bindings.contains_key(callee)
                {
                    let escaped = arguments
                        .iter()
                        .chain(keywords.iter().map(|(_, argument)| argument))
                        .flat_map(|argument| outcomes::argument_values(env, argument))
                        .collect();
                    outcomes::escape(env, &escaped, file, expr.range, result);
                }
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
                if let Some(name) = expr_name(item) {
                    report_possibly_unbound_alias(name, item.range, env, file, result);
                } else {
                    consume_expr(item, env, file, contracts, functions, result, depth + 1);
                }
                if !env.alive {
                    return;
                }
                let mut outcomes = outcomes::escaping_expression(env, item);
                outcomes.append(&mut env.outcomes.take_value());
                self::outcomes::escape(env, &outcomes, file, item.range, result);
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
                let escaped = env.outcomes.take_value();
                outcomes::escape(env, &escaped, file, item.range, result);
                if !env.alive {
                    break;
                }
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
            if matches!(
                &expr.kind,
                AstExprKind::Collection(_) | AstExprKind::Closure(_)
            ) {
                let captured = outcomes::escaping_expression(env, expr);
                outcomes::escape(env, &captured, file, expr.range, result);
            }
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

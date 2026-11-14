use super::*;

pub(super) fn execute(
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

pub(super) fn join_states(mut states: Vec<State>) -> Option<State> {
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
pub(super) enum ExceptionMatch {
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

pub(super) fn raised_exception_name(exit: &FlowExit) -> Option<&str> {
    completion_exception_name(&exit.kind, &exit.state)
}

pub(super) fn completion_exception_name<'a>(
    kind: &'a CompletionKind,
    state: &State,
) -> Option<&'a str> {
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

pub(super) fn builtin_exception_match(exception: &str, caught: &str) -> Option<bool> {
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

pub(super) fn is_unshadowed_builtin_exception(name: &str, state: &State) -> bool {
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

pub(super) fn exception_handler_match(
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
    state.definitely_bound_names.remove(name);
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
            let exception_target_was_possibly_unbound = handler
                .name
                .as_ref()
                .is_some_and(|name| dispatch_state.possibly_unbound_places.contains(name));
            if let Some(name) = &handler.name {
                handler_state.possibly_unbound_places.remove(name);
                handler_state.definitely_bound_names.insert(name.clone());
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
                if exception_target_was_possibly_unbound {
                    handler_state.possibly_unbound_places.insert(name.clone());
                    for exit in handler_outcomes
                        .returns
                        .iter_mut()
                        .chain(&mut handler_outcomes.raises)
                        .chain(&mut handler_outcomes.breaks)
                        .chain(&mut handler_outcomes.continues)
                    {
                        exit.state.possibly_unbound_places.insert(name.clone());
                    }
                }
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
            iteration.definitely_bound_names.insert(target.to_owned());
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
            iteration.definitely_bound_names.insert(target.to_owned());
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
            iteration.definitely_bound_names.insert(target.to_owned());
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
                env.possibly_unbound_places.remove(target);
                env.definitely_bound_names.insert(target.clone());
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
                env.definitely_bound_names.insert(target.to_owned());
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
                env.definitely_bound_names.insert(name.clone());
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
                            env.definitely_bound_names.insert(name.clone());
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
                            env.definitely_bound_names.insert(name.clone());
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
                        env.definitely_bound_names.insert(name.clone());
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
                env.definitely_bound_names.insert(name.clone());
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
                env.definitely_bound_names.insert(name.clone());
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
                env.definitely_bound_names.insert(name.clone());
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
            env.definitely_bound_names.insert(name.clone());
            env.possibly_unbound_places.remove(name);
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
            env.definitely_bound_names.insert(name.clone());
            env.possibly_unbound_places.remove(name);
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
        FlowStatement::DeleteLocal { names, range } if env.ownership.is_some() => {
            for name in names {
                report_possibly_unbound_alias(name, *range, env, file, result);
                env.possibly_unbound_places.remove(name);
                env.definitely_bound_names.remove(name);
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

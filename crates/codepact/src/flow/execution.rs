use super::contexts::execute_with;
use super::*;

#[allow(clippy::too_many_arguments)]
fn execute_match_statement(
    subject: &AstExpr,
    cases: &[crate::frontend::FlowMatchCase],
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> FlowResult {
    let mut outcomes = FlowResult::default();
    consume_expr(subject, env, file, contracts, functions, result, 0);
    let subject_value = env.outcomes.take_carrier();
    let subject_outcomes = &subject_value.ids;
    let subject_fully_tracked = !subject_value.may_be_untracked();
    outcomes.raises.extend(take_pending_raises(env));
    if !env.alive {
        return outcomes;
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
            range,
            "control-flow nesting budget exceeded; tracked resources are unknown",
        );
        return outcomes;
    }
    let original_control_depth = env.control_depth;
    let mut next_case = Some(Box::new(env.clone()));
    let mut fallthrough = None;
    for case in cases {
        let Some(mut incoming) = next_case.take() else {
            break;
        };
        if !take_flow_step(&mut incoming, file, case.range, result) {
            next_case = Some(incoming);
            break;
        }

        let irrefutable = pattern_is_irrefutable(&case.pattern);
        let mut branch = Box::new(incoming.as_ref().clone());
        let pattern = if subject_outcomes.is_empty() {
            None
        } else {
            let pattern = outcomes::select_pattern(&case.pattern, &branch);
            if pattern.is_none() {
                boundary(
                    result,
                    file,
                    case.range,
                    "persistent outcome pattern is not resolved",
                );
            }
            pattern
        };
        let mut matches_all = irrefutable;
        if let Some((mask, _)) = pattern {
            if outcomes::narrow(&mut branch, subject_outcomes, mask) == Some(0)
                && subject_fully_tracked
            {
                next_case = Some(incoming);
                continue;
            }
            matches_all |= outcomes::narrow(&mut incoming, subject_outcomes, !mask) == Some(0)
                && subject_fully_tracked;
        }
        let mut captures = BTreeSet::new();
        case.pattern.binding_names(&mut captures);
        for name in &captures {
            rebind_match_capture(&mut branch, name);
        }
        if pattern.is_some() {
            outcomes::bind_capture(&case.pattern, &subject_value, &mut branch);
        }
        if !matches_all && pattern.is_none() && !captures.is_empty() {
            // A failed pattern can leave partial bindings in place.
            incoming = merge_match_states(Some(incoming), branch.clone(), file, case.range, result);
        }
        next_case = (!matches_all).then_some(incoming);

        branch.control_depth = original_control_depth.saturating_add(1);
        if let Some(guard) = &case.guard {
            consume_expr(guard, &mut branch, file, contracts, functions, result, 0);
            outcomes.raises.extend(take_pending_raises(&mut branch));
            if branch.alive && !matches!(guard.kind, AstExprKind::Boolean(true)) {
                let mut guard_failed = Box::new(branch.as_ref().clone());
                guard_failed.control_depth = original_control_depth;
                next_case = Some(merge_match_states(
                    next_case,
                    guard_failed,
                    file,
                    case.range,
                    result,
                ));
            }
            if matches!(guard.kind, AstExprKind::Boolean(false)) {
                continue;
            }
        }
        if branch.alive {
            if let Some((matched, wildcard)) = pattern {
                if let Some(evidence) =
                    outcomes::acknowledge(&mut branch, subject_outcomes, matched, wildcard)
                {
                    evidence.report(file, case.pattern.range(), result);
                }
            }
            outcomes.append(execute(
                &case.body,
                &mut branch,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            ));
            if branch.alive {
                fallthrough = Some(merge_match_states(
                    fallthrough,
                    branch,
                    file,
                    case.range,
                    result,
                ));
            }
        }
    }
    if let Some(unmatched) = next_case {
        fallthrough = Some(merge_match_states(
            fallthrough,
            unmatched,
            file,
            range,
            result,
        ));
    }
    if let Some(mut joined) = fallthrough {
        joined.alive = true;
        joined.control_depth = original_control_depth;
        *env = *joined;
        outcomes::lose(env, subject_value.ids, file, range, result);
        ownership::cap_handles(env, file, range, result);
    } else {
        env.alive = false;
    }
    outcomes
}

fn rebind_match_capture(state: &mut State, name: &str) {
    clear_local_binding(state, name);
    for views in state.exclusive_views.values_mut() {
        views.remove(name);
    }
    state.definitely_bound_names.insert(name.to_owned());
    state.unknown_places.insert(name.to_owned());
}

fn merge_match_states(
    previous: Option<Box<State>>,
    incoming: Box<State>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> Box<State> {
    let Some(previous) = previous else {
        return incoming;
    };
    let (joined, effects) = merge(&previous, &incoming);
    for place in effects.aliases_over_budget {
        boundary(
            result,
            file,
            range,
            &format!(
                "alias target budget exceeded for {place}; affected resource states are unknown"
            ),
        );
    }
    Box::new(joined)
}

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
        if let FlowStatement::Match {
            subject,
            cases,
            range,
        } = statement
        {
            outcomes.append(execute_match_statement(
                subject, cases, *range, env, file, contracts, functions, result, depth,
            ));
            continue;
        }
        if let FlowStatement::If {
            test,
            body,
            alternate,
            range,
        } = statement
        {
            let termination = process::termination_test(test, env, file, result);
            let work_test = work::test(test, env, file, result);
            let protocol_test = process::protocol_test(test, env, file, result);
            let main_guard = process::is_main_guard(test, env, file);
            if termination.is_none()
                && work_test.is_none()
                && protocol_test.is_none()
                && !main_guard
            {
                consume_expr(test, env, file, contracts, functions, result, 0);
            }
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
            let known_test = match &test.kind {
                AstExprKind::Boolean(value) => Some(*value),
                _ => async_protocols::test(test, env),
            };
            if let Some(value) = known_test {
                let branch = if value { body } else { alternate };
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
            if let Some(test) = &termination {
                process::refine_test(&mut left, test, true);
                process::refine_test(&mut right, test, false);
            }
            if let Some(test) = &work_test {
                work::refine(&mut left, test, true);
                work::refine(&mut right, test, false);
            }
            if let Some(test) = &protocol_test {
                process::refine_protocol_test(&mut left, test, true);
                process::refine_protocol_test(&mut right, test, false);
            }
            process::refine_main_guard(&mut left, main_guard);
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
                    asynchronous,
                    test,
                    iterator,
                    target,
                    body,
                    alternate,
                    range,
                } => {
                    if *asynchronous {
                        outcomes.append(execute_async_for(
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
                        ));
                        continue;
                    }
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
                    let returned =
                        execute_with_outcomes(statement, env, file, contracts, functions, result);
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
                            state: *returned,
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
                            CompletionKind::Cancelled(_) => outcomes.raises.push(exit),
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

fn execute_with_outcomes(
    statement: &FlowStatement,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<Box<State>> {
    env.outcomes.clear_value();
    let assignment_outcome = match statement {
        FlowStatement::Assign { value, .. } | FlowStatement::EscapingAssign { value, .. } => {
            expr_name(value).map(|name| env.outcomes.read_carrier(name))
        }
        _ => None,
    };
    match statement {
        FlowStatement::Assign {
            annotation: Some(annotation),
            ..
        }
        | FlowStatement::EscapingAssign {
            annotation: Some(annotation),
            ..
        }
        | FlowStatement::Store {
            annotation: Some(annotation),
            ..
        }
        | FlowStatement::AnnotatedDeclaration { annotation, .. } => {
            outcomes::local_annotation(annotation, env, result, file);
        }
        _ => {}
    }
    let returned = execute_non_if_statement(statement, env, file, contracts, functions, result);
    if env.alive {
        match statement {
            FlowStatement::Assign { name, range, .. }
            | FlowStatement::EscapingAssign { name, range, .. } => {
                guarded_access::check_binding(name, env, file, *range, functions, result);
                let value = assignment_outcome.unwrap_or_else(|| env.outcomes.take_carrier());
                if matches!(statement, FlowStatement::EscapingAssign { .. }) {
                    outcomes::escape(env, &value.ids, file, *range, result);
                }
                let previous = env.outcomes.bind_carrier(name, value);
                outcomes::lose(env, previous, file, *range, result);
            }
            FlowStatement::Expression { range, .. } => {
                let value = env.outcomes.take_value();
                outcomes::lose(env, value, file, *range, result);
            }
            FlowStatement::BindFunction { name, range, .. }
            | FlowStatement::ClosureDefinition { name, range, .. } => {
                if let FlowStatement::ClosureDefinition { captures, .. } = statement {
                    let escaped = captures
                        .iter()
                        .flat_map(|name| env.outcomes.read(name))
                        .collect();
                    outcomes::escape(env, &escaped, file, *range, result);
                }
                let previous = env.outcomes.bind(name, outcomes::Values::new());
                outcomes::lose(env, previous, file, *range, result);
            }
            FlowStatement::DeleteLocal { names, range } => {
                for name in names {
                    let previous = env.outcomes.bind(name, outcomes::Values::new());
                    outcomes::lose(env, previous, file, *range, result);
                }
            }
            FlowStatement::Store { value, range, .. } => {
                let escaped = outcomes::escaping_expression(env, value);
                outcomes::escape(env, &escaped, file, *range, result);
            }
            _ => {}
        }
    }
    returned.map(Box::new)
}

fn pattern_is_irrefutable(pattern: &crate::frontend::AstPattern) -> bool {
    use crate::frontend::AstPattern as Pattern;
    match pattern {
        Pattern::Capture { .. } => true,
        Pattern::As { pattern: None, .. } => true,
        Pattern::As {
            pattern: Some(pattern),
            ..
        } => pattern_is_irrefutable(pattern),
        Pattern::Or { patterns, .. } => patterns.iter().any(pattern_is_irrefutable),
        Pattern::Class { .. } | Pattern::Unsupported { .. } => false,
    }
}

const MAX_LOOP_ITERATIONS: usize = 16;

pub(super) fn push_exit(outcomes: &mut FlowResult, exit: FlowExit) {
    match &exit.kind {
        CompletionKind::Fallthrough => unreachable!("fallthrough is reported at function exit"),
        CompletionKind::Return(_) => outcomes.returns.push(exit),
        CompletionKind::Raise(_) => outcomes.raises.push(exit),
        CompletionKind::Cancelled(_) => outcomes.raises.push(exit),
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
    let pending_outcome_return = exit.state.outcomes.returned.clone();
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
            state.outcomes.set_return(pending_outcome_return);
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

fn resolved_exception_name<'a>(expression: &'a AstExpr, state: &State) -> Option<&'a str> {
    match ownership::external_identity(expression, state).as_deref() {
        Some("external:asyncio.CancelledError") => return Some("CancelledError"),
        Some("external:asyncio.InvalidStateError") => return Some("asyncio.InvalidStateError"),
        Some("external:asyncio.QueueEmpty") => return Some("asyncio.QueueEmpty"),
        Some("external:asyncio.QueueFull") => return Some("asyncio.QueueFull"),
        Some("external:asyncio.QueueShutDown") => return Some("asyncio.QueueShutDown"),
        Some("external:queue.Empty") => return Some("queue.Empty"),
        Some("external:queue.Full") => return Some("queue.Full"),
        Some("external:concurrent.futures.CancelledError") => {
            return Some("concurrent.futures.CancelledError")
        }
        Some("external:concurrent.futures.TimeoutError") => return Some("TimeoutError"),
        Some("external:multiprocessing.TimeoutError") => {
            return Some("multiprocessing.TimeoutError")
        }
        _ => {}
    }
    if let Some(name) = expr_name(expression).and_then(|name| {
        name.strip_prefix(":work-error:")
            .or_else(|| name.strip_prefix(":exception:"))
    }) {
        return known_exception_for_match(name).then_some(name);
    }
    expr_name(expression).filter(|name| is_unshadowed_exception_handler_name(name, state))
}

fn expression_exception_names<'a>(
    expression: &'a AstExpr,
    state: &State,
) -> Option<Vec<Option<&'a str>>> {
    match &expression.kind {
        AstExprKind::Name(_) | AstExprKind::Attribute(..) => {
            Some(vec![resolved_exception_name(expression, state)])
        }
        AstExprKind::Tuple(items) => items
            .iter()
            .map(|item| expression_exception_names(item, state))
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
    if matches!(kind, CompletionKind::Cancelled(_)) {
        return Some("CancelledError");
    }
    let CompletionKind::Raise(Some(expression)) = kind else {
        return None;
    };
    match &expression.kind {
        AstExprKind::Call { function, .. } => resolved_exception_name(function, state),
        _ => resolved_exception_name(expression, state),
    }
}

fn known_builtin_exception(name: &str) -> bool {
    crate::frontend::is_known_builtin_exception_name(name)
}

fn known_exception_for_match(name: &str) -> bool {
    known_builtin_exception(name)
        || matches!(
            name,
            "CancelledError"
                | "asyncio.InvalidStateError"
                | "asyncio.QueueEmpty"
                | "asyncio.QueueFull"
                | "asyncio.QueueShutDown"
                | "queue.Empty"
                | "queue.Full"
                | "concurrent.futures.CancelledError"
                | "multiprocessing.TimeoutError"
        )
}

fn builtin_exception_parents(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "CancelledError" => &["BaseException"],
        "asyncio.InvalidStateError"
        | "asyncio.QueueEmpty"
        | "asyncio.QueueFull"
        | "asyncio.QueueShutDown"
        | "queue.Empty"
        | "queue.Full"
        | "concurrent.futures.CancelledError"
        | "multiprocessing.TimeoutError" => &["Exception"],
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
    let Some(caught) = expression_exception_names(handler, state) else {
        return ExceptionMatch::Possible;
    };
    let mut possible = false;
    for caught in caught {
        if caught == Some("BaseException") {
            return ExceptionMatch::Definite;
        }
        let matching = exception
            .zip(caught)
            .and_then(|(exception, caught)| builtin_exception_match(exception, caught));
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

fn clear_local_binding(state: &mut State, name: &str) {
    state.outcomes.bind(name, outcomes::Values::new());
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
        clear_local_binding(&mut exit.state, name);
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
                if expression_exception_names(exception_type, &dispatch_state)
                    .is_none_or(|names| names.iter().any(Option::is_none))
                {
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
                clear_local_binding(&mut handler_state, name);
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
fn execute_async_for(
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
    fn orders(names: &[String]) -> Vec<Vec<String>> {
        if names.is_empty() {
            return vec![Vec::new()];
        }
        let mut permutations = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let remaining = names
                .iter()
                .enumerate()
                .filter(|(position, _)| *position != index)
                .map(|(_, name)| name.clone())
                .collect::<Vec<_>>();
            for mut order in orders(&remaining) {
                order.insert(0, name.clone());
                permutations.push(order);
            }
        }
        permutations
    }

    let mut outcomes = FlowResult::default();
    let applicable = iterator.zip(target).and_then(|(iterator, target)| {
        let members = asyncio::as_completed_members(iterator, env)?;
        (depth < MAX_CONTROL_FLOW_DEPTH && env.control_depth < MAX_CONTROL_FLOW_DEPTH)
            .then_some((target, members))
    });
    let Some((target, members)) = applicable else {
        execute_non_if_statement(
            &FlowStatement::Unsupported { range },
            env,
            file,
            contracts,
            functions,
            result,
        );
        outcomes.raises.extend(take_pending_raises(env));
        return outcomes;
    };
    runtime::use_model(
        "asyncio.as_completed",
        "iterate-known-members",
        "TPM-ASYNCIO-001",
        Ok(()),
        file,
        range,
        result,
    );
    let captured = members
        .iter()
        .enumerate()
        .map(|(index, member)| {
            let alias = synthetic_name(env, "as-completed-member", range, index);
            ownership::assign_handle(&alias, Some(member), false, env, file, range, result);
            alias
        })
        .collect::<Vec<_>>();
    let mut normal = Vec::new();
    for order in orders(&captured) {
        let mut path = env.clone();
        for member in order {
            if !path.alive {
                break;
            }
            let mut interrupted = path.clone();
            interrupted.pending_raises.clear();
            outcomes.raises.push(FlowExit {
                state: interrupted,
                range,
                kind: CompletionKind::Cancelled(None),
            });
            asyncio::deliver_member(
                &member,
                asyncio::Delivery::WaitOnly,
                range,
                &mut path,
                file,
                contracts,
                functions,
                result,
            );
            outcomes.raises.extend(take_pending_raises(&mut path));
            if !path.alive {
                break;
            }
            path.bind_runtime_handle(target);
            ownership::assign_handle(target, Some(&member), false, &mut path, file, range, result);
            let old = path.outcomes.bind(target, outcomes::Values::new());
            outcomes::lose(&mut path, old, file, range, result);
            let mut iteration = execute(
                body,
                &mut path,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            );
            normal.extend(iteration.breaks.drain(..).map(|exit| exit.state));
            let mut continuing = iteration
                .continues
                .drain(..)
                .map(|exit| exit.state)
                .collect::<Vec<_>>();
            if path.alive {
                continuing.push(path.clone());
            }
            outcomes.append(iteration);
            if let Some(next) = join_states(continuing) {
                path = next;
            } else {
                path.alive = false;
            }
        }
        if path.alive {
            outcomes.append(execute(
                alternate,
                &mut path,
                file,
                contracts,
                functions,
                result,
                depth + 1,
            ));
            if path.alive {
                normal.push(path);
            }
        }
    }
    if let Some(joined) = join_states(normal) {
        *env = joined;
    } else {
        env.alive = false;
    }
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

pub(super) fn execute_non_if_statement(
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
            if process::initialize(name, value, env, file, functions, result) {
                return None;
            }
            if executor::initialize_handle(name, value, env, file, result) {
                return None;
            }
            if tasks::initialize_group_handle(name, value, env, file, result) {
                return None;
            }
            if let Some(source) = expr_name(value) {
                report_possibly_unbound_alias(source, value.range, env, file, result);
            }
            env.pending_return_targets = None;
            if let Some(ownership) = env.ownership.as_mut() {
                ownership.pending_task = None;
                ownership.work.clear_pending();
                ownership.pending_move = false;
            }
            let source_coroutine = expr_name(value)
                .and_then(|source| env.coroutine_aliases.get(source))
                .cloned();
            let source_exclusive_objects = expr_name(value)
                .into_iter()
                .filter(|source| {
                    env.exclusive_views
                        .values()
                        .any(|views| views.contains(*source))
                })
                .flat_map(|source| {
                    env.places
                        .get(source)
                        .into_iter()
                        .flatten()
                        .filter(|object| env.exclusive.contains(*object))
                        .cloned()
                })
                .collect::<BTreeSet<_>>();
            for views in env.exclusive_views.values_mut() {
                views.remove(name);
            }
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
                .and_then(|_| ownership::external_identity(value, env))
                .or_else(|| {
                    providers::public_name(value, env)
                        .filter(|name| {
                            env.models
                                .foundations
                                .as_ref()
                                .is_some_and(|inputs| inputs.module(name).is_some())
                        })
                        .map(|name| format!("provider:{name}"))
                });
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
                            Some(calls::prepare_call(
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
                        if resource_contract
                            .as_ref()
                            .is_some_and(|resource| resource.lifecycle.is_some())
                        {
                            env.duty_origins.insert(object.clone(), span(file, *range));
                        }
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
                                        exit.state.protocol_states.remove(&object);
                                        exit.state.protocol_unknown.remove(&object);
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
                        if let Some(initial) = env
                            .alive
                            .then(|| {
                                env.resources
                                    .get(&object)
                                    .and_then(|resource| {
                                        env.protocol_machines.get(&resource.base_symbol)
                                    })
                                    .map(|machine| machine.initial.clone())
                            })
                            .flatten()
                        {
                            if repeated_allocation {
                                env.protocol_states
                                    .entry(object.clone())
                                    .or_default()
                                    .insert(initial);
                                env.protocol_unknown.insert(object.clone());
                            } else {
                                env.protocol_states
                                    .insert(object.clone(), BTreeSet::from([initial]));
                                env.protocol_unknown.remove(&object);
                            }
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
                env.shadowed_classes.insert(name.clone());
                env.class_bindings.remove(name);
                env.shadowed_functions.insert(name.clone());
                if env
                    .ownership
                    .as_ref()
                    .is_some_and(|ownership| ownership.work.has_pending())
                {
                    env.unknown_places.remove(name);
                    env.coroutine_aliases.remove(name);
                    env.unknown_coroutine_aliases.remove(name);
                } else {
                    env.unknown_places.insert(name.clone());
                    boundary(
                        result,
                        file,
                        *range,
                        "resource-valued call result is not tracked",
                    );
                }
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
                for object in source_exclusive_objects {
                    env.exclusive_views
                        .entry(object)
                        .or_default()
                        .insert(name.clone());
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
            if let Some(ownership) = &mut env.ownership {
                ownership.work.clear_pending();
            }
        }
        FlowStatement::Pass { .. } | FlowStatement::AnnotatedDeclaration { .. } => {}
        FlowStatement::Return { value, .. } => {
            env.pending_return_targets = None;
            env.outcomes.set_return(outcomes::Carrier::default());
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
                    work::escape_pending(env, file, value.range, result);
                }
                let returned_value = match expr_name(value) {
                    Some(name) => env.outcomes.read_carrier(name),
                    None => env.outcomes.take_carrier(),
                };
                env.outcomes.set_return(returned_value);
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
                    let resource = env
                        .module_class_bindings
                        .get(name)
                        .cloned()
                        .flatten()
                        .or_else(|| {
                            class_resource_contract(file, name, class_decorators, contracts)
                        });
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
            ..
        } if env.ownership.is_some() => {
            if process::initialize_pipe(value, target.as_ref(), env, file, result) {
                return None;
            }
            if asyncio::is_gather(value, env) {
                if let (
                    AstExprKind::Await(awaited),
                    Some(AstExpr {
                        kind: AstExprKind::Tuple(targets) | AstExprKind::Collection(targets),
                        ..
                    }),
                ) = (&value.kind, target)
                {
                    if let Some(names) = targets
                        .iter()
                        .map(|target| expr_name(target).map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                    {
                        asyncio::await_combination(
                            awaited,
                            value.range,
                            Some(&names),
                            env,
                            file,
                            contracts,
                            functions,
                            result,
                        );
                        return None;
                    }
                }
            }
            let names = ownership::stored_names(value);
            if expr_name(value).is_none() {
                consume_expr(value, env, file, contracts, functions, result, 0);
            }
            if !env.alive {
                return None;
            }
            let mut escaped = outcomes::escaping_expression(env, value);
            escaped.append(&mut env.outcomes.take_value());
            outcomes::escape(env, &escaped, file, *range, result);
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
                if env
                    .ownership
                    .as_ref()
                    .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
                {
                    if let Some(alias) = root_place_name(target) {
                        report_exclusive_alias_access(
                            env,
                            alias,
                            file,
                            *range,
                            result,
                            "resource is mutated through an alias while an exclusive borrow is active",
                        );
                    }
                }
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
        FlowStatement::Store { value, range, .. }
            if outcomes::call_contract(value, env, functions).is_some() =>
        {
            consume_expr(value, env, file, contracts, functions, result, 0);
            let escaped = env.outcomes.take_value();
            outcomes::escape(env, &escaped, file, *range, result);
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
        FlowStatement::DeleteLocal { names, .. }
            if names.iter().all(|name| {
                !env.outcomes.read(name).is_empty() && !env.places.contains_key(name)
            }) =>
        {
            for name in names {
                env.definitely_bound_names.remove(name);
                env.unknown_places.remove(name);
                env.possibly_unbound_places.remove(name);
                env.function_bindings.remove(name);
                env.class_bindings.remove(name);
                env.shadowed_functions.insert(name.clone());
                env.shadowed_classes.insert(name.clone());
            }
        }
        FlowStatement::AugmentedStore {
            target,
            value,
            range,
        } if process::compound_store(target, value, *range, env, file, result) => {}
        FlowStatement::Unsupported { range }
        | FlowStatement::AugmentedStore { range, .. }
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
        | FlowStatement::Match { .. }
        | FlowStatement::Try { .. }
        | FlowStatement::Loop { .. }
        | FlowStatement::With { .. } => {
            unreachable!("compound statements use the stack-bounded dispatcher")
        }
    }
    None
}

//! Qualified asyncio bridges. The wrapper owns a wait, while the work ledger
//! owns completion, result delivery and resource captures.
use super::super::*;
use super::{ExecutorState, EXECUTOR_ACCEPTING};

const TO_THREAD: &str = "external:asyncio.to_thread";
const WRAP_FUTURE: &str = "external:asyncio.wrap_future";
const COROUTINE_THREADSAFE: &str = "external:asyncio.run_coroutine_threadsafe";
const DEFAULT_EXECUTOR: &str = ":asyncio-default-executor";

pub(in crate::flow) fn profile(env: &State) -> Result<(), &'static str> {
    let ownership = env
        .ownership
        .as_ref()
        .filter(|ownership| ownership.mode.includes_advanced_contracts())
        .ok_or("executor bridges require advanced ownership checks")?;
    runtime::runtime_profile(env)?.asyncio(
        &env.target_python_version,
        ownership.mode.assumes_default_task_factory() && !ownership.incompatible_task_factory,
    )
}

fn known_loop(receiver: &AstExpr, env: &State) -> bool {
    expr_name(receiver).is_some_and(|name| {
        env.ownership
            .as_ref()
            .is_some_and(|ownership| ownership.work.running_loop(name))
    }) || call_parts(receiver).is_some_and(|(callee, arguments, keywords)| {
        arguments.is_empty()
            && keywords.is_empty()
            && ownership::external_identity(callee, env).as_deref()
                == Some("external:asyncio.get_running_loop")
    })
}

fn record(
    symbol: &str,
    effect: &str,
    applies: Result<(), &str>,
    expression: &AstExpr,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    runtime::use_model(
        symbol,
        effect,
        work::BRIDGE_MODEL,
        applies,
        file,
        expression.range,
        result,
    )
}

fn running_loop_profile(env: &State) -> Result<(), &'static str> {
    profile(env)?;
    env.async_context
        .then_some(())
        .ok_or("the bridge requires an established running asyncio loop")
}

pub(in crate::flow) fn scheduling_call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let Some((callee, [source], [])) = call_parts(expression) else {
        return false;
    };
    if ownership::external_identity(callee, env).as_deref() != Some("external:asyncio.create_task")
        || !expr_name(source).is_some_and(|name| work::is_deferred(env, name))
    {
        return false;
    }
    let applies = running_loop_profile(env).and_then(|()| {
        work::schedule_bridge(env, expr_name(source).unwrap(), file, expression.range)
            .then_some(())
            .ok_or("bridge coroutine was already consumed or the wrapper budget is exhausted")
    });
    if !record(
        "asyncio.create_task",
        "schedule-executor-bridge",
        applies,
        expression,
        file,
        result,
    ) {
        queue_raise(env, expression.range, None);
    }
    true
}

pub(in crate::flow) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if env.models.foundations.is_none() {
        return false;
    }
    let Some((callee, arguments, keywords)) = call_parts(expression) else {
        return false;
    };
    match ownership::external_identity(callee, env).as_deref() {
        Some(TO_THREAD) => {
            let captures = profile(env).and_then(|()| {
                super::thread_worker(arguments, keywords, env, functions, true)
                    .ok_or("to_thread requires a resolved synchronous worker and bounded captures")
            });
            if record(
                "asyncio.to_thread",
                "create-coroutine",
                captures.as_ref().map(|_| ()).map_err(|reason| *reason),
                expression,
                file,
                result,
            ) {
                let outcome = worker_outcome(arguments, keywords, env, functions, result);
                work::register(
                    env,
                    None,
                    work::Kind::Thread(DEFAULT_EXECUTOR.into()),
                    file,
                    expression.range,
                    outcome,
                    result,
                );
                work::bind_worker_contract(env, &arguments[0], functions);
                work::capture_pending_bridge(env, captures.expect("applicable bridge"), true);
            } else {
                ownership::argument_escapes(
                    arguments,
                    keywords,
                    env,
                    file,
                    expression.range,
                    result,
                );
                queue_raise(env, expression.range, None);
            }
            true
        }
        Some(WRAP_FUTURE) => {
            let applies = running_loop_profile(env).and_then(|()| {
                let [future] = arguments else {
                    return Err("wrap_future requires one resolved concurrent future");
                };
                if !matches!(keywords, [] | [(Some(_), _)])
                    || keywords.first().is_some_and(|(name, loop_value)| {
                        name.as_deref() != Some("loop")
                            || !(known_loop(loop_value, env)
                                || matches!(loop_value.kind, AstExprKind::NoneLiteral))
                    })
                {
                    return Err("wrap_future requires the current resolved asyncio loop");
                }
                let Some(future) = expr_name(future) else {
                    return Err("wrap_future requires one resolved concurrent future");
                };
                work::wrap_executor(env, Some(future), file, expression.range)
                    .then_some(())
                    .ok_or("wrap_future source identity or wrapper allocation is unresolved")
            });
            if !record(
                "asyncio.wrap_future",
                "link-existing-work",
                applies,
                expression,
                file,
                result,
            ) {
                queue_raise(env, expression.range, None);
            }
            true
        }
        Some(COROUTINE_THREADSAFE) => cross_thread(
            expression, arguments, keywords, env, file, functions, result,
        ),
        _ => {
            let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
                return false;
            };
            match method {
                "run_in_executor" if known_loop(receiver, env) => {
                    let applies = running_loop_profile(env).and_then(|()| {
                        if arguments.len() < 2 || !keywords.is_empty() {
                            return Err("run_in_executor requires an exact executor and positional worker arguments");
                        }
                        let name = expr_name(&arguments[0]);
                        if !matches!(arguments[0].kind, AstExprKind::NoneLiteral)
                            && !name.is_some_and(|name| env.ownership.as_ref().unwrap().executor_names.contains_key(name))
                        {
                            return Err("run_in_executor executor is unresolved");
                        }
                        Ok(())
                    });
                    if record(
                        "asyncio.AbstractEventLoop.run_in_executor",
                        "dispatch",
                        applies,
                        expression,
                        file,
                        result,
                    ) {
                        run_in_executor(expression, arguments, env, file, functions, result);
                    } else {
                        ownership::argument_escapes(
                            arguments,
                            keywords,
                            env,
                            file,
                            expression.range,
                            result,
                        );
                        queue_raise(env, expression.range, None);
                    }
                    true
                }
                "set_default_executor" if known_loop(receiver, env) => {
                    env.ownership.as_mut().unwrap().incompatible_task_factory = true;
                    record(
                        "asyncio.AbstractEventLoop.set_default_executor",
                        "replace-default-executor",
                        Err("default executor replacement invalidates executor bridge applicability"),
                        expression,
                        file,
                        result,
                    );
                    queue_raise(env, expression.range, None);
                    true
                }
                _ => false,
            }
        }
    }
}

fn run_in_executor(
    expression: &AstExpr,
    arguments: &[AstExpr],
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let mut default = false;
    let executor = if let Some(name) = expr_name(&arguments[0]) {
        name
    } else {
        default = true;
        let ownership = env.ownership.as_mut().unwrap();
        ownership
            .executor_names
            .insert(DEFAULT_EXECUTOR.into(), DEFAULT_EXECUTOR.into());
        ownership
            .executors
            .entry(DEFAULT_EXECUTOR.into())
            .or_insert_with(|| ExecutorState {
                states: EXECUTOR_ACCEPTING,
                origin: span(file, expression.range),
                has_submissions: false,
                manager_detached: false,
                process: false,
            });
        DEFAULT_EXECUTOR
    };
    let range = expression.range;
    let submission = AstExpr {
        range,
        kind: AstExprKind::Call {
            function: Box::new(AstExpr {
                range,
                kind: AstExprKind::Attribute(
                    Box::new(AstExpr {
                        range,
                        kind: AstExprKind::Name(executor.into()),
                    }),
                    "submit".into(),
                ),
            }),
            arguments: arguments[1..].to_vec(),
            keywords: Vec::new(),
        },
    };
    super::call(&submission, env, file, functions, result);
    if default {
        env.ownership
            .as_mut()
            .unwrap()
            .executor_names
            .remove(DEFAULT_EXECUTOR);
    }
    if env.alive && !work::wrap_executor(env, None, file, range) {
        boundary(
            result,
            file,
            range,
            "run_in_executor did not establish a uniquely tracked wrapper",
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow) fn await_bridge(
    awaited: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> bool {
    if let Some(name) = expr_name(awaited) {
        if work::is_deferred(env, name) {
            if let Err(reason) = running_loop_profile(env) {
                runtime::use_model(
                    "asyncio.to_thread",
                    "dispatch-and-await",
                    work::BRIDGE_MODEL,
                    Err(reason),
                    file,
                    range,
                    result,
                );
                queue_raise(env, range, None);
                return true;
            }
        }
        return work::await_bridge(env, Some(name), file, range, result);
    }
    if env.models.foundations.is_none() {
        return false;
    }
    let Some((callee, _, _)) = call_parts(awaited) else {
        return false;
    };
    let bridge = matches!(
        ownership::external_identity(callee, env).as_deref(),
        Some(TO_THREAD | WRAP_FUTURE)
    ) || called_method(awaited).is_some_and(|(receiver, method, _, _)| {
        method == "run_in_executor" && known_loop(receiver, env)
    });
    if !bridge {
        return false;
    }
    consume_expr(awaited, env, file, contracts, functions, result, depth + 1);
    if env.alive {
        work::await_bridge(env, None, file, range, result);
    }
    true
}

fn cross_thread(
    expression: &AstExpr,
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    let applicable = profile(env).and_then(|()| {
        let [coroutine, loop_] = arguments else {
            return Err("cross-thread submission requires a resolved coroutine and loop");
        };
        if !keywords.is_empty()
            || !known_loop(loop_, env)
            || !env.ownership.as_ref().unwrap().work.is_worker_thread()
        {
            return Err("cross-thread coroutine submission requires a known non-loop submitting thread and live loop");
        }
        let id = expr_name(coroutine)
            .and_then(|name| env.coroutine_aliases.get(name))
            .ok_or("cross-thread submission requires a resolved coroutine")?;
        let coroutine = env.coroutines.get(id)
            .filter(|coroutine| !coroutine.consumed && !coroutine.ambiguous)
            .ok_or("cross-thread submission requires a fresh local coroutine")?;
        let (worker, _, _) = call_parts(&coroutine.invocation)
            .filter(|(worker, _, _)| expr_name(worker)
                .and_then(|name| env.function_bindings.get(name))
                .and_then(|key| functions.get(key))
                .is_some_and(|worker| worker.supported_binding && worker.is_async))
            .ok_or("cross-thread coroutine body cannot be resolved")?;
        if coroutine.captures.values().any(|place| place.unknown || place.transferred) {
            return Err("cross-thread coroutine captures are unresolved");
        }
        Ok((id.clone(), worker.clone(), coroutine.captures.values()
            .flat_map(|place| place.targets.iter().cloned())
            .collect()))
    });
    if record(
        "asyncio.run_coroutine_threadsafe",
        "admit-loop-callback",
        applicable.as_ref().map(|_| ()).map_err(|reason| *reason),
        expression,
        file,
        result,
    ) {
        let (id, worker, captures) = applicable.expect("qualified cross-thread coroutine");
        env.coroutines.get_mut(&id).unwrap().consumed = true;
        work::register(
            env,
            None,
            work::Kind::CrossThread,
            file,
            expression.range,
            work::for_worker(env, &worker, functions),
            result,
        );
        work::bind_worker_contract(env, &worker, functions);
        work::capture_pending_bridge(env, captures, false);
        if !work::cross_thread_wrapper(env, file, expression.range) {
            boundary(
                result,
                file,
                expression.range,
                "cross-thread result wrapper budget exceeded",
            );
        }
    } else {
        ownership::argument_escapes(arguments, keywords, env, file, expression.range, result);
        queue_raise(env, expression.range, None);
    }
    true
}

pub(in crate::flow) fn worker_outcome(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    parent: &State,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> work::WorkerExit {
    let ordinary = || work::for_worker(parent, &arguments[0], functions);
    if !parent.async_context || parent.call_depth >= MAX_CALL_DEPTH {
        return ordinary();
    }
    let Some(name) = expr_name(&arguments[0]) else {
        return ordinary();
    };
    let Some(definition) = parent
        .function_bindings
        .get(name)
        .and_then(|key| functions.get(key))
    else {
        return ordinary();
    };
    let Ok(bindings) = bind_arguments(&definition.signature, &arguments[1..], keywords, false)
    else {
        return ordinary();
    };
    let loop_names = bindings
        .iter()
        .filter(|(_, value)| known_loop(value, parent))
        .map(|(parameter, _)| parameter.clone())
        .chain(
            parent
                .ownership
                .as_ref()
                .into_iter()
                .flat_map(|ownership| ownership.closure_captures(name))
                .filter(|capture| {
                    parent
                        .ownership
                        .as_ref()
                        .unwrap()
                        .work
                        .running_loop(capture)
                })
                .cloned(),
        )
        .collect::<Vec<_>>();
    if loop_names.is_empty() {
        return ordinary();
    }
    let Some(mut ownership) = OwnershipState::for_child(parent, false) else {
        return ordinary();
    };
    ownership.work.mark_worker_thread(loop_names);
    let (module_functions, module_classes) = module_bindings_for_child(parent, definition);
    let mut child = State {
        ownership: Some(ownership),
        function_bindings: module_functions.clone(),
        class_bindings: module_classes.clone(),
        module_function_bindings: module_functions,
        module_class_bindings: module_classes,
        function_bindings_complete: true,
        class_bindings_complete: true,
        scope_module: definition.file.module.clone(),
        target_python_version: parent.target_python_version.clone(),
        models: parent.models.clone(),
        async_guard_models: parent.async_guard_models.clone(),
        call_depth: parent.call_depth + 1,
        alive: true,
        ..State::default()
    };
    for (parameter, value) in &bindings {
        let Some(objects) = expr_name(value).and_then(|name| parent.places.get(name)) else {
            continue;
        };
        copy_object_metadata(&mut child, parent, objects);
        child.places.insert(parameter.clone(), objects.clone());
        child.borrowed.extend(objects.iter().cloned());
    }
    let exits = execute(
        &definition.statements,
        &mut child,
        &definition.file,
        &definition.contracts,
        functions,
        result,
        0,
    );
    let range = definition
        .statements
        .last()
        .map_or(arguments[0].range, statement_range);
    work::report_scope(&child, &exits, &definition.file, range, result);
    let normal = child.alive || !exits.returns.is_empty();
    let errors = exits
        .raises
        .iter()
        .map(|exit| completion_exception_name(&exit.kind, &exit.state).map(str::to_owned))
        .collect();
    work::WorkerExit { normal, errors }
}

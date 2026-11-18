use super::tasks::{ScheduledTask, TaskGroup};
use super::*;

const ASYNC_MODEL: &str = "TPM-ASYNCIO-001";
const STANDALONE: &str = ":standalone:";
const CREATE_TASK: &str = "external:asyncio.create_task";
const SHIELD: &str = "external:asyncio.shield";
const SLEEP: &str = "external:asyncio.sleep";
const TIMEOUT: &str = "external:asyncio.timeout";

fn identity(expression: &AstExpr, env: &State) -> Option<String> {
    let (function, _, _) = call_parts(expression)?;
    ownership::external_identity(function, env)
}

fn enabled(env: &State) -> bool {
    env.models.foundations.is_some() && env.ownership.is_some()
}

fn applicable(env: &State) -> Result<(), &'static str> {
    let default_factory = env.ownership.as_ref().is_some_and(|ownership| {
        ownership.mode.assumes_default_task_factory() && !ownership.incompatible_task_factory
    });
    runtime::runtime_profile(env)?.asyncio(&env.target_python_version, default_factory)
}

fn simple_argument(expression: &AstExpr) -> Option<&AstExpr> {
    let (_, arguments, keywords) = call_parts(expression)?;
    match (arguments, keywords) {
        ([argument], []) => Some(argument),
        _ => None,
    }
}

fn schedule_id(env: &State, argument: &AstExpr) -> Option<String> {
    let id = env.coroutine_aliases.get(expr_name(argument)?)?;
    let record = env.coroutines.get(id)?;
    let valid = !record.consumed
        && !record.ambiguous
        && call_parts(&record.invocation).is_some_and(|(function, _, _)| {
            expr_name(function).is_some() || super::library::direct_task(record, env)
        })
        && !env
            .ownership
            .as_ref()?
            .groups
            .values()
            .any(|group| group.tasks.contains_key(id));
    valid.then(|| id.clone())
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> calls::ModelCall {
    if !enabled(env) {
        return calls::ModelCall::Unrecognized;
    }
    match identity(expression, env).as_deref() {
        Some("external:asyncio.set_event_loop" | "external:asyncio.set_event_loop_policy") => {
            let ownership = env.ownership.as_mut().expect("asyncio preview");
            ownership.incompatible_task_factory = true;
            for (id, group) in &mut ownership.groups {
                if id.starts_with(STANDALONE) {
                    group.incomplete = true;
                }
            }
            boundary(
                result,
                file,
                expression.range,
                "event loop mutation invalidates the declared asyncio profile",
            );
            queue_raise(env, expression.range, None);
            calls::ModelCall::Declined
        }
        Some(CREATE_TASK) => {
            let id = simple_argument(expression).and_then(|argument| schedule_id(env, argument));
            let applicability = applicable(env).and_then(|()| {
                if !env.async_context {
                    return Err("asyncio.create_task requires an established running loop");
                }
                if id.is_none() {
                    return Err(
                        "asyncio.create_task requires one resolved, unscheduled local coroutine",
                    );
                }
                if env.ownership.as_ref().unwrap().groups.len() >= MAX_PENDING_COROUTINES {
                    return Err("local task identity budget exceeded");
                }
                Ok(())
            });
            if !runtime::use_model(
                "asyncio.create_task",
                "schedule",
                ASYNC_MODEL,
                applicability,
                file,
                expression.range,
                result,
            ) {
                let range = expression.range;
                let error = (!env.async_context).then(|| AstExpr {
                    range,
                    kind: AstExprKind::Name("RuntimeError".into()),
                });
                queue_raise(env, range, error);
                // On rejection the coroutine and any already evaluated ownership
                // moves remain with the caller's pending obligations.
                return calls::ModelCall::Declined;
            }
            let id = id.expect("applicable scheduling arguments");
            super::library::check_scheduled_operation(&id, env, file, expression.range, result);
            let group = format!("{STANDALONE}{id}");
            let ownership = env.ownership.as_mut().unwrap();
            ownership.groups.insert(
                group.clone(),
                TaskGroup {
                    tasks: BTreeMap::from([(
                        id.clone(),
                        ScheduledTask {
                            site: span(file, expression.range),
                            possible: false,
                            cancel_requested: false,
                            may_have_started: false,
                            cancelled_before_start: false,
                        },
                    )]),
                    ..TaskGroup::default()
                },
            );
            ownership.pending_task = Some((group, id.clone()));
            work::schedule_task(env, &id, file, expression.range, result);
            calls::ModelCall::Applied
        }
        _ => calls::ModelCall::Unrecognized,
    }
}

pub(super) fn returns_task(expression: &AstExpr, env: &State) -> bool {
    enabled(env) && identity(expression, env).as_deref() == Some(CREATE_TASK)
}

pub(super) fn is_scheduled(env: &State, coroutine: &str) -> bool {
    env.models.foundations.is_some()
        && env.ownership.as_ref().is_some_and(|ownership| {
            ownership.groups.iter().any(|(group, work)| {
                group.starts_with(STANDALONE) && work.tasks.contains_key(coroutine)
            })
        })
}

fn queue_cancelled(env: &mut State, range: SourceRange, mut after: State) {
    after.alive = true;
    after.pending_raises.clear();
    env.pending_raises.push(FlowExit {
        state: after,
        range,
        kind: CompletionKind::Cancelled(None),
    });
}

pub(super) fn queue_deadline_cancellations(
    env: &mut State,
    range: SourceRange,
    after: &State,
    immediate_only: bool,
) {
    let Some(ownership) = &after.ownership else {
        return;
    };
    for (deadline, immediate) in &ownership.active_timeouts {
        if immediate_only && !immediate {
            continue;
        }
        let mut state = after.clone();
        state.alive = true;
        state.pending_raises.clear();
        env.pending_raises.push(FlowExit {
            state,
            range,
            kind: CompletionKind::Cancelled(Some(deadline.clone())),
        });
    }
}

fn number_literal(expression: &AstExpr, file: &SourceFile) -> Option<f64> {
    if !matches!(expression.kind, AstExprKind::Primitive) {
        return None;
    }
    file.source
        .get(expression.range.start..expression.range.end)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub(super) fn enter_timeout(
    item: &FlowWithItem,
    deadline: &str,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<bool> {
    if !enabled(env) || identity(&item.context, env).as_deref() != Some(TIMEOUT) {
        return None;
    }
    let argument = simple_argument(&item.context);
    let delay = argument.and_then(|value| number_literal(value, file));
    let applicability = applicable(env).and_then(|()| {
        (item.target.is_none() && !item.unsupported_target && argument.is_some_and(|value|
            matches!(value.kind, AstExprKind::NoneLiteral) || delay.is_some()))
            .then_some(())
            .ok_or("asyncio.timeout requires a literal delay and an unbound context manager in this profile")
    });
    if !runtime::use_model(
        "asyncio.timeout",
        "timeout-context",
        ASYNC_MODEL,
        applicability,
        file,
        item.range,
        result,
    ) {
        return None;
    }
    Some(delay.is_some_and(|delay| {
        env.ownership
            .as_mut()
            .unwrap()
            .active_timeouts
            .insert(deadline.into(), delay <= 0.0);
        true
    }))
}

pub(super) fn leave_timeout(
    deadline: &str,
    env: &mut State,
    range: SourceRange,
    incoming: Option<&CompletionKind>,
) {
    if let Some(ownership) = &mut env.ownership {
        ownership.active_timeouts.remove(deadline);
    }
    if matches!(incoming, Some(CompletionKind::Cancelled(Some(origin))) if origin == deadline) {
        queue_raise(
            env,
            range,
            Some(AstExpr {
                range,
                kind: AstExprKind::Name("TimeoutError".into()),
            }),
        );
        env.alive = false;
    }
}

fn complete(env: &mut State, id: &str, error: Option<Option<String>>) {
    if let Some(record) = env.coroutines.get_mut(id) {
        record.complete_task(TaskCompletion {
            normal: error.is_none(),
            errors: error.into_iter().collect(),
        });
    }
}

fn finish_path(
    caller: &State,
    worker: &State,
    id: &str,
    record: &CoroutineRecord,
    error: Option<Option<String>>,
) -> State {
    let mut path = caller.clone();
    path.pending_raises.clear();
    path.alive = true;
    complete(&mut path, id, error);
    for capture in record.captures.values() {
        for object in &capture.targets {
            if capture.transferred {
                tasks::apply_worker_cleanup(&mut path, object, Some(worker), false);
            } else if worker.escaped.contains(object)
                || worker.forbidden_borrowed_closes.contains(object)
            {
                path.escaped.insert(object.clone());
                *path.objects.entry(object.clone()).or_default() |= UNKNOWN;
            }
            tasks::apply_worker_protocol_state(&mut path, object, Some(worker), false);
        }
    }
    path
}

#[allow(clippy::too_many_arguments)]
pub(super) fn await_local(
    awaited: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if !enabled(env) {
        return false;
    }
    let callee = identity(awaited, env);
    if callee.as_deref() == Some(SLEEP) {
        let applicability = applicable(env).and_then(|()| {
            simple_argument(awaited)
                .is_some_and(|value| number_literal(value, file).is_some())
                .then_some(())
                .ok_or("modeled asyncio.sleep requires a literal delay")
        });
        if runtime::use_model(
            "asyncio.sleep",
            "suspend",
            ASYNC_MODEL,
            applicability,
            file,
            range,
            result,
        ) {
            let interrupted = env.clone();
            queue_deadline_cancellations(env, range, &interrupted, false);
            queue_cancelled(env, range, interrupted);
            return true;
        }
        return false;
    }
    let shielded = callee.as_deref() == Some(SHIELD);
    let target = if shielded {
        simple_argument(awaited)
    } else {
        Some(awaited)
    };
    target.and_then(expr_name).is_some_and(|name| {
        deliver_task(
            name,
            Delivery::Await { shielded },
            range,
            env,
            file,
            contracts,
            functions,
            result,
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    Await { shielded: bool },
    Result,
    Exception,
    Join,
    Collect,
    WaitOnly,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn task_result(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if !enabled(env) {
        return false;
    }
    let Some((receiver, method, [], [])) = called_method(expr) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    let delivery = match method {
        "result" => Delivery::Result,
        "exception" => Delivery::Exception,
        "done"
            if env
                .ownership
                .as_ref()
                .is_some_and(|ownership| ownership.task_names.contains_key(name)) =>
        {
            runtime::use_model(
                "asyncio.Task",
                "done",
                ASYNC_MODEL,
                applicable(env),
                file,
                expr.range,
                result,
            );
            return true;
        }
        _ => return false,
    };
    deliver_task(
        name, delivery, expr.range, env, file, contracts, functions, result,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn deliver_task(
    name: &str,
    delivery: Delivery,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    let Some((group, id)) = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.task_names.get(name))
        .cloned()
    else {
        return false;
    };
    let (symbol, effect) = match delivery {
        Delivery::Await { shielded: true } => ("asyncio.shield", "await-local-task"),
        Delivery::Await { shielded: false } => ("asyncio.Task", "await-local-task"),
        Delivery::Result => ("asyncio.Task", "result"),
        Delivery::Exception => ("asyncio.Task", "exception"),
        Delivery::Join | Delivery::Collect => ("asyncio.Task", "observe-gather-member"),
        Delivery::WaitOnly => ("asyncio.Task", "complete-member-without-observation"),
    };
    let Some(record) = env.coroutines.get(&id).cloned() else {
        boundary(
            result,
            file,
            range,
            "awaited task has no retained coroutine identity",
        );
        return true;
    };
    let group_state = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.groups.get(&group));
    let task = group_state.and_then(|group| group.tasks.get(&id)).cloned();
    let applies = applicable(env).and_then(|()| {
        if !record.consumed
            && (record.ambiguous
                || group_state.is_none_or(|group| group.incomplete)
                || task.as_ref().is_none_or(|task| task.possible))
        {
            return Err("awaited task or its captures escape the bounded completion model");
        }
        Ok(())
    });
    if !runtime::use_model(symbol, effect, ASYNC_MODEL, applies, file, range, result) {
        if matches!(delivery, Delivery::Await { .. }) {
            queue_cancelled(env, range, env.clone());
        } else {
            queue_raise(env, range, None);
        }
        return true;
    }
    if record.consumed {
        if let Some(completion) = &record.task_completion {
            let mut normal = Vec::new();
            let mut failures = Vec::new();
            if completion.normal {
                let mut path = env.clone();
                if !matches!(delivery, Delivery::Exception | Delivery::WaitOnly) {
                    work::deliver_buffered(&mut path, &id);
                    work::observe(&mut path, &id);
                    deliver_typed_task(&mut path, &id, &record, file, range, functions, result);
                }
                normal.push(path);
            }
            for error in completion.errors.iter().cloned() {
                let mut path = env.clone();
                let cancelled = error.as_deref() == Some("CancelledError");
                if cancelled {
                    work::cancel(&mut path, &id);
                } else if delivery != Delivery::WaitOnly {
                    work::observe(&mut path, &id);
                }
                if matches!(delivery, Delivery::Collect | Delivery::WaitOnly)
                    || (delivery == Delivery::Exception && !cancelled)
                {
                    normal.push(path);
                } else {
                    failures.push(FlowExit {
                        state: path,
                        range,
                        kind: if cancelled {
                            CompletionKind::Cancelled(None)
                        } else {
                            CompletionKind::Raise(error.map(|name| AstExpr {
                                range,
                                kind: AstExprKind::Name(name),
                            }))
                        },
                    });
                }
            }
            if let Some(joined) = join_states(normal) {
                *env = joined;
            } else {
                env.alive = false;
            }
            env.pending_raises.extend(failures);
        } else {
            boundary(result, file, range, "completed task outcome is unresolved");
            queue_raise(env, range, None);
        }
        return true;
    }
    let task = task.expect("applicable tracked task");
    let retrieval = matches!(delivery, Delivery::Result | Delivery::Exception);
    let ready = env
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.work.is_ready(&id));
    if retrieval && !ready {
        queue_raise(
            env,
            range,
            Some(AstExpr {
                range,
                kind: AstExprKind::Name(":work-error:asyncio.InvalidStateError".into()),
            }),
        );
        if !task.may_have_started {
            env.alive = false;
            return true;
        }
    }
    if task.cancelled_before_start {
        let mut cancelled = env.clone();
        complete(&mut cancelled, &id, Some(Some("CancelledError".into())));
        work::cancel(&mut cancelled, &id);
        if matches!(delivery, Delivery::Collect | Delivery::WaitOnly) {
            *env = cancelled;
            return true;
        }
        if let Delivery::Await { shielded } = delivery {
            let interrupted = if shielded {
                env.clone()
            } else {
                cancelled.clone()
            };
            queue_deadline_cancellations(env, range, &interrupted, true);
        }
        queue_cancelled(env, range, cancelled);
        env.alive = false;
        return true;
    }
    let mut worker =
        tasks::execute_scheduled_coroutine(&id, &record, env, file, contracts, functions, result);
    let exits = take_pending_raises(&mut worker);
    let previous = env.clone();
    let mut normal = Vec::new();
    let mut failures = Vec::new();
    if worker.alive {
        let mut path = finish_path(&previous, &worker, &id, &record, None);
        if library::direct_task(&record, &previous) {
            work::retain_buffered(&mut path, &worker, &id);
        }
        if !matches!(delivery, Delivery::Exception | Delivery::WaitOnly) {
            work::deliver_buffered(&mut path, &id);
            work::observe(&mut path, &id);
            deliver_typed_task(&mut path, &id, &record, file, range, functions, result);
        } else {
            work::ready(&mut path, &id);
        }
        normal.push(path);
    }
    for exit in exits {
        let name = completion_exception_name(&exit.kind, &exit.state).map(str::to_owned);
        let cancelled = matches!(exit.kind, CompletionKind::Cancelled(_))
            || name.as_deref() == Some("CancelledError");
        let mut state = finish_path(
            &previous,
            &exit.state,
            &id,
            &record,
            Some(if cancelled {
                Some("CancelledError".into())
            } else {
                name
            }),
        );
        if cancelled {
            work::cancel(&mut state, &id);
        } else if delivery != Delivery::WaitOnly {
            work::observe(&mut state, &id);
        } else {
            work::ready(&mut state, &id);
        }
        if matches!(delivery, Delivery::Collect | Delivery::WaitOnly)
            || (delivery == Delivery::Exception && !cancelled)
        {
            normal.push(state);
        } else {
            failures.push(FlowExit {
                state,
                range,
                kind: if cancelled {
                    CompletionKind::Cancelled(None)
                } else {
                    exit.kind
                },
            });
        }
    }
    if let Some(mut joined) = join_states(normal) {
        joined.pending_raises = previous.pending_raises.clone();
        *env = joined;
    } else {
        env.alive = false;
    }
    env.pending_raises.extend(failures);
    if let Delivery::Await { shielded } = delivery {
        let mut interrupted = previous;
        if !shielded {
            complete(&mut interrupted, &id, Some(Some("CancelledError".into())));
            work::cancel(&mut interrupted, &id);
            for capture in record.captures.values() {
                for object in &capture.targets {
                    if capture.transferred || worker.escaped.contains(object) {
                        *interrupted.objects.entry(object.clone()).or_default() |= UNKNOWN;
                    }
                }
            }
        }
        queue_deadline_cancellations(env, range, &interrupted, false);
        queue_cancelled(env, range, interrupted);
    }
    true
}

fn deliver_typed_task(
    env: &mut State,
    id: &str,
    record: &CoroutineRecord,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    if let Some(contract) = call_parts(&record.invocation)
        .and_then(|(function, _, _)| expr_name(function))
        .and_then(|callee| env.function_bindings.get(callee))
        .and_then(|binding| functions.get(binding))
        .and_then(|definition| definition.outcome_return)
    {
        outcomes::deliver_work(env, id, contract, file, range, result);
    }
}

const MAX_COMBINATOR_MEMBERS: usize = 8;

fn fixed_members(values: &[AstExpr], env: &State) -> Option<Vec<String>> {
    let ownership = env.ownership.as_ref()?;
    let names = values.iter().map(expr_name).collect::<Option<Vec<_>>>()?;
    let ids = names
        .iter()
        .map(|name| {
            ownership
                .task_names
                .get(*name)
                .map(|(_, id)| id.as_str())
                .or_else(|| ownership.work.future_id(name))
        })
        .collect::<Option<BTreeSet<_>>>()?;
    (names.len() == ids.len() && (1..=MAX_COMBINATOR_MEMBERS).contains(&names.len()))
        .then(|| names.into_iter().map(str::to_owned).collect())
}

fn fixed_collection(expr: &AstExpr, env: &State) -> Option<Vec<String>> {
    match &expr.kind {
        AstExprKind::Tuple(items) | AstExprKind::Collection(items) => fixed_members(items, env),
        _ => None,
    }
}

pub(super) fn as_completed_members(expr: &AstExpr, env: &State) -> Option<Vec<String>> {
    if identity(expr, env).as_deref() != Some("external:asyncio.as_completed")
        || applicable(env).is_err()
        || !env.async_context
        || env.target_python_version != "3.13"
    {
        return None;
    }
    let members = fixed_collection(simple_argument(expr)?, env)?;
    // Enumerating all observed orderings has a separate small budget.
    (members.len() <= 3).then_some(members)
}

pub(super) fn is_gather(expr: &AstExpr, env: &State) -> bool {
    enabled(env)
        && matches!(&expr.kind, AstExprKind::Await(awaited) if identity(awaited, env).as_deref() == Some("external:asyncio.gather"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn deliver_member(
    name: &str,
    mode: Delivery,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    env.outcomes.clear_value();
    env.pending_return_targets = None;
    if !deliver_task(name, mode, range, env, file, contracts, functions, result) {
        work::combine_future(name, range, mode, env, file, result);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn await_combination(
    awaited: &AstExpr,
    range: SourceRange,
    unpack: Option<&[String]>,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if !enabled(env) {
        return false;
    }
    let symbol = match identity(awaited, env).as_deref() {
        Some("external:asyncio.gather") => "asyncio.gather",
        Some("external:asyncio.wait") => "asyncio.wait",
        _ => return false,
    };
    let Some((_, arguments, keywords)) = call_parts(awaited) else {
        return false;
    };
    let gathers = symbol == "asyncio.gather";
    let mut collect_errors = false;
    let mut waits_all = true;
    let members = if gathers {
        match keywords {
            [] => fixed_members(arguments, env),
            [(
                Some(key),
                AstExpr {
                    kind: AstExprKind::Boolean(enabled),
                    ..
                },
            )] if key == "return_exceptions" => {
                collect_errors = *enabled;
                fixed_members(arguments, env)
            }
            _ => None,
        }
    } else {
        match (arguments, keywords) {
            ([collection], []) => fixed_collection(collection, env),
            ([collection], [(Some(key), timeout)]) if key == "timeout" => {
                waits_all = matches!(timeout.kind, AstExprKind::NoneLiteral);
                if waits_all || number_literal(timeout, file).is_some() {
                    fixed_collection(collection, env)
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    let applies = applicable(env).and_then(|()| members.as_ref().map(|_| ()).ok_or(
        "asyncio combinator requires bounded distinct local task/future identities and literal options",
    ));
    if !runtime::use_model(
        symbol,
        "fixed-member-wait",
        ASYNC_MODEL,
        applies,
        file,
        range,
        result,
    ) {
        queue_raise(env, range, None);
        return true;
    }
    let members = members.expect("applicable combination members");
    let previous = env.clone();
    if !waits_all {
        // A timed wait returns done and pending members; neither set observes a result.
        queue_cancelled(env, range, previous);
        return true;
    }

    let mode = if !gathers {
        Delivery::WaitOnly
    } else if collect_errors {
        Delivery::Collect
    } else {
        Delivery::Join
    };
    let mut failed_first = Vec::new();
    if mode == Delivery::Join {
        for member in &members {
            let mut candidate = previous.clone();
            candidate.pending_raises.clear();
            deliver_member(
                member,
                mode,
                range,
                &mut candidate,
                file,
                contracts,
                functions,
                result,
            );
            failed_first.extend(take_pending_raises(&mut candidate));
        }
    }
    let mut values = Vec::new();
    for member in &members {
        if !env.alive {
            break;
        }
        deliver_member(member, mode, range, env, file, contracts, functions, result);
        values.push((
            env.outcomes.take_carrier(),
            env.pending_return_targets.take(),
        ));
    }
    if mode == Delivery::Join {
        // The first error is chosen by completion order, not argument order.
        // Its path must not inherit observation of the other members.
        env.pending_raises.clone_from(&previous.pending_raises);
        env.pending_raises.extend(failed_first);
    }
    queue_deadline_cancellations(env, range, &previous, false);
    queue_cancelled(env, range, previous);
    if env.alive && gathers {
        if let Some(names) = unpack {
            if names.len() != values.len() {
                queue_raise(
                    env,
                    range,
                    Some(AstExpr {
                        range,
                        kind: AstExprKind::Name(":work-error:ValueError".into()),
                    }),
                );
                env.alive = false;
                return true;
            }
            for (name, (value, buffered)) in names.iter().zip(values) {
                env.bind_runtime_handle(name);
                if let Some(targets) = buffered {
                    env.places.insert(name.clone(), targets);
                }
                let old = env.outcomes.bind_carrier(name, value);
                outcomes::lose(env, old, file, range, result);
            }
        } else {
            let mut carried = BTreeSet::new();
            let mut buffered = BTreeSet::new();
            for (value, targets) in values {
                carried.extend(value.ids);
                buffered.extend(targets.into_iter().flatten());
            }
            outcomes::escape(env, &carried, file, range, result);
            if !buffered.is_empty() {
                boundary(
                    result,
                    file,
                    range,
                    "buffered work value escapes through an unsupported aggregate carrier",
                );
                env.escaped.extend(buffered.iter().cloned());
                weaken(env, buffered.into_iter());
            }
        }
    }
    true
}

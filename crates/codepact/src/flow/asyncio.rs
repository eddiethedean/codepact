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
        && call_parts(&record.invocation)
            .is_some_and(|(function, _, _)| expr_name(function).is_some())
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
            ownership.pending_task = Some((group, id));
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
    }
    .and_then(expr_name);
    let Some((group, id)) = target
        .and_then(|name| env.ownership.as_ref()?.task_names.get(name))
        .cloned()
    else {
        return false;
    };
    let symbol = if shielded {
        "asyncio.shield"
    } else {
        "asyncio.Task"
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
    if !runtime::use_model(
        symbol,
        "await-local-task",
        ASYNC_MODEL,
        applies,
        file,
        range,
        result,
    ) {
        queue_cancelled(env, range, env.clone());
        return true;
    }
    if record.consumed {
        if let Some(completion) = record.task_completion {
            for error in completion.errors {
                if error.as_deref() == Some("CancelledError") {
                    queue_cancelled(env, range, env.clone());
                } else {
                    queue_raise(
                        env,
                        range,
                        error.map(|name| AstExpr {
                            range,
                            kind: AstExprKind::Name(name),
                        }),
                    );
                }
            }
            env.alive &= completion.normal;
        } else {
            boundary(result, file, range, "completed task outcome is unresolved");
            queue_raise(env, range, None);
        }
        return true;
    }
    let task = task.expect("applicable tracked task");
    if task.cancelled_before_start {
        let mut cancelled = env.clone();
        complete(&mut cancelled, &id, Some(Some("CancelledError".into())));
        let interrupted = if shielded {
            env.clone()
        } else {
            cancelled.clone()
        };
        queue_deadline_cancellations(env, range, &interrupted, true);
        queue_cancelled(env, range, cancelled);
        env.alive = false;
        return true;
    }
    let mut worker =
        tasks::execute_scheduled_coroutine(&id, &record, env, file, contracts, functions, result);
    let exits = take_pending_raises(&mut worker);
    let previous = env.clone();
    if worker.alive {
        *env = finish_path(&previous, &worker, &id, &record, None);
        env.pending_raises = previous.pending_raises.clone();
    } else {
        env.alive = false;
    }
    for exit in exits {
        let name = completion_exception_name(&exit.kind, &exit.state).map(str::to_owned);
        let cancelled = matches!(exit.kind, CompletionKind::Cancelled(_))
            || name.as_deref() == Some("CancelledError");
        let state = finish_path(
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
        env.pending_raises.push(FlowExit {
            state,
            range,
            kind: if cancelled {
                CompletionKind::Cancelled(None)
            } else {
                exit.kind
            },
        });
    }
    let mut interrupted = previous;
    if !shielded {
        complete(&mut interrupted, &id, Some(Some("CancelledError".into())));
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
    true
}

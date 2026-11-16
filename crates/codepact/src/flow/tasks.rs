use super::ownership::{
    escape_task_handles, external_identity, ExecutorState, ScheduledTask, TaskGroup,
    EXECUTOR_ACCEPTING, EXECUTOR_SHUTDOWN, EXECUTOR_UNKNOWN, TASK_GROUP,
};
use super::*;

const THREAD_POOL_EXECUTOR: &str = "external:concurrent.futures.ThreadPoolExecutor";
const THREAD_POOL_MODEL: &str = "TPM-EXECUTOR-001@v3.13.16";
const MAX_EXECUTORS: usize = 32;

fn task_group_model_identity(env: &State) -> String {
    format!(
        "TPM-TASK-001@{}",
        crate::model::task_group_revision(&env.target_python_version).unwrap_or("unknown")
    )
}

pub(super) fn is_group_context(expression: &AstExpr, env: &State) -> bool {
    match &expression.kind {
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            arguments.is_empty()
                && keywords.is_empty()
                && external_identity(function, env).as_deref() == Some(TASK_GROUP)
        }
        AstExprKind::Name(name) => env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.group_names.contains_key(name)),
        _ => false,
    }
}

pub(super) fn initialize_executor_handle(
    name: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &expression.kind
    else {
        return false;
    };
    if external_identity(function, env).as_deref() != Some(THREAD_POOL_EXECUTOR)
        || !env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
    {
        return false;
    }
    if !arguments.is_empty() || !keywords.is_empty() {
        return false;
    }
    if env
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.executors.len() >= MAX_EXECUTORS)
    {
        boundary(
            result,
            file,
            expression.range,
            "ThreadPoolExecutor handle budget exceeded; local protocol state is unknown",
        );
        let ownership = env.ownership.as_mut().expect("advanced executor handle");
        ownership.rebind(name);
        env.places.remove(name);
        env.unknown_places.insert(name.to_owned());
        env.possibly_unbound_places.remove(name);
        env.definitely_bound_names.insert(name.to_owned());
        env.shadowed_functions.insert(name.to_owned());
        env.shadowed_classes.insert(name.to_owned());
        return true;
    }
    let id = format!(
        "{}:{}:{}",
        file.path,
        expression.range.start,
        env.allocation_context.join("/")
    );
    let supported_pin = env.target_python_version.starts_with("3.13");
    let initial = if supported_pin {
        EXECUTOR_ACCEPTING
    } else {
        boundary(
            result,
            file,
            expression.range,
            "ThreadPoolExecutor model is qualified only for CPython 3.13.16",
        );
        EXECUTOR_UNKNOWN
    };
    let origin = span(file, expression.range);
    let mut exceptional = env.clone();
    exceptional.alive = true;
    exceptional.pending_raises.clear();
    exceptional.places.remove(name);
    exceptional.unknown_places.remove(name);
    exceptional.definitely_bound_names.remove(name);
    exceptional.possibly_unbound_places.remove(name);
    if let Some(ownership) = exceptional.ownership.as_mut() {
        ownership.rebind(name);
    }
    env.pending_raises.push(FlowExit {
        state: exceptional,
        range: expression.range,
        kind: CompletionKind::Raise(None),
    });
    let ownership = env.ownership.as_mut().expect("advanced executor handle");
    ownership.rebind(name);
    ownership.executors.insert(
        id.clone(),
        ExecutorState {
            states: initial,
            origin,
            has_submissions: false,
        },
    );
    ownership.executor_names.insert(name.to_owned(), id);
    env.places.remove(name);
    env.unknown_places.remove(name);
    env.coroutine_aliases.remove(name);
    env.unknown_coroutine_aliases.remove(name);
    env.possibly_unbound_places.remove(name);
    env.definitely_bound_names.insert(name.to_owned());
    env.shadowed_functions.insert(name.to_owned());
    env.shadowed_classes.insert(name.to_owned());
    true
}

pub(super) fn executor_call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    let Some(id) = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.executor_names.get(name))
        .cloned()
    else {
        return false;
    };
    let Some(snapshot) = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.executors.get(&id))
        .cloned()
    else {
        boundary(
            result,
            file,
            expression.range,
            "ThreadPoolExecutor protocol state is missing",
        );
        queue_raise(env, expression.range, None);
        return true;
    };
    if !env.target_python_version.starts_with("3.13") {
        if let Some(executor) = env
            .ownership
            .as_mut()
            .and_then(|ownership| ownership.executors.get_mut(&id))
        {
            executor.states |= EXECUTOR_UNKNOWN;
        }
        boundary(
            result,
            file,
            expression.range,
            "ThreadPoolExecutor model is qualified only for CPython 3.13.16",
        );
        queue_raise(env, expression.range, None);
        return true;
    }
    match method {
        "submit" => {
            let has_accepting = snapshot.states & EXECUTOR_ACCEPTING != 0;
            let has_shutdown = snapshot.states & EXECUTOR_SHUTDOWN != 0;
            let has_unknown = snapshot.states & EXECUTOR_UNKNOWN != 0;
            if has_shutdown {
                report(
                    result,
                    "TP2102",
                    "ThreadPoolExecutor.submit requires the Accepting state",
                    span(file, expression.range),
                    vec![snapshot.origin.clone()],
                    if has_accepting || has_unknown {
                        "possible"
                    } else {
                        "definite"
                    },
                    vec![THREAD_POOL_MODEL.into()],
                );
            }
            let supported = if has_accepting {
                supported_submit(arguments, keywords, env, functions)
            } else {
                false
            };
            if has_unknown || has_accepting && !supported {
                boundary(
                    result,
                    file,
                    expression.range,
                    if has_unknown {
                        "ThreadPoolExecutor state is unknown at submit"
                    } else {
                        "ThreadPoolExecutor submit requires a resolved worker without tracked captures or resources"
                    },
                );
            }
            if has_shutdown && !has_accepting && !has_unknown {
                queue_raise(
                    env,
                    expression.range,
                    Some(AstExpr {
                        range: expression.range,
                        kind: AstExprKind::Name("RuntimeError".into()),
                    }),
                );
                env.alive = false;
                return true;
            }
            if let Some(ownership) = env.ownership.as_mut() {
                if let Some(executor) = ownership.executors.get_mut(&id) {
                    if supported && has_accepting {
                        executor.has_submissions = true;
                        executor.states &= !EXECUTOR_SHUTDOWN;
                        if has_unknown {
                            executor.states |= EXECUTOR_UNKNOWN;
                        } else {
                            executor.states = EXECUTOR_ACCEPTING;
                        }
                    } else {
                        executor.states |= EXECUTOR_UNKNOWN;
                    }
                }
            }
            if supported && has_accepting {
                let mut exceptional = env.clone();
                exceptional.alive = true;
                exceptional.pending_raises.clear();
                if let Some(executor) = exceptional
                    .ownership
                    .as_mut()
                    .and_then(|ownership| ownership.executors.get_mut(&id))
                {
                    executor.states |= EXECUTOR_UNKNOWN;
                }
                env.pending_raises.push(FlowExit {
                    state: exceptional,
                    range: expression.range,
                    kind: CompletionKind::Raise(None),
                });
            } else {
                queue_raise(env, expression.range, None);
            }
            true
        }
        "shutdown" => {
            let Some(wait) = shutdown_wait(arguments, keywords) else {
                if let Some(executor) = env
                    .ownership
                    .as_mut()
                    .and_then(|ownership| ownership.executors.get_mut(&id))
                {
                    executor.states |= EXECUTOR_UNKNOWN;
                }
                boundary(
                    result,
                    file,
                    expression.range,
                    "ThreadPoolExecutor shutdown arguments are outside the qualified literal subset",
                );
                queue_raise(env, expression.range, None);
                return true;
            };
            let mut next_states = if snapshot.states & EXECUTOR_UNKNOWN != 0 {
                EXECUTOR_UNKNOWN
            } else {
                0
            };
            if snapshot.states & (EXECUTOR_ACCEPTING | EXECUTOR_SHUTDOWN) != 0 {
                next_states |= EXECUTOR_SHUTDOWN;
            }
            if next_states & EXECUTOR_UNKNOWN != 0 {
                boundary(
                    result,
                    file,
                    expression.range,
                    "ThreadPoolExecutor shutdown outcome includes an unknown entry state",
                );
            }
            if snapshot.has_submissions && !wait {
                boundary(
                    result,
                    file,
                    expression.range,
                    "shutdown(wait=False) does not establish worker completion",
                );
            }
            if let Some(executor) = env
                .ownership
                .as_mut()
                .and_then(|ownership| ownership.executors.get_mut(&id))
            {
                executor.states = next_states;
            }
            let mut exceptional = env.clone();
            exceptional.alive = true;
            exceptional.pending_raises.clear();
            env.pending_raises.push(FlowExit {
                state: exceptional,
                range: expression.range,
                kind: CompletionKind::Raise(None),
            });
            true
        }
        _ => {
            if let Some(executor) = env
                .ownership
                .as_mut()
                .and_then(|ownership| ownership.executors.get_mut(&id))
            {
                executor.states |= EXECUTOR_UNKNOWN;
            }
            boundary(
                result,
                file,
                expression.range,
                "ThreadPoolExecutor operation is outside the qualified local subset",
            );
            queue_raise(env, expression.range, None);
            true
        }
    }
}

fn supported_submit(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
    functions: &FunctionMap,
) -> bool {
    let Some(callee) = arguments.first().and_then(expr_name) else {
        return false;
    };
    if env.shadowed_functions.contains(callee) {
        return false;
    }
    let Some(definition) = env
        .function_bindings
        .get(callee)
        .and_then(|binding| functions.get(binding))
    else {
        return false;
    };
    if env.ownership.as_ref().is_some_and(|ownership| {
        ownership
            .closure_captures(callee)
            .any(|capture| env.places.contains_key(capture) || env.unknown_places.contains(capture))
    }) {
        return false;
    }
    let worker_arguments = &arguments[1..];
    let Ok(bindings) = bind_arguments(&definition.signature, worker_arguments, keywords, false)
    else {
        return false;
    };
    bindings.values().all(|argument| {
        expr_name(argument)
            .is_none_or(|name| !env.places.contains_key(name) && !env.unknown_places.contains(name))
    })
}

fn shutdown_wait(arguments: &[AstExpr], keywords: &[CallKeyword]) -> Option<bool> {
    if arguments.len() > 1
        || keywords
            .iter()
            .any(|(name, _)| !matches!(name.as_deref(), Some("wait" | "cancel_futures")))
    {
        return None;
    }
    let mut wait = true;
    if let Some(argument) = arguments.first() {
        let AstExprKind::Boolean(value) = argument.kind else {
            return None;
        };
        wait = value;
    }
    let mut saw_wait = false;
    let mut saw_cancel = false;
    for (name, value) in keywords {
        match name.as_deref()? {
            "wait" => {
                if saw_wait {
                    return None;
                }
                let AstExprKind::Boolean(value) = value.kind else {
                    return None;
                };
                wait = value;
                saw_wait = true;
            }
            "cancel_futures" => {
                if saw_cancel || !matches!(value.kind, AstExprKind::Boolean(_)) {
                    return None;
                }
                saw_cancel = true;
            }
            _ => return None,
        }
    }
    Some(wait)
}

pub(super) fn initialize_group_handle(
    name: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if !is_group_context(expression, env)
        || !matches!(&expression.kind, AstExprKind::Call { .. })
        || !env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
    {
        return false;
    }
    if env
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.group_names.len() >= 32)
    {
        boundary(
            result,
            file,
            expression.range,
            "structured task group budget exceeded; completion is unknown",
        );
        return true;
    }
    let id = format!(
        "{}:{}:{}",
        file.path,
        expression.range.start,
        env.allocation_context.join("/")
    );
    let ownership = env.ownership.as_mut().expect("advanced TaskGroup handle");
    ownership.rebind(name);
    ownership.group_names.insert(name.to_owned(), id.clone());
    ownership
        .group_origins
        .insert(id.clone(), span(file, expression.range));
    ownership.fresh_groups.insert(id.clone());
    ownership.possibly_fresh_groups.insert(id);
    env.places.remove(name);
    env.unknown_places.remove(name);
    env.coroutine_aliases.remove(name);
    env.unknown_coroutine_aliases.remove(name);
    env.possibly_unbound_places.remove(name);
    env.definitely_bound_names.insert(name.to_owned());
    env.shadowed_functions.insert(name.to_owned());
    env.shadowed_classes.insert(name.to_owned());
    true
}

pub(super) fn enter_group(
    item: &FlowWithItem,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<String> {
    let task_model = task_group_model_identity(env);
    let ownership = env.ownership.as_mut()?;
    if !ownership.mode.assumes_default_task_factory() || ownership.incompatible_task_factory {
        boundary(result, file, item.range, "TaskGroup requires the explicit default-task-factory assumption and a compatible runtime");
        return None;
    }
    let existing_id = expr_name(&item.context)
        .and_then(|name| ownership.group_names.get(name))
        .cloned();
    if item.unsupported_target || (item.target.is_none() && existing_id.is_none()) {
        boundary(
            result,
            file,
            item.range,
            "TaskGroup requires a tracked local context target",
        );
        return None;
    }
    if ownership.groups.len() >= 32 {
        boundary(
            result,
            file,
            item.range,
            "structured task group budget exceeded; completion is unknown",
        );
        return None;
    }
    let inline_construction = existing_id.is_none();
    let id = existing_id.unwrap_or_else(|| {
        format!(
            "{}:{}:{}",
            file.path,
            item.range.start,
            env.allocation_context.join("/")
        )
    });
    let definitely_fresh = inline_construction || ownership.fresh_groups.remove(&id);
    let possibly_fresh =
        inline_construction || ownership.possibly_fresh_groups.remove(&id) || definitely_fresh;
    let active = ownership.groups.contains_key(&id);
    let definitely_entered = ownership.entered_groups.contains(&id);
    let possibly_entered = ownership.possibly_entered_groups.contains(&id);
    if active || (!definitely_fresh && (definitely_entered || possibly_entered)) {
        let certainty = if (active
            && ownership
                .groups
                .get(&id)
                .is_some_and(|group| !group.incomplete))
            || (definitely_entered && !possibly_fresh)
        {
            "definite"
        } else {
            "possible"
        };
        if ownership.mode.includes_advanced_contracts()
            && ownership.mode.assumes_default_task_factory()
            && !ownership.incompatible_task_factory
        {
            report(
                result,
                "TP2102",
                "TaskGroup cannot be entered more than once",
                span(file, item.range),
                ownership
                    .group_origins
                    .get(&id)
                    .cloned()
                    .into_iter()
                    .collect(),
                certainty,
                vec![task_model],
            );
        }
        boundary(
            result,
            file,
            item.range,
            "TaskGroup re-entry outcome cannot be modeled precisely",
        );
        return None;
    }
    ownership.entered_groups.insert(id.clone());
    ownership.possibly_entered_groups.insert(id.clone());
    ownership.fresh_groups.remove(&id);
    ownership.possibly_fresh_groups.remove(&id);
    ownership
        .group_origins
        .entry(id.clone())
        .or_insert_with(|| span(file, item.range));
    ownership.groups.insert(id.clone(), TaskGroup::default());
    if let Some(target) = &item.target {
        ownership.rebind(target);
        ownership.group_names.insert(target.clone(), id.clone());
        env.places.remove(target);
        env.unknown_places.remove(target);
        env.coroutine_aliases.remove(target);
        env.unknown_coroutine_aliases.remove(target);
        env.shadowed_functions.insert(target.clone());
        env.shadowed_classes.insert(target.clone());
    }
    Some(id)
}

pub(super) fn task_call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    let Some(ownership) = env.ownership.as_ref() else {
        return false;
    };
    if method == "set_task_factory" {
        let ownership = env.ownership.as_mut().expect("runtime configuration");
        ownership.incompatible_task_factory = true;
        for group in ownership.groups.values_mut() {
            group.incomplete = true;
        }
        boundary(
            result,
            file,
            expression.range,
            "task factory mutation invalidates the declared default-factory model",
        );
        return false;
    }
    if let Some((group, id)) = ownership.task_names.get(name).cloned() {
        if method == "cancel" && arguments.is_empty() && keywords.is_empty() {
            if let Some(task) = env
                .ownership
                .as_mut()
                .and_then(|ownership| ownership.groups.get_mut(&group))
                .and_then(|group| group.tasks.get_mut(&id))
            {
                task.cancel_requested = true;
                task.cancelled_before_start |= !task.may_have_started && !task.possible;
            }
        } else {
            escape_task_handles(
                env,
                &BTreeSet::from([name.to_owned()]),
                file,
                expression.range,
                result,
            );
            queue_raise(env, expression.range, None);
        }
        return true;
    }
    let Some(group_id) = ownership.group_names.get(name).cloned() else {
        return false;
    };
    if !ownership.mode.assumes_default_task_factory() || ownership.incompatible_task_factory {
        boundary(
            result,
            file,
            expression.range,
            "TaskGroup operation requires the explicit default-task-factory assumption",
        );
        queue_raise(env, expression.range, None);
        return true;
    }
    if method != "create_task" {
        escape_task_handles(
            env,
            &BTreeSet::from([name.to_owned()]),
            file,
            expression.range,
            result,
        );
        return true;
    }
    let id = arguments
        .first()
        .and_then(expr_name)
        .and_then(|name| env.coroutine_aliases.get(name))
        .filter(|id| env.coroutines.contains_key(*id))
        .cloned();
    let supported = arguments.len() == 1 && keywords.is_empty() && id.is_some();
    if !supported {
        boundary(
            result,
            file,
            expression.range,
            "TaskGroup scheduling requires one resolved coroutine and modeled arguments",
        );
        if let Some(group) = env
            .ownership
            .as_mut()
            .and_then(|ownership| ownership.groups.get_mut(&group_id))
        {
            group.incomplete = true;
        }
        queue_raise(env, expression.range, None);
        return true;
    }
    let id = id.expect("validated coroutine identity");
    let record = env
        .coroutines
        .get(&id)
        .expect("retained coroutine identity");
    let repeated = record.consumed
        || env
            .ownership
            .as_ref()
            .expect("task ownership")
            .groups
            .values()
            .any(|group| group.tasks.contains_key(&id));
    let active = env
        .ownership
        .as_ref()
        .expect("task ownership")
        .groups
        .contains_key(&group_id);
    let active_possible = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.groups.get(&group_id))
        .is_some_and(|group| group.active_possible);
    if !active {
        if env.ownership.as_ref().is_some_and(|ownership| {
            ownership.mode.includes_advanced_contracts()
                && ownership.mode.assumes_default_task_factory()
                && !ownership.incompatible_task_factory
        }) {
            let origin = env
                .ownership
                .as_ref()
                .and_then(|ownership| ownership.group_origins.get(&group_id))
                .cloned();
            report(
                result,
                "TP2102",
                "TaskGroup.create_task requires an active group",
                span(file, expression.range),
                origin.into_iter().collect(),
                "definite",
                vec![task_group_model_identity(env)],
            );
        }
        if env.target_python_version == "3.13" {
            env.coroutines
                .get_mut(&id)
                .expect("retained coroutine")
                .consumed = true;
        }
        queue_raise(
            env,
            expression.range,
            Some(AstExpr {
                range: expression.range,
                kind: AstExprKind::Name("RuntimeError".into()),
            }),
        );
        env.alive = false;
        return true;
    }
    let direct_function = matches!(&record.invocation.kind, AstExprKind::Call { function, .. } if expr_name(function).is_some());
    if repeated || !direct_function || record.ambiguous {
        env.ownership
            .as_mut()
            .expect("task ownership")
            .groups
            .get_mut(&group_id)
            .expect("active group")
            .incomplete = true;
        boundary(
            result,
            file,
            expression.range,
            "task scheduling requires a single resolved coroutine function and capture set",
        );
        queue_raise(env, expression.range, None);
        return true;
    }
    if active_possible {
        let ownership = env.ownership.as_ref().expect("task ownership");
        if ownership.mode.includes_advanced_contracts()
            && ownership.mode.assumes_default_task_factory()
            && !ownership.incompatible_task_factory
        {
            let origin = ownership.group_origins.get(&group_id).cloned();
            report(
                result,
                "TP2102",
                "TaskGroup.create_task may run while the group is inactive",
                span(file, expression.range),
                origin.into_iter().collect(),
                "possible",
                vec![task_group_model_identity(env)],
            );
        }
        boundary(
            result,
            file,
            expression.range,
            "TaskGroup active state varies across control-flow paths",
        );
        let mut rejected = env.clone();
        if env.target_python_version == "3.13" {
            if let Some(coroutine) = rejected.coroutines.get_mut(&id) {
                coroutine.consumed = true;
            }
        }
        rejected.alive = true;
        rejected.pending_raises.clear();
        env.pending_raises.push(FlowExit {
            state: rejected,
            range: expression.range,
            kind: CompletionKind::Raise(Some(AstExpr {
                range: expression.range,
                kind: AstExprKind::Name("RuntimeError".into()),
            })),
        });
        if let Some(group) = env
            .ownership
            .as_mut()
            .and_then(|ownership| ownership.groups.get_mut(&group_id))
        {
            group.incomplete = true;
        }
    }
    env.ownership
        .as_mut()
        .expect("task ownership")
        .groups
        .get_mut(&group_id)
        .expect("active group")
        .tasks
        .insert(
            id.clone(),
            ScheduledTask {
                site: span(file, expression.range),
                possible: active_possible,
                cancel_requested: false,
                may_have_started: false,
                cancelled_before_start: false,
            },
        );
    env.ownership.as_mut().expect("task ownership").pending_task = Some((group_id, id));
    true
}

pub(super) fn restore_captures(record: &CoroutineRecord, env: &mut State) {
    for (name, place) in &record.captures {
        if !place.targets.is_empty() || place.unknown {
            env.places.insert(name.clone(), place.targets.clone());
        }
        if place.unknown {
            env.unknown_places.insert(name.clone());
        }
        if place.transferred {
            if let Some(ownership) = env.ownership.as_mut() {
                ownership.transfer_arguments.insert(name.clone());
            }
        }
    }
}

pub(super) fn clear_captures(record: &CoroutineRecord, env: &mut State) {
    for name in record.captures.keys() {
        env.places.remove(name);
        env.unknown_places.remove(name);
        if let Some(ownership) = env.ownership.as_mut() {
            ownership.transfer_arguments.remove(name);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn join_group(
    id: &str,
    range: SourceRange,
    incoming: Option<&CompletionKind>,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let group = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.groups.get(id))
        .cloned();
    let Some(group) = group else {
        boundary(
            result,
            file,
            range,
            "TaskGroup completion identity is unresolved",
        );
        return;
    };
    let aborting = matches!(
        incoming,
        Some(CompletionKind::Raise(_) | CompletionKind::Cancelled)
    );
    let body_exception = incoming.and_then(|kind| completion_exception_name(kind, env));
    let mut interrupted = env.clone();
    interrupted.pending_raises.clear();
    let mut child_errors = BTreeSet::new();
    let mut normal_possible = true;
    for (task_id, task) in &group.tasks {
        let Some(record) = env.coroutines.get(task_id).cloned() else {
            boundary(
                result,
                file,
                range,
                "scheduled coroutine record is missing; capture completion is unknown",
            );
            continue;
        };
        if (task.cancelled_before_start || (aborting && !task.may_have_started))
            && !task.possible
            && !group.incomplete
        {
            for continuation in [&mut *env, &mut interrupted] {
                if let Some(coroutine) = continuation.coroutines.get_mut(task_id) {
                    coroutine.consumed = true;
                }
            }
            continue;
        }
        let mut execution = env.clone();
        execution.pending_raises.clear();
        execution.pending_return_targets = None;
        execution.alive = true;
        execution
            .ownership
            .as_mut()
            .expect("task execution")
            .entering_task = true;
        restore_captures(&record, &mut execution);
        // The parent may already have violated the borrow. The worker's
        // contracted entry is open; TP2006 retains the parent-side witness.
        for object in record
            .captures
            .values()
            .filter(|place| !place.transferred)
            .flat_map(|place| &place.targets)
        {
            if execution
                .objects
                .get(object)
                .is_some_and(|mask| mask & CLOSED != 0)
            {
                execution.objects.insert(object.clone(), OPEN);
            }
        }
        execution
            .coroutines
            .get_mut(task_id)
            .expect("running coroutine is retained")
            .consumed = true;
        execution.awaited_call = Some((record.file_path.clone(), record.invocation.range));
        consume_expr(
            &record.invocation,
            &mut execution,
            file,
            contracts,
            functions,
            result,
            0,
        );
        let exits = take_pending_raises(&mut execution);
        let cancelled = |exit: &FlowExit| {
            matches!(exit.kind, CompletionKind::Cancelled)
                || raised_exception_name(exit) == Some("CancelledError")
        };
        child_errors.extend(
            exits
                .iter()
                .filter(|exit| !cancelled(exit))
                .map(|exit| raised_exception_name(exit).map(str::to_owned)),
        );
        let has_completion = execution.alive || !exits.is_empty();
        let mut normal_outcomes = exits
            .iter()
            .filter(|exit| cancelled(exit))
            .map(|exit| exit.state.clone())
            .collect::<Vec<_>>();
        if execution.alive {
            normal_outcomes.push(execution.clone());
        }
        normal_possible &= !normal_outcomes.is_empty()
            || task.cancel_requested
            || task.possible
            || group.incomplete
            || !has_completion;
        let normal = join_states(normal_outcomes);
        let all_outcomes = join_states(
            exits
                .into_iter()
                .map(|exit| exit.state)
                .chain(execution.alive.then_some(execution))
                .collect(),
        );

        let mut unresolved_capture = group.incomplete || !has_completion;
        for place in record.captures.values() {
            for object in &place.targets {
                if place.transferred {
                    apply_worker_cleanup(&mut interrupted, object, all_outcomes.as_ref(), true);
                    apply_worker_cleanup(
                        env,
                        object,
                        normal.as_ref(),
                        task.cancel_requested || task.possible || group.incomplete,
                    );
                    apply_worker_protocol_state(
                        &mut interrupted,
                        object,
                        all_outcomes.as_ref(),
                        true,
                    );
                    apply_worker_protocol_state(
                        env,
                        object,
                        normal.as_ref(),
                        task.cancel_requested || task.possible || group.incomplete,
                    );
                } else {
                    let escaped = all_outcomes.as_ref().is_none_or(|effects| {
                        effects.escaped.contains(object)
                            || effects.forbidden_borrowed_closes.contains(object)
                            || effects
                                .objects
                                .get(object)
                                .is_some_and(|mask| mask & UNKNOWN != 0)
                    });
                    if escaped {
                        unresolved_capture = true;
                        for continuation in [&mut *env, &mut interrupted] {
                            *continuation.objects.entry(object.clone()).or_default() |= UNKNOWN;
                            continuation.escaped.insert(object.clone());
                            continuation.protocol_unknown.insert(object.clone());
                        }
                    } else {
                        apply_worker_protocol_state(
                            &mut interrupted,
                            object,
                            all_outcomes.as_ref(),
                            true,
                        );
                        apply_worker_protocol_state(
                            env,
                            object,
                            normal.as_ref(),
                            task.cancel_requested || task.possible || group.incomplete,
                        );
                    }
                }
            }
        }
        if unresolved_capture {
            boundary(
                result,
                file,
                range,
                "TaskGroup exit leaves escaped or unresolved capture lifetimes",
            );
        } else {
            for continuation in [&mut *env, &mut interrupted] {
                if let Some(coroutine) = continuation.coroutines.get_mut(task_id) {
                    coroutine.consumed = true;
                }
            }
        }
    }
    if group.incomplete {
        boundary(
            result,
            file,
            range,
            "TaskGroup membership or completion is unresolved",
        );
    }
    env.ownership
        .as_mut()
        .expect("joined group")
        .groups
        .remove(id);
    interrupted
        .ownership
        .as_mut()
        .expect("joined group")
        .groups
        .remove(id);
    if !group.tasks.is_empty() {
        allow_tasks_to_start(env);
        allow_tasks_to_start(&mut interrupted);
    }
    if aborting {
        *env = interrupted;
        match (incoming, body_exception) {
            (_, Some("KeyboardInterrupt" | "SystemExit")) => {}
            (Some(CompletionKind::Cancelled), _) | (_, Some("CancelledError")) => {
                if !child_errors.is_empty() {
                    queue_group_error(env, range, None, &child_errors);
                }
            }
            _ => {
                if body_exception.is_some() {
                    queue_group_error(env, range, body_exception, &child_errors);
                } else {
                    queue_raise(env, range, None);
                }
                env.alive = false;
            }
        }
        return;
    }
    if !group.tasks.is_empty() {
        env.pending_raises.push(FlowExit {
            state: interrupted.clone(),
            range,
            kind: CompletionKind::Cancelled,
        });
    }
    if !child_errors.is_empty() {
        queue_group_error(&mut interrupted, range, None, &child_errors);
        env.pending_raises
            .extend(take_pending_raises(&mut interrupted));
    }
    env.alive = normal_possible;
}

fn queue_group_error(
    env: &mut State,
    range: SourceRange,
    body_exception: Option<&str>,
    child_errors: &BTreeSet<Option<String>>,
) {
    // Special child failures may escape directly. Keep the type open when
    // child exits disagree or the original exception type was unresolved.
    let ordinary_child_errors = child_errors.iter().all(|error| {
        error
            .as_deref()
            .and_then(|name| builtin_exception_match(name, "Exception"))
            == Some(true)
    });
    let group_type = match body_exception {
        Some(name)
            if builtin_exception_match(name, "Exception") == Some(true)
                && ordinary_child_errors =>
        {
            Some("ExceptionGroup")
        }
        Some("BaseException" | "BaseExceptionGroup" | "GeneratorExit") if ordinary_child_errors => {
            Some("BaseExceptionGroup")
        }
        None if !child_errors.is_empty() && ordinary_child_errors => Some("ExceptionGroup"),
        _ => None,
    };
    queue_raise(
        env,
        range,
        group_type.map(|name| AstExpr {
            range,
            kind: AstExprKind::Name(name.into()),
        }),
    );
}

fn apply_worker_cleanup(
    state: &mut State,
    object: &str,
    effects: Option<&State>,
    may_not_start: bool,
) {
    let before = state.objects.get(object).copied().unwrap_or(UNKNOWN);
    let after = effects
        .and_then(|effects| effects.objects.get(object))
        .copied()
        .unwrap_or(UNKNOWN);
    let mask = if may_not_start { before | after } else { after };
    state.objects.insert(object.to_owned(), mask);
    if let Some(effects) = effects {
        if let Some(close) = effects.close_origins.get(object) {
            state.close_origins.insert(object.to_owned(), close.clone());
        }
        if let (Some(own), Some(effects)) = (state.ownership.as_mut(), effects.ownership.as_ref()) {
            own.copy_transfers(effects, object);
        }
    }
}

fn apply_worker_protocol_state(
    state: &mut State,
    object: &str,
    effects: Option<&State>,
    may_not_start: bool,
) {
    let Some(effects) = effects else {
        return;
    };
    let Some(after) = effects.protocol_states.get(object) else {
        if effects.protocol_unknown.contains(object) {
            state.protocol_unknown.insert(object.to_owned());
        }
        return;
    };
    let mut states = if may_not_start {
        state
            .protocol_states
            .get(object)
            .cloned()
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    states.extend(after.iter().cloned());
    state.protocol_states.insert(object.to_owned(), states);
    if effects.protocol_unknown.contains(object) {
        state.protocol_unknown.insert(object.to_owned());
    } else if !may_not_start {
        state.protocol_unknown.remove(object);
    }
}

pub(super) fn allow_tasks_to_start(env: &mut State) {
    if let Some(ownership) = env.ownership.as_mut() {
        for task in ownership
            .groups
            .values_mut()
            .flat_map(|group| group.tasks.values_mut())
        {
            task.may_have_started = true;
        }
    }
}

pub(super) fn returns_task_handle_or_status(expression: &AstExpr, env: &State) -> bool {
    let Some(ownership) = env.ownership.as_ref() else {
        return false;
    };
    let Some((receiver, method, _, _)) = called_method(expression) else {
        return false;
    };
    expr_name(receiver).is_some_and(|name| {
        method == "create_task" && ownership.group_names.contains_key(name)
            || method == "cancel" && ownership.task_names.contains_key(name)
    })
}

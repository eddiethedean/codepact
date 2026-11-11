use super::ownership::{
    escape_task_handles, external_identity, ScheduledTask, TaskGroup, TASK_GROUP,
};
use super::*;

pub(super) fn is_group_context(expression: &AstExpr, env: &State) -> bool {
    matches!(&expression.kind, AstExprKind::Call { function, arguments, keywords }
        if arguments.is_empty() && keywords.is_empty()
            && external_identity(function, env).as_deref() == Some(TASK_GROUP))
}

pub(super) fn enter_group(
    item: &FlowWithItem,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<String> {
    let ownership = env.ownership.as_mut()?;
    if ownership.mode != crate::Ownership::AssumeDefaultTaskFactory
        || ownership.incompatible_task_factory
    {
        boundary(result, file, item.range, "TaskGroup requires the explicit default-task-factory assumption and a compatible runtime");
        return None;
    }
    if item.unsupported_target || item.target.is_none() {
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
    let id = format!(
        "{}:{}:{}",
        file.path,
        item.range.start,
        env.allocation_context.join("/")
    );
    if ownership.groups.contains_key(&id) {
        boundary(
            result,
            file,
            item.range,
            "repeated TaskGroup creation has unresolved instance multiplicity",
        );
        return None;
    }
    let target = item.target.as_ref().expect("validated group target");
    ownership.rebind(target);
    ownership.groups.insert(id.clone(), TaskGroup::default());
    ownership.group_names.insert(target.clone(), id.clone());
    env.places.remove(target);
    env.unknown_places.remove(target);
    env.shadowed_functions.insert(target.clone());
    env.shadowed_classes.insert(target.clone());
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
    if ownership.incompatible_task_factory {
        boundary(
            result,
            file,
            expression.range,
            "TaskGroup operation uses an unresolved task factory",
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
    if !active {
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
    let task_count = env
        .ownership
        .as_ref()
        .expect("task ownership")
        .groups
        .values()
        .map(|group| group.tasks.len())
        .sum::<usize>();
    if task_count >= MAX_PENDING_COROUTINES {
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
            "structured task budget exceeded; capture completion is unknown",
        );
        return true;
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
                possible: false,
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
    aborting: bool,
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
    let mut interrupted = env.clone();
    interrupted.pending_raises.clear();
    let mut child_can_raise = false;
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
        if task.cancelled_before_start && !task.possible && !group.incomplete {
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
            .executing_task = true;
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
        child_can_raise |= exits.iter().any(|exit| !cancelled(exit));
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
                        }
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
        return;
    }
    if !group.tasks.is_empty() {
        env.pending_raises.push(FlowExit {
            state: interrupted.clone(),
            range,
            kind: CompletionKind::Cancelled,
        });
    }
    if child_can_raise {
        env.pending_raises.push(FlowExit {
            state: interrupted,
            range,
            kind: CompletionKind::Raise(None),
        });
    }
    env.alive = normal_possible;
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

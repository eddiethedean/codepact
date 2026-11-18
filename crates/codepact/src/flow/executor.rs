//! ThreadPoolExecutor protocol transitions and worker admission.
use super::ownership::external_identity;
use super::{
    bind_arguments, boundary, called_method, expr_name, queue_raise, report, span, AstExpr,
    AstExprKind, CallKeyword, CompletionKind, FlowExit, FunctionMap, Inspection, SourceFile, Span,
    State,
};

const THREAD_POOL_EXECUTOR: &str = "external:concurrent.futures.ThreadPoolExecutor";
const THREAD_POOL_MODEL: &str = "TPM-EXECUTOR-001@v3.13.16";
const MAX_EXECUTORS: usize = 32;

pub(super) const EXECUTOR_ACCEPTING: u8 = 1;
pub(super) const EXECUTOR_SHUTDOWN: u8 = 2;
pub(super) const EXECUTOR_UNKNOWN: u8 = 4;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ExecutorState {
    pub(super) states: u8,
    pub(super) origin: Span,
    pub(super) has_submissions: bool,
}

pub(super) fn initialize_handle(
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
    let ownership = env.bind_runtime_handle(name);
    ownership.executors.insert(
        id.clone(),
        ExecutorState {
            states: initial,
            origin,
            has_submissions: false,
        },
    );
    ownership.executor_names.insert(name.to_owned(), id);
    true
}

pub(super) fn call(
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

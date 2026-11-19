//! Executor protocol transitions and worker admission.
mod bridges;
pub(super) use bridges::{await_bridge, scheduling_call};

use super::ownership::external_identity;
use super::{
    bind_arguments, boundary, called_method, expr_name, queue_raise, report, span, AstExpr,
    AstExprKind, CallKeyword, CompletionKind, FlowExit, FunctionMap, Inspection, SourceFile, Span,
    State,
};
use super::{process, runtime, work};
use std::collections::BTreeSet;

const THREAD_POOL_EXECUTOR: &str = "external:concurrent.futures.ThreadPoolExecutor";
const PROCESS_POOL_EXECUTOR: &str = "external:concurrent.futures.ProcessPoolExecutor";
const THREAD_POOL_MODEL: &str = "TPM-EXECUTOR-001@v3.13.16";
const MAX_EXECUTORS: usize = 32;

pub(super) fn enter_context(
    name: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let Some(id) = expr_name(expression)
        .and_then(|source| env.ownership.as_ref()?.executor_names.get(source))
        .cloned()
    else {
        return initialize_handle(name, expression, env, file, result);
    };
    env.bind_runtime_handle(name)
        .executor_names
        .insert(name.into(), id);
    true
}

pub(super) fn leave_context(
    name: &str,
    range: super::SourceRange,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let receiver = AstExpr {
        range,
        kind: AstExprKind::Name(name.into()),
    };
    let expression = AstExpr {
        range,
        kind: AstExprKind::Call {
            function: Box::new(AstExpr {
                range,
                kind: AstExprKind::Attribute(Box::new(receiver), "shutdown".into()),
            }),
            arguments: Vec::new(),
            keywords: Vec::new(),
        },
    };
    call(&expression, env, file, functions, result);
}

pub(super) const EXECUTOR_ACCEPTING: u8 = 1;
pub(super) const EXECUTOR_SHUTDOWN: u8 = 2;
pub(super) const EXECUTOR_UNKNOWN: u8 = 4;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ExecutorState {
    pub(super) states: u8,
    pub(super) origin: Span,
    pub(super) has_submissions: bool,
    pub(super) manager_detached: bool,
    process: bool,
}

impl ExecutorState {
    fn kind(&self, id: String) -> work::Kind {
        if self.process {
            work::Kind::ProcessExecutor(id)
        } else {
            work::Kind::Thread(id)
        }
    }

    fn symbol(&self) -> &'static str {
        if self.process {
            "ProcessPoolExecutor"
        } else {
            "ThreadPoolExecutor"
        }
    }

    fn model(&self) -> &'static str {
        if self.process {
            work::BRIDGE_MODEL
        } else {
            THREAD_POOL_MODEL
        }
    }
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
    if !env
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
    {
        return false;
    }
    let process = match external_identity(function, env).as_deref() {
        Some(THREAD_POOL_EXECUTOR) => false,
        Some(PROCESS_POOL_EXECUTOR) if env.models.foundations.is_some() => true,
        _ => return false,
    };
    let constructor_supported = if process {
        process_executor_constructor(arguments, keywords, env, file)
    } else if bridges::profile(env).is_ok() {
        thread_executor_constructor(arguments, keywords, file)
    } else {
        arguments.is_empty() && keywords.is_empty()
    };
    if !process && !constructor_supported {
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
    let supported_pin = if process {
        runtime::use_model(
            "concurrent.futures.ProcessPoolExecutor",
            "construct-explicit-spawn-executor",
            work::BRIDGE_MODEL,
            if constructor_supported {
                Ok(())
            } else {
                Err("process executor requires a bounded worker count and the declared explicit spawn profile")
            },
            file,
            expression.range,
            result,
        )
    } else {
        env.target_python_version.starts_with("3.13")
    };
    let initial = if supported_pin {
        EXECUTOR_ACCEPTING
    } else {
        if !process {
            boundary(
                result,
                file,
                expression.range,
                "ThreadPoolExecutor model is qualified only for CPython 3.13.16",
            );
        }
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
            manager_detached: false,
            process,
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
    if bridges::call(expression, env, file, functions, result) {
        return true;
    }
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
                    &format!("{}.submit requires the Accepting state", snapshot.symbol()),
                    span(file, expression.range),
                    vec![snapshot.origin.clone()],
                    if has_accepting || has_unknown {
                        "possible"
                    } else {
                        "definite"
                    },
                    vec![snapshot.model().into()],
                );
            }
            let captures = has_accepting
                .then(|| {
                    if snapshot.process {
                        process::spawn_executor_worker(arguments, keywords, env, functions)
                            .then(BTreeSet::new)
                    } else {
                        thread_worker(
                            arguments,
                            keywords,
                            env,
                            functions,
                            bridges::profile(env).is_ok(),
                        )
                    }
                })
                .flatten();
            let supported = captures.is_some();
            if has_unknown || has_accepting && !supported {
                boundary(
                    result,
                    file,
                    expression.range,
                    if has_unknown {
                        if snapshot.process {
                            "ProcessPoolExecutor state is unknown at submit"
                        } else {
                            "ThreadPoolExecutor state is unknown at submit"
                        }
                    } else if snapshot.process {
                        "process executor submission requires a resolved spawn worker and bounded plain arguments"
                    } else {
                        "thread executor submission requires a resolved worker and bounded captures"
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
                if snapshot.process || bridges::profile(env).is_err() {
                    if let Some(executor) = exceptional
                        .ownership
                        .as_mut()
                        .and_then(|ownership| ownership.executors.get_mut(&id))
                    {
                        executor.states |= EXECUTOR_UNKNOWN;
                    }
                }
                env.pending_raises.push(FlowExit {
                    state: exceptional,
                    range: expression.range,
                    kind: CompletionKind::Raise(None),
                });
                let outcome = if snapshot.process {
                    work::for_worker(env, &arguments[0], functions)
                } else {
                    bridges::worker_outcome(arguments, keywords, env, functions, result)
                };
                work::register(
                    env,
                    None,
                    snapshot.kind(id.clone()),
                    file,
                    expression.range,
                    outcome,
                    result,
                );
                if !snapshot.process {
                    work::bind_worker_contract(env, &arguments[0], functions);
                    work::capture_pending_bridge(env, captures.unwrap_or_default(), false);
                }
            } else {
                queue_raise(env, expression.range, None);
            }
            true
        }
        "shutdown" => {
            let Some((wait, cancel_queued)) = shutdown_options(arguments, keywords) else {
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
            if snapshot.has_submissions
                && !wait
                && !snapshot.process
                && bridges::profile(env).is_err()
            {
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
                if executor.process && !wait {
                    executor.manager_detached = true;
                }
            }
            let mut exceptional = env.clone();
            exceptional.alive = true;
            exceptional.pending_raises.clear();
            env.pending_raises.push(FlowExit {
                state: exceptional,
                range: expression.range,
                kind: CompletionKind::Raise(None),
            });
            let owner = snapshot.kind(id);
            if cancel_queued {
                work::owner_may_cancel_waiting(env, &owner);
            }
            let joined = wait && next_states == EXECUTOR_SHUTDOWN && !snapshot.manager_detached;
            if joined {
                work::owner_finished(env, &owner);
            }
            if snapshot.process {
                runtime::use_model(
                    "concurrent.futures.ProcessPoolExecutor.shutdown",
                    if joined {
                        "join-manager"
                    } else if snapshot.manager_detached {
                        "manager-already-detached"
                    } else {
                        "shutdown-request"
                    },
                    work::BRIDGE_MODEL,
                    Ok(()),
                    file,
                    expression.range,
                    result,
                );
            }
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

fn thread_worker(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
    functions: &FunctionMap,
    allow_captures: bool,
) -> Option<BTreeSet<String>> {
    let callee = arguments.first().and_then(expr_name)?;
    if env.shadowed_functions.contains(callee) {
        return None;
    }
    let definition = env
        .function_bindings
        .get(callee)
        .and_then(|binding| functions.get(binding))?;
    if definition.is_async || !definition.supported_binding {
        return None;
    }
    let worker_arguments = &arguments[1..];
    let Ok(bindings) = bind_arguments(&definition.signature, worker_arguments, keywords, false)
    else {
        return None;
    };
    let mut captures = BTreeSet::new();
    for name in bindings
        .values()
        .filter_map(|argument| expr_name(argument))
        .chain(
            env.ownership
                .as_ref()?
                .closure_captures(callee)
                .map(String::as_str),
        )
    {
        if env.unknown_places.contains(name) || env.possibly_unbound_places.contains(name) {
            return None;
        }
        if let Some(objects) = env.places.get(name) {
            if !allow_captures || objects.len() != 1 {
                return None;
            }
            captures.extend(objects.iter().cloned());
        }
    }
    (captures.len() <= 8).then_some(captures)
}

fn bounded_workers(argument: &AstExpr, file: &SourceFile) -> bool {
    matches!(argument.kind, AstExprKind::Primitive)
        && file
            .source
            .get(argument.range.start..argument.range.end)
            .and_then(|literal| literal.parse::<usize>().ok())
            .is_some_and(|count| (1..=3).contains(&count))
}

fn thread_executor_constructor(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    file: &SourceFile,
) -> bool {
    match (arguments, keywords) {
        ([], []) => true,
        ([count], []) => bounded_workers(count, file),
        ([], [(Some(name), count)]) => name == "max_workers" && bounded_workers(count, file),
        _ => false,
    }
}

fn process_executor_constructor(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
    file: &SourceFile,
) -> bool {
    if arguments.len() > 1 {
        return false;
    }
    let mut count = arguments.first();
    let mut context = None;
    for (name, value) in keywords {
        match name.as_deref() {
            Some("max_workers") if count.is_none() => count = Some(value),
            Some("mp_context") if context.is_none() => context = Some(value),
            _ => return false,
        }
    }
    count.is_some_and(|count| bounded_workers(count, file))
        && context.is_some_and(|context| process::spawn_executor_context(context, env))
}

fn shutdown_options(arguments: &[AstExpr], keywords: &[CallKeyword]) -> Option<(bool, bool)> {
    if arguments.len() > 1
        || keywords
            .iter()
            .any(|(name, _)| !matches!(name.as_deref(), Some("wait" | "cancel_futures")))
    {
        return None;
    }
    let mut wait = true;
    let mut cancel_queued = false;
    if let Some(argument) = arguments.first() {
        let AstExprKind::Boolean(value) = argument.kind else {
            return None;
        };
        wait = value;
    }
    let mut saw_wait = !arguments.is_empty();
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
                let AstExprKind::Boolean(value) = value.kind else {
                    return None;
                };
                if saw_cancel {
                    return None;
                }
                cancel_queued = value;
                saw_cancel = true;
            }
            _ => return None,
        }
    }
    Some((wait, cancel_queued))
}

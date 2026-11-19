//! Entry, exit partitions, and cleanup for context managers.
use super::{
    async_protocols, async_resources, asyncio, boundary, call_parts, consume_expr, execute,
    execution::{execute_non_if_statement, push_exit},
    expr_name, join_states, library, merge, ownership, process, queue_raise, runtime,
    synthetic_name, take_flow_step, take_pending_raises, tasks, weaken, AstExpr, AstExprKind,
    CompletionKind, Contract, FlowExit, FlowResult, FlowStatement, FlowWithItem, FunctionMap,
    Inspection, SourceFile, SourceRange, State, MAX_EXIT_PARTITIONS, UNKNOWN,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum CleanupMethod {
    Close,
    Aclose,
}

impl CleanupMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Aclose => "aclose",
        }
    }

    fn asynchronous(self) -> bool {
        matches!(self, Self::Aclose)
    }

    fn call(self, receiver: &str, range: SourceRange) -> AstExpr {
        let receiver = AstExpr {
            range,
            kind: AstExprKind::Name(receiver.to_owned()),
        };
        let call = AstExpr {
            range,
            kind: AstExprKind::Call {
                function: Box::new(AstExpr {
                    range,
                    kind: AstExprKind::Attribute(Box::new(receiver), self.name().to_owned()),
                }),
                arguments: Vec::new(),
                keywords: Vec::new(),
            },
        };
        if self.asynchronous() {
            AstExpr {
                range,
                kind: AstExprKind::Await(Box::new(call)),
            }
        } else {
            call
        }
    }
}

#[derive(Clone)]
enum ContextCleanup {
    Method {
        receiver: String,
        method: CleanupMethod,
    },
    TaskGroup(String),
    Pool(String),
    Executor(String),
    Timeout(String),
    AsyncProtocol(String),
    AsyncGenerator(String),
    ProcessGuard(String),
    Library {
        receiver: String,
        context: library::Context,
    },
}

#[derive(Clone)]
struct ModeledCleanup {
    action: ContextCleanup,
    range: SourceRange,
}

#[derive(Clone)]
enum ContextCleanupFrame {
    Modeled(ModeledCleanup),
    Unknown {
        asynchronous: bool,
        range: SourceRange,
    },
}

impl ContextCleanupFrame {
    fn modeled(action: ContextCleanup, range: SourceRange) -> Self {
        Self::Modeled(ModeledCleanup { action, range })
    }

    fn range(&self) -> SourceRange {
        match self {
            Self::Modeled(cleanup) => cleanup.range,
            Self::Unknown { range, .. } => *range,
        }
    }
}

impl ModeledCleanup {
    fn run(
        &self,
        incoming: Option<&CompletionKind>,
        env: &mut State,
        file: &SourceFile,
        contracts: &[Contract],
        functions: &FunctionMap,
        result: &mut Inspection,
    ) {
        let range = self.range;
        match &self.action {
            ContextCleanup::TaskGroup(id) => {
                tasks::join_group(id, range, incoming, env, file, contracts, functions, result);
            }
            ContextCleanup::Pool(receiver) => {
                process::finish_pool(receiver, env, file, range, result)
            }
            ContextCleanup::Executor(receiver) => {
                super::executor::leave_context(receiver, range, env, file, functions, result)
            }
            ContextCleanup::Timeout(id) => asyncio::leave_timeout(id, env, range, incoming),
            ContextCleanup::AsyncProtocol(id) => {
                async_protocols::leave(id, env, file, range, result)
            }
            ContextCleanup::AsyncGenerator(id) => {
                async_resources::context_exit(id, incoming, env, file, range, functions, result)
            }
            ContextCleanup::ProcessGuard(receiver) => {
                process::leave_guard(receiver, env, file, range, result)
            }
            ContextCleanup::Library { receiver, context } => {
                context.exit(receiver, incoming, env, file, range, result);
            }
            ContextCleanup::Method { receiver, method } => {
                let call = method.call(receiver, range);
                consume_expr(&call, env, file, contracts, functions, result, 0);
            }
        }
    }
}

fn contextlib_cleanup_method(expression: &AstExpr, env: &State) -> Option<CleanupMethod> {
    if !matches!(env.target_python_version.as_str(), "3.11" | "3.12" | "3.13") {
        return None;
    }
    let AstExprKind::Call { function, .. } = &expression.kind else {
        return None;
    };
    match &function.kind {
        AstExprKind::Name(name) if !env.shadowed_functions.contains(name) => {
            match env.function_bindings.get(name).map(String::as_str) {
                Some("external:contextlib.closing") => Some(CleanupMethod::Close),
                Some("external:contextlib.aclosing") => Some(CleanupMethod::Aclose),
                _ => None,
            }
        }
        AstExprKind::Attribute(module, name) => {
            let module_binding = expr_name(module)
                .filter(|module| !env.shadowed_functions.contains(*module))
                .and_then(|module| env.function_bindings.get(module))
                .map(String::as_str);
            match (module_binding, name.as_str()) {
                (Some("external:contextlib"), "closing") => Some(CleanupMethod::Close),
                (Some("external:contextlib"), "aclosing") => Some(CleanupMethod::Aclose),
                _ => None,
            }
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_with(
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
    if asynchronous
        && env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
        && !env.exclusive.is_empty()
    {
        boundary(
            result,
            file,
            range,
            "exclusive borrow remains active across async context entry or exit",
        );
        for object in &env.exclusive {
            *env.objects.entry(object.clone()).or_default() |= UNKNOWN;
            if env
                .resources
                .get(object)
                .is_some_and(|resource| env.protocol_machines.contains_key(&resource.base_symbol))
            {
                env.protocol_unknown.insert(object.clone());
            }
        }
    }
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
            Some(CompletionKind::Cancelled(_)) => 3,
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
            (
                Some((CompletionKind::Cancelled(previous), _)),
                Some((CompletionKind::Cancelled(incoming), _)),
            ) if *previous != incoming => {
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
        let initial_cleanup_range = cleanups[remaining - 1].range();
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
            let cleanup_range = cleanup.range();
            if !take_flow_step(&mut path.state, file, cleanup_range, result) {
                completed.push(path);
                continue;
            }
            match cleanup {
                ContextCleanupFrame::Modeled(cleanup) => {
                    path.state.alive = true;
                    path.state.pending_raises.clear();
                    cleanup.run(
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
                                completion: Some((CompletionKind::Raise(None), cleanup_range)),
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
                        Some(CompletionKind::Raise(_) | CompletionKind::Cancelled(_))
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
                                completion: Some((CompletionKind::Cancelled(None), range)),
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
        if asynchronous {
            let deadline = format!("{}:{}:{index}", file.path, item.range.start);
            if let Some(active) = asyncio::enter_timeout(item, &deadline, env, file, result) {
                if active {
                    cleanups.push(ContextCleanupFrame::modeled(
                        ContextCleanup::Timeout(deadline),
                        item.range,
                    ));
                }
                continue;
            }
        }
        if asynchronous && env.ownership.is_some() && tasks::is_group_context(&item.context, env) {
            if let Some(id) = tasks::enter_group(item, env, file, result) {
                cleanups.push(ContextCleanupFrame::modeled(
                    ContextCleanup::TaskGroup(id),
                    item.range,
                ));
                continue;
            }
        }
        if asynchronous {
            if let Some(generator) =
                async_resources::context_enter(item, env, file, contracts, functions, result)
            {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                if let Some(id) = generator {
                    cleanups.push(ContextCleanupFrame::modeled(
                        ContextCleanup::AsyncGenerator(id),
                        item.range,
                    ));
                }
                continue;
            }
            let inline = async_protocols::is_constructor(&item.context, env).then(|| {
                let receiver = synthetic_name(env, "async-guard", item.range, index);
                execute_non_if_statement(
                    &FlowStatement::Assign {
                        name: receiver.clone(),
                        annotation: None,
                        value: item.context.clone(),
                        range: item.context.range,
                    },
                    env,
                    file,
                    contracts,
                    functions,
                    result,
                );
                AstExpr {
                    range: item.context.range,
                    kind: AstExprKind::Name(receiver),
                }
            });
            if !env.alive {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                break;
            }
            let manager = inline.as_ref().unwrap_or(&item.context);
            if let Some(id) = async_protocols::enter(manager, true, env, file, result) {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                if env.alive {
                    if let Some(target) = &item.target {
                        env.bind_runtime_handle(target);
                    }
                    if item.unsupported_target {
                        boundary(
                            result,
                            file,
                            item.range,
                            "async guard context target is outside the local subset",
                        );
                    }
                    cleanups.push(ContextCleanupFrame::modeled(
                        ContextCleanup::AsyncProtocol(id),
                        item.range,
                    ));
                }
                continue;
            }
            if inline.is_some() {
                boundary(
                    result,
                    file,
                    item.range,
                    "the asyncio context entry or cleanup effect is unresolved",
                );
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                cleanups.push(ContextCleanupFrame::Unknown {
                    asynchronous,
                    range: item.range,
                });
                continue;
            }
        } else {
            let receiver = synthetic_name(env, "process-guard", item.range, index);
            if let Some(active) =
                process::enter_guard(&receiver, &item.context, env, file, item.range, result)
            {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                if active {
                    if let Some(target) = &item.target {
                        env.bind_runtime_handle(target);
                    }
                    if item.unsupported_target {
                        boundary(
                            result,
                            file,
                            item.range,
                            "process guard context target is outside the local subset",
                        );
                    }
                    cleanups.push(ContextCleanupFrame::modeled(
                        ContextCleanup::ProcessGuard(receiver),
                        item.range,
                    ));
                }
                continue;
            }
            let executor = synthetic_name(env, "executor-context", item.range, index);
            if runtime::runtime_profile(env)
                .is_ok_and(|profile| profile.applicability(&env.target_python_version).is_ok())
                && super::executor::enter_context(&executor, &item.context, env, file, result)
            {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                if let Some(target) = &item.target {
                    let source = AstExpr {
                        range: item.range,
                        kind: AstExprKind::Name(executor.clone()),
                    };
                    super::executor::enter_context(target, &source, env, file, result);
                }
                if item.unsupported_target {
                    boundary(
                        result,
                        file,
                        item.range,
                        "executor context target is outside the qualified local subset",
                    );
                }
                cleanups.push(ContextCleanupFrame::modeled(
                    ContextCleanup::Executor(executor),
                    item.range,
                ));
                continue;
            }
            let receiver = synthetic_name(env, "pool-context", item.range, index);
            if let Some(active) = process::enter_pool(
                &receiver,
                &item.context,
                env,
                file,
                item.range,
                functions,
                result,
            ) {
                entry_raises.extend(
                    take_pending_raises(env)
                        .into_iter()
                        .map(|exit| (exit, cleanups.len())),
                );
                if active {
                    if item.unsupported_target {
                        boundary(
                            result,
                            file,
                            item.range,
                            "Pool context target destructuring is not modeled",
                        );
                    }
                    if let Some(target) = &item.target {
                        process::alias_pool(env, target, &receiver);
                    }
                    cleanups.push(ContextCleanupFrame::modeled(
                        ContextCleanup::Pool(receiver),
                        item.range,
                    ));
                }
                continue;
            }
        }
        if let Some(context) = library::context(&item.context, asynchronous, env) {
            let receiver = synthetic_name(env, "library-context", item.range, index);
            if context.special() {
                if let Some((_, arguments, keywords)) = call_parts(&item.context) {
                    for argument in arguments
                        .iter()
                        .chain(keywords.iter().map(|(_, value)| value))
                    {
                        consume_expr(argument, env, file, contracts, functions, result, 0);
                    }
                }
                if env.alive {
                    context.enter_special(&item.context, env, file, result);
                    if let Some(targets) = env.pending_return_targets.take() {
                        env.places.insert(receiver.clone(), targets);
                    }
                }
            } else {
                execute_non_if_statement(
                    &FlowStatement::Assign {
                        name: receiver.clone(),
                        annotation: None,
                        value: item.context.clone(),
                        range: item.context.range,
                    },
                    env,
                    file,
                    contracts,
                    functions,
                    result,
                );
            }
            if env.alive {
                context.enter(&receiver, env, file, item.range, result);
            }
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            if !env.alive {
                break;
            }
            if let Some(targets) = env
                .places
                .get(&receiver)
                .cloned()
                .filter(|_| !env.unknown_places.contains(&receiver))
            {
                if item.unsupported_target {
                    boundary(
                        result,
                        file,
                        item.range,
                        "reviewed context target destructuring is unsupported",
                    );
                }
                if let Some(target) = &item.target {
                    env.places.insert(target.clone(), targets);
                    ownership::assign_handle(target, None, false, env, file, item.range, result);
                    env.possibly_unbound_places.remove(target);
                    env.unknown_places.remove(target);
                    env.definitely_bound_names.insert(target.clone());
                    env.shadowed_functions.insert(target.clone());
                    env.shadowed_classes.insert(target.clone());
                }
                cleanups.push(ContextCleanupFrame::modeled(
                    ContextCleanup::Library { receiver, context },
                    item.range,
                ));
            } else {
                boundary(
                    result,
                    file,
                    item.range,
                    "reviewed context receiver is unresolved",
                );
                cleanups.push(ContextCleanupFrame::Unknown {
                    asynchronous,
                    range: item.range,
                });
            }
            continue;
        }
        let file_context = (!asynchronous)
            .then(|| runtime::TextFileContext::resolve(&item.context, env))
            .flatten();
        let Some(method) = contextlib_cleanup_method(&item.context, env)
            .filter(|method| asynchronous == method.asynchronous())
            .or(file_context.is_some().then_some(CleanupMethod::Close))
        else {
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
                            kind: CompletionKind::Cancelled(None),
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

        let (arguments, keywords) = call_parts(&item.context)
            .map_or((&[][..], &[][..]), |(_, args, keywords)| (args, keywords));
        let object = if file_context.is_some() {
            Some(&item.context)
        } else {
            match (arguments, keywords) {
                ([object], []) => Some(object),
                ([], [(Some(name), object)]) if name == "thing" => Some(object),
                _ => None,
            }
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
        if let Some(context) = file_context {
            context.enter(env, item.range);
            entry_raises.extend(
                take_pending_raises(env)
                    .into_iter()
                    .map(|exit| (exit, cleanups.len())),
            );
            if !env.alive {
                break;
            }
        }
        if !env.places.contains_key(&receiver) || env.unknown_places.contains(&receiver) {
            uncertain.extend(env.places.values().flatten().cloned());
            boundary(
                result,
                file,
                item.range,
                "context cleanup receiver is not a resolved tracked resource",
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
            cleanups.push(ContextCleanupFrame::modeled(
                ContextCleanup::Method { receiver, method },
                item.range,
            ));
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

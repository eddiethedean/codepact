use super::*;

#[derive(Clone)]
pub(in crate::flow) struct GeneratorDefinition {
    prefix: Vec<FlowStatement>,
    setup: Vec<FlowStatement>,
    resume: Vec<FlowStatement>,
    cleanup: Vec<FlowStatement>,
    suffix: Vec<FlowStatement>,
    yield_value: Option<AstExpr>,
    suspension: SourceRange,
    pub(in crate::flow) context: bool,
}

fn direct_yield(statement: &FlowStatement) -> Option<(&Option<Box<AstExpr>>, SourceRange)> {
    match statement {
        FlowStatement::Expression {
            value:
                AstExpr {
                    kind: AstExprKind::Yield(value),
                    range,
                },
            ..
        } => Some((value, *range)),
        _ => None,
    }
}

fn contains_yield(statements: &[FlowStatement]) -> bool {
    statements.iter().any(|statement| {
        direct_yield(statement).is_some()
            || match statement {
                FlowStatement::Try {
                    body,
                    handlers,
                    alternate,
                    finalbody,
                    ..
                } => {
                    contains_yield(body)
                        || contains_yield(alternate)
                        || contains_yield(finalbody)
                        || handlers.iter().any(|handler| contains_yield(&handler.body))
                }
                FlowStatement::If {
                    body, alternate, ..
                }
                | FlowStatement::Loop {
                    body, alternate, ..
                } => contains_yield(body) || contains_yield(alternate),
                FlowStatement::With { body, .. } => contains_yield(body),
                FlowStatement::Match { cases, .. } => {
                    cases.iter().any(|case| contains_yield(&case.body))
                }
                FlowStatement::Assign { value, .. }
                | FlowStatement::Store { value, .. }
                | FlowStatement::EscapingAssign { value, .. } => {
                    matches!(value.kind, AstExprKind::Yield(_))
                }
                _ => false,
            }
    })
}

pub(in crate::flow) fn definition(
    body: &[FlowStatement],
    decorators: &[AstExpr],
    declarations: &[Declaration],
    supplied_modules: &BTreeSet<String>,
) -> Option<GeneratorDefinition> {
    let context = match decorators {
        [] => false,
        [decorator] => {
            let env = State {
                function_bindings: external_module_bindings(
                    declarations,
                    &BTreeMap::new(),
                    supplied_modules,
                ),
                ..State::default()
            };
            if ownership::external_identity(decorator, &env).as_deref()
                != Some("external:contextlib.asynccontextmanager")
            {
                return None;
            }
            true
        }
        _ => return None,
    };
    for (index, statement) in body.iter().enumerate() {
        let (prefix, suffix) = (&body[..index], &body[index + 1..]);
        if let Some((yield_value, suspension)) = direct_yield(statement) {
            if !contains_yield(prefix) && !contains_yield(suffix) {
                return Some(GeneratorDefinition {
                    prefix: prefix.to_vec(),
                    setup: Vec::new(),
                    resume: Vec::new(),
                    cleanup: Vec::new(),
                    suffix: suffix.to_vec(),
                    yield_value: yield_value.as_deref().cloned(),
                    suspension,
                    context,
                });
            }
            return None;
        }
        if let FlowStatement::Try {
            body: inside,
            handlers,
            alternate,
            finalbody,
            ..
        } = statement
        {
            if handlers.is_empty() && alternate.is_empty() {
                if let Some((position, (yield_value, suspension))) = inside
                    .iter()
                    .enumerate()
                    .find_map(|(position, item)| direct_yield(item).map(|site| (position, site)))
                {
                    let (setup, resume) = (&inside[..position], &inside[position + 1..]);
                    if [prefix, setup, resume, finalbody, suffix]
                        .into_iter()
                        .any(contains_yield)
                    {
                        return None;
                    }
                    return Some(GeneratorDefinition {
                        prefix: prefix.to_vec(),
                        setup: setup.to_vec(),
                        resume: resume.to_vec(),
                        cleanup: finalbody.to_vec(),
                        suffix: suffix.to_vec(),
                        yield_value: yield_value.as_deref().cloned(),
                        suspension,
                        context,
                    });
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Fresh,
    Suspended,
    Finished,
    Uncertain,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Generator {
    key: String,
    frame: Option<Box<State>>,
    captured: BTreeSet<String>,
    stage: Stage,
    origin: Span,
    context: bool,
}

impl Generator {
    pub(super) fn make_uncertain(&mut self) {
        if self.stage != Stage::Finished {
            self.stage = Stage::Uncertain;
            self.frame = None;
        }
    }

    pub(super) fn merge(&mut self, other: &Self) {
        if self.stage != other.stage || self.frame != other.frame || self.key != other.key {
            self.stage = Stage::Uncertain;
            self.frame = None;
        }
        self.captured.extend(other.captured.iter().cloned());
    }

    pub(super) fn capture_site(&self, object: &str) -> Option<(Span, bool)> {
        (self.stage != Stage::Finished && self.captured.contains(object))
            .then(|| (self.origin.clone(), self.stage == Stage::Uncertain))
    }

    pub(super) fn report_abandonment(&self, result: &mut Inspection) {
        if self.stage == Stage::Suspended
            || self.stage == Stage::Uncertain
            || (self.stage == Stage::Fresh && !self.captured.is_empty())
        {
            boundary_at_span(
                result,
                self.origin.clone(),
                "abandoned async generator has no established explicit close or exhaustion",
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Next,
    Close,
}

fn construct(
    expression: &AstExpr,
    definition: &FunctionBody,
    plan: &GeneratorDefinition,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let (_, args, keywords) = call_parts(expression).expect("resolved generator call");
    let range = expression.range;
    let selection = applicable(env).and_then(|()| {
        if env.async_resources.generators.len() >= MAX_HANDLES {
            return Err("async-generator identity budget exceeded");
        }
        if definition
            .parameter_contracts
            .values()
            .any(|contract| contract.access != "shared")
        {
            return Err("async-generator captures require a known borrowed or plain argument");
        }
        Ok(())
    });
    if !runtime::use_model(
        if plan.context {
            "contextlib.asynccontextmanager"
        } else {
            GENERATOR_HANDLE
        },
        "construct-lazy-frame",
        GENERATOR_MODEL,
        selection,
        file,
        range,
        result,
    ) {
        queue_raise(env, range, None);
        return;
    }
    let bindings = match bind_arguments(&definition.signature, args, keywords, false) {
        Ok(bindings) => bindings,
        Err(failure) => {
            binding_failure(failure, env, file, range, result);
            return;
        }
    };
    let (module_functions, module_classes) = module_bindings_for_child(env, definition);
    let mut child = State {
        ownership: OwnershipState::for_child(env, false),
        alive: true,
        async_context: true,
        scope_module: definition.file.module.clone(),
        call_depth: env.call_depth.saturating_add(1),
        models: env.models.clone(),
        target_python_version: env.target_python_version.clone(),
        function_bindings: module_functions.clone(),
        function_bindings_complete: true,
        class_bindings: module_classes.clone(),
        class_bindings_complete: true,
        module_function_bindings: module_functions,
        module_class_bindings: module_classes,
        allocation_context: env.allocation_context.clone(),
        ..State::default()
    };
    child
        .allocation_context
        .push(format!("{}:{}", file.path, range.start));
    child
        .shadowed_functions
        .extend(definition.parameters.iter().cloned());
    child
        .shadowed_functions
        .extend(assigned_names(&definition.statements));
    child
        .shadowed_classes
        .extend(child.shadowed_functions.iter().cloned());
    child
        .definitely_bound_names
        .extend(definition.parameters.iter().cloned());
    let mut captured = BTreeSet::new();
    for (name, argument) in bindings {
        let Some(actual) = expr_name(argument) else {
            continue;
        };
        if env.unknown_places.contains(actual) || env.possibly_unbound_places.contains(actual) {
            boundary(
                result,
                file,
                range,
                "async-generator argument capture is unresolved",
            );
            queue_raise(env, range, None);
            return;
        }
        if let Some(objects) = env.places.get(actual) {
            if objects.iter().any(|id| env.exclusive.contains(id)) {
                boundary(
                    result,
                    file,
                    range,
                    "exclusive borrow cannot be captured across generator suspension",
                );
            }
            captured.extend(objects.iter().cloned());
            child.places.insert(name, objects.clone());
        }
    }
    copy_object_metadata(&mut child, env, &captured);
    for id in &captured {
        child.borrowed.insert(id.clone());
        child.duty_origins.remove(id);
    }
    let Some(id) = allocate(
        GENERATOR_HANDLE,
        "aclose",
        GENERATOR_MODEL,
        false,
        env,
        file,
        range,
        result,
    ) else {
        return;
    };
    if let Some(existing) = env.async_resources.generators.get_mut(&id) {
        existing.captured.extend(captured);
        existing.make_uncertain();
        existing.stage = Stage::Uncertain;
        boundary(
            result,
            file,
            range,
            "repeated generator construction cannot distinguish instances",
        );
    } else {
        env.async_resources.generators.insert(
            id,
            Generator {
                key: definition
                    .binding_key
                    .clone()
                    .expect("qualified generator binding"),
                frame: Some(Box::new(child)),
                captured,
                stage: Stage::Fresh,
                origin: span(file, range),
                context: plan.context,
            },
        );
    }
}

fn anext(function: &AstExpr, env: &State) -> bool {
    ownership::external_identity(function, env).as_deref() == Some("external:builtins.anext")
        || (expr_name(function) == Some("anext")
            && !env.shadowed_functions.contains("anext")
            && !env.function_bindings.contains_key("anext")
            && !env.definitely_bound_names.contains("anext"))
}

fn expression_generator(receiver: &AstExpr, env: &State) -> Option<String> {
    object_for(receiver, env).filter(|id| env.async_resources.generators.contains_key(id))
}

pub(super) fn is_operation(expression: &AstExpr, env: &State) -> bool {
    call_parts(expression).is_some_and(|(function, _, _)| anext(function, env))
        || called_method(expression).is_some_and(|(receiver, method, _, _)| {
            matches!(method, "aclose" | "__anext__")
                && expression_generator(receiver, env).is_some()
        })
}

enum Progress {
    Yielded(BTreeSet<String>),
    Finished,
    Interrupted,
}

fn copy_frame_effects(parent: &mut State, child: &State) {
    let duties = child.duty_origins.keys().cloned().collect::<BTreeSet<_>>();
    for object in child.objects.keys() {
        apply_call_object_effects(parent, child, &duties, object);
    }
}

fn propagate(
    env: &mut State,
    id: &str,
    exits: impl IntoIterator<Item = FlowExit>,
    file: &SourceFile,
    range: SourceRange,
) {
    for exit in exits {
        let mut state = env.clone();
        state.alive = true;
        state.pending_raises.clear();
        copy_frame_effects(&mut state, &exit.state);
        if let Some(generator) = state.async_resources.generators.get_mut(id) {
            generator.stage = Stage::Finished;
            generator.frame = None;
        }
        finish(&mut state, id, file, range);
        let kind = match exit.kind {
            CompletionKind::Cancelled(deadline) => CompletionKind::Cancelled(deadline),
            CompletionKind::Raise(exception) => CompletionKind::Raise(exception),
            _ => CompletionKind::Raise(None),
        };
        env.pending_raises.push(FlowExit { state, kind, range });
    }
}

fn step(
    id: &str,
    operation: Step,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Progress {
    let Some(snapshot) = env.async_resources.generators.get(id).cloned() else {
        return Progress::Interrupted;
    };
    let effect = match operation {
        Step::Next => "advance-frame",
        Step::Close => "finalize-frame",
    };
    let selection = applicable(env).and_then(|()| {
        (snapshot.stage != Stage::Uncertain
            && !env.escaped.contains(id)
            && env.objects.get(id).is_none_or(|mask| mask & UNKNOWN == 0))
        .then_some(())
        .ok_or("async-generator identity or suspension state differs across paths")
    });
    if !runtime::use_model(
        GENERATOR_HANDLE,
        effect,
        GENERATOR_MODEL,
        selection,
        file,
        range,
        result,
    ) {
        queue_raise(env, range, None);
        return Progress::Interrupted;
    }
    if snapshot.stage == Stage::Finished {
        return Progress::Finished;
    }
    if snapshot.stage == Stage::Fresh && operation == Step::Close {
        let generator = env
            .async_resources
            .generators
            .get_mut(id)
            .expect("tracked generator");
        generator.stage = Stage::Finished;
        generator.frame = None;
        finish(env, id, file, range);
        return Progress::Finished;
    }
    let Some(definition) = functions.get(&snapshot.key) else {
        boundary(
            result,
            file,
            range,
            "async-generator definition disappeared from the analyzed scope",
        );
        return Progress::Interrupted;
    };
    let plan = definition
        .async_generator
        .as_ref()
        .expect("tracked generator definition");
    let mut child = *snapshot.frame.expect("live generator frame");
    for object in &snapshot.captured {
        if let Some(mask) = env.objects.get(object) {
            child.objects.insert(object.clone(), *mask);
        }
    }
    if let (Some(child_ownership), Some(parent_ownership)) = (&mut child.ownership, &env.ownership)
    {
        child_ownership.active_timeouts = parent_ownership.active_timeouts.clone();
    }
    let definition_file = &definition.file;
    let contracts = &definition.contracts;
    if snapshot.stage == Stage::Fresh {
        let before = execute(
            &plan.prefix,
            &mut child,
            definition_file,
            contracts,
            functions,
            result,
            0,
        );
        propagate(env, id, before.raises, file, range);
        if !before.returns.is_empty() {
            boundary(
                result,
                file,
                range,
                "async-generator entry may return before its known yield",
            );
        }
        if child.alive {
            let setup = execute(
                &plan.setup,
                &mut child,
                definition_file,
                contracts,
                functions,
                result,
                0,
            );
            for mut failure in setup.raises {
                let cleanup = execute(
                    &plan.cleanup,
                    &mut failure.state,
                    definition_file,
                    contracts,
                    functions,
                    result,
                    0,
                );
                propagate(env, id, cleanup.raises, file, range);
                if failure.state.alive {
                    propagate(env, id, [failure], file, range);
                }
            }
            if !setup.returns.is_empty() {
                boundary(
                    result,
                    file,
                    range,
                    "async-generator entry may leave before its known yield",
                );
            }
        }
        if !child.alive {
            env.alive = false;
            return Progress::Interrupted;
        }
        if !child.exclusive.is_empty() {
            boundary(
                result,
                definition_file,
                plan.suspension,
                "exclusive borrow remains active across generator suspension",
            );
        }
        let yielded = plan
            .yield_value
            .as_ref()
            .and_then(|value| {
                let name = expr_name(value)?;
                child.places.get(name).cloned()
            })
            .unwrap_or_default();
        if plan.yield_value.as_ref().is_some_and(|value| {
            !matches!(
                value.kind,
                AstExprKind::String(_)
                    | AstExprKind::Primitive
                    | AstExprKind::Boolean(_)
                    | AstExprKind::NoneLiteral
                    | AstExprKind::Name(_)
            )
        }) || (!plan.context && !yielded.is_empty())
        {
            boundary(
                result,
                definition_file,
                plan.suspension,
                "async-generator yield value is outside the reviewed plain-value subset",
            );
        }
        copy_frame_effects(env, &child);
        let generator = env
            .async_resources
            .generators
            .get_mut(id)
            .expect("tracked generator");
        generator.stage = Stage::Suspended;
        generator.frame = Some(Box::new(child));
        Progress::Yielded(yielded)
    } else {
        let statements = FlowStatement::Try {
            body: if operation == Step::Next {
                plan.resume.clone()
            } else {
                Vec::new()
            },
            handlers: Vec::new(),
            alternate: Vec::new(),
            finalbody: plan.cleanup.clone(),
            range: plan.suspension,
        };
        let cleanup = execute(
            std::slice::from_ref(&statements),
            &mut child,
            definition_file,
            contracts,
            functions,
            result,
            0,
        );
        propagate(env, id, cleanup.raises, file, range);
        let normal = join_states(
            cleanup
                .returns
                .into_iter()
                .map(|exit| exit.state)
                .chain(child.alive.then(|| child.clone()))
                .collect(),
        );
        let Some(mut child) = normal else {
            env.alive = false;
            return Progress::Interrupted;
        };
        if operation == Step::Next {
            let suffix = execute(
                &plan.suffix,
                &mut child,
                definition_file,
                contracts,
                functions,
                result,
                0,
            );
            propagate(env, id, suffix.raises, file, range);
            let normal = join_states(
                suffix
                    .returns
                    .into_iter()
                    .map(|exit| exit.state)
                    .chain(child.alive.then(|| child.clone()))
                    .collect(),
            );
            let Some(normal) = normal else {
                env.alive = false;
                return Progress::Interrupted;
            };
            child = normal;
        }
        copy_frame_effects(env, &child);
        let generator = env
            .async_resources
            .generators
            .get_mut(id)
            .expect("tracked generator");
        generator.stage = Stage::Finished;
        generator.frame = None;
        finish(env, id, file, range);
        Progress::Finished
    }
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    let Some((function, arguments, keywords)) = call_parts(expression) else {
        return false;
    };
    if let Some(definition) = resolved_function(function, env, functions) {
        if let Some(plan) = &definition.async_generator {
            construct(expression, definition, plan, env, file, result);
            return true;
        }
    }
    let (receiver, operation) = if anext(function, env) {
        match (arguments, keywords) {
            ([receiver], []) => (receiver, Step::Next),
            _ => {
                boundary(
                    result,
                    file,
                    expression.range,
                    "anext requires one resolved local async generator",
                );
                queue_raise(env, expression.range, None);
                return true;
            }
        }
    } else if let Some((receiver, method @ ("aclose" | "__anext__"), [], [])) =
        called_method(expression)
    {
        (
            receiver,
            if method == "aclose" {
                Step::Close
            } else {
                Step::Next
            },
        )
    } else {
        return false;
    };
    let Some(id) = expression_generator(receiver, env) else {
        if anext(function, env) {
            boundary(
                result,
                file,
                expression.range,
                "anext cannot establish a unique local async-generator frame",
            );
            queue_raise(env, expression.range, None);
            return true;
        }
        return false;
    };
    if env.async_resources.generators[&id].context || !awaited(expression, env, file) {
        runtime::use_model(
            GENERATOR_HANDLE,
            "create-unawaited-operation",
            GENERATOR_MODEL,
            Err("async-generator operation requires a directly awaited local generator"),
            file,
            expression.range,
            result,
        );
        return true;
    }
    if matches!(
        step(
            &id,
            operation,
            expression.range,
            env,
            file,
            functions,
            result
        ),
        Progress::Finished
    ) && operation == Step::Next
    {
        rejected(env, expression.range, "StopAsyncIteration");
    }
    true
}

pub(in crate::flow) fn context_enter(
    item: &FlowWithItem,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<Option<String>> {
    if !active(env) {
        return None;
    }
    let id = if let Some((function, _, _)) = call_parts(&item.context) {
        let definition = resolved_function(function, env, functions)?;
        if !definition.async_generator.as_ref()?.context {
            return None;
        }
        consume_expr(&item.context, env, file, contracts, functions, result, 0);
        env.pending_return_targets
            .take()
            .and_then(|objects| objects.first().cloned())
    } else {
        expression_generator(&item.context, env)
            .filter(|id| env.async_resources.generators[id].context)
    }?;
    match step(&id, Step::Next, item.range, env, file, functions, result) {
        Progress::Yielded(objects) => {
            if item.unsupported_target {
                boundary(
                    result,
                    file,
                    item.range,
                    "asynccontextmanager target destructuring is outside the reviewed subset",
                );
            }
            if let Some(name) = &item.target {
                bind(name, objects.clone(), env);
                for object in objects {
                    env.borrowed.insert(object);
                }
            }
            Some(Some(id))
        }
        Progress::Finished => {
            rejected(env, item.range, "RuntimeError");
            Some(None)
        }
        Progress::Interrupted => Some(None),
    }
}

pub(in crate::flow) fn context_exit(
    id: &str,
    incoming: Option<&CompletionKind>,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let operation = if matches!(
        incoming,
        Some(CompletionKind::Raise(_) | CompletionKind::Cancelled(_))
    ) {
        Step::Close
    } else {
        Step::Next
    };
    step(id, operation, range, env, file, functions, result);
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow) fn execute_for(
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
) -> Option<FlowResult> {
    if !active(env) || depth >= MAX_CONTROL_FLOW_DEPTH {
        return None;
    }
    let (iterator, target) = iterator.zip(target)?;
    let id = if let Some((function, _, _)) = call_parts(iterator) {
        let definition = resolved_function(function, env, functions)?;
        if definition.async_generator.as_ref()?.context {
            return None;
        }
        consume_expr(iterator, env, file, contracts, functions, result, 0);
        env.pending_return_targets
            .take()
            .and_then(|objects| objects.first().cloned())?
    } else {
        expression_generator(iterator, env)?
    };
    let mut flow = FlowResult::default();
    let mut normal = Vec::new();
    let started = step(&id, Step::Next, range, env, file, functions, result);
    flow.raises.extend(take_pending_raises(env));
    if matches!(started, Progress::Yielded(_)) && env.alive {
        if let Progress::Yielded(objects) = started {
            bind(target, objects, env);
        }
        let mut iteration = execute(body, env, file, contracts, functions, result, depth + 1);
        normal.extend(iteration.breaks.drain(..).map(|exit| exit.state));
        let continuing = iteration
            .continues
            .drain(..)
            .map(|exit| exit.state)
            .chain(env.alive.then(|| env.clone()))
            .collect();
        flow.append(iteration);
        if let Some(next) = join_states(continuing) {
            *env = next;
            let completed = step(&id, Step::Next, range, env, file, functions, result);
            flow.raises.extend(take_pending_raises(env));
            if matches!(completed, Progress::Finished) && env.alive {
                flow.append(execute(
                    alternate,
                    env,
                    file,
                    contracts,
                    functions,
                    result,
                    depth + 1,
                ));
                if env.alive {
                    normal.push(env.clone());
                }
            }
        }
    } else if matches!(started, Progress::Finished) && env.alive {
        flow.append(execute(
            alternate,
            env,
            file,
            contracts,
            functions,
            result,
            depth + 1,
        ));
        if env.alive {
            normal.push(env.clone());
        }
    } else if env.alive {
        normal.push(env.clone());
    }
    if let Some(joined) = join_states(normal) {
        *env = joined;
    } else {
        env.alive = false;
    }
    Some(flow)
}

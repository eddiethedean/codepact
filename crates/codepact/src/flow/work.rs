//! Observation duties for locally submitted work and independently completed handles.
use super::*;

const PENDING: u8 = 1;
const OBSERVED: u8 = 2;
const ESCAPED: u8 = 4;
const ABSENT: u8 = 8;
const WAITING: u8 = 1;
const READY: u8 = 2;
const CANCELLED: u8 = 4;
const MAX_WORK: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Task,
    AsyncFuture,
    Pool(String),
    Thread(String),
}

impl Kind {
    fn model(&self) -> (&'static str, &'static str) {
        match self {
            Self::Task => ("asyncio.Task", "TPM-ASYNCIO-001"),
            Self::AsyncFuture => ("asyncio.Future", "TPM-ASYNCIO-001"),
            Self::Pool(_) => ("multiprocessing.pool.AsyncResult", "TPM-POOL-001"),
            Self::Thread(_) => ("concurrent.futures.Future", "TPM-EXECUTOR-001"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct WorkerExit {
    pub(super) normal: bool,
    pub(super) errors: BTreeSet<Option<String>>,
}

impl WorkerExit {
    fn unknown() -> Self {
        Self {
            normal: true,
            errors: BTreeSet::from([None]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    kind: Kind,
    origin: Span,
    duty: u8,
    readiness: u8,
    dispatched: bool,
    outcome: WorkerExit,
    contract: Option<outcomes::OutcomeContract>,
    buffered: Option<BufferedValue>,
    delivered_buffered: bool,
    stored_outcomes: outcomes::Carrier,
    unclear_outcomes: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferedValue {
    object: String,
    cleanup_origin: Option<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Handle(String),
    RunningLoop,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct WorkState {
    entries: BTreeMap<String, Entry>,
    names: BTreeMap<String, Value>,
    pending: Option<Value>,
}

impl WorkState {
    pub(super) fn join(&mut self, other: &Self) {
        self.names
            .retain(|name, value| other.names.get(name) == Some(value));
        if self.pending != other.pending {
            self.pending = None;
        }
        for (id, entry) in &mut self.entries {
            if let Some(other) = other.entries.get(id) {
                entry.duty |= other.duty;
                entry.readiness |= other.readiness;
                entry.dispatched &= other.dispatched;
                entry.outcome.normal |= other.outcome.normal;
                entry
                    .outcome
                    .errors
                    .extend(other.outcome.errors.iter().cloned());
                if entry.buffered != other.buffered {
                    entry.buffered = None;
                }
                entry.delivered_buffered &= other.delivered_buffered;
                entry.unclear_outcomes |=
                    other.unclear_outcomes || entry.stored_outcomes != other.stored_outcomes;
                entry.stored_outcomes.merge(&other.stored_outcomes);
            } else {
                entry.duty |= ABSENT;
            }
        }
        for (id, entry) in &other.entries {
            self.entries.entry(id.clone()).or_insert_with(|| {
                let mut entry = entry.clone();
                entry.duty |= ABSENT;
                entry
            });
        }
    }

    pub(super) fn rebind(&mut self, name: &str) {
        self.names.remove(name);
    }

    pub(super) fn assign(&mut self, name: &str, source: Option<&str>) {
        let value = self
            .pending
            .take()
            .or_else(|| source.and_then(|source| self.names.get(source).cloned()));
        self.rebind(name);
        if let Some(value) = value {
            self.names.insert(name.into(), value);
        }
    }

    pub(super) fn has_name(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub(super) fn has_names(&self) -> bool {
        !self.names.is_empty()
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn snapshot(&mut self, from: &str, temporary: &str) -> bool {
        if let Some(value) = self.names.get(from).cloned() {
            self.names.insert(temporary.into(), value);
            true
        } else {
            false
        }
    }

    pub(super) fn snapshot_pending(&mut self, temporary: &str) -> bool {
        if let Some(value) = self.pending.take() {
            self.names.insert(temporary.into(), value);
            true
        } else {
            false
        }
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending = None;
    }

    fn handle(&self, name: &str) -> Option<(&str, &Entry)> {
        let Value::Handle(id) = self.names.get(name)? else {
            return None;
        };
        Some((id, self.entries.get(id)?))
    }

    pub(super) fn future_id(&self, name: &str) -> Option<&str> {
        self.handle(name)
            .filter(|(_, entry)| entry.kind == Kind::AsyncFuture)
            .map(|(id, _)| id)
    }

    fn escape(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.get_mut(id) else {
            return false;
        };
        if entry.duty & PENDING == 0 {
            return false;
        }
        entry.duty = (entry.duty & !PENDING) | ESCAPED;
        true
    }

    pub(super) fn is_ready(&self, id: &str) -> bool {
        self.entries
            .get(id)
            .is_some_and(|entry| entry.readiness == READY)
    }
}

fn selected(env: &State) -> bool {
    env.ownership.is_some()
        && env.models.foundations.is_some()
        && runtime::runtime_profile(env)
            .is_ok_and(|profile| profile.applicability(&env.target_python_version).is_ok())
}

pub(super) fn register(
    env: &mut State,
    id: Option<String>,
    kind: Kind,
    file: &SourceFile,
    range: SourceRange,
    outcome: WorkerExit,
    result: &mut Inspection,
) {
    if !selected(env) {
        return;
    }
    let id = id.unwrap_or_else(|| {
        format!(
            "{}:{}:{}",
            file.path,
            range.start,
            env.allocation_context.join("/")
        )
    });
    let work = &mut env.ownership.as_mut().expect("selected work model").work;
    if work.entries.contains_key(&id) || work.entries.len() >= MAX_WORK {
        if let Some(entry) = work.entries.get_mut(&id) {
            entry.readiness |= WAITING;
            entry.duty |= PENDING | ESCAPED;
        }
        boundary(
            result,
            file,
            range,
            "work identity budget exceeded or work repeatedly submitted at one site",
        );
        return;
    }
    let readiness = if matches!(kind, Kind::Thread(_) | Kind::Pool(_)) {
        WAITING | READY
    } else {
        WAITING
    };
    work.entries.insert(
        id.clone(),
        Entry {
            kind,
            origin: span(file, range),
            duty: PENDING,
            readiness,
            dispatched: false,
            outcome,
            contract: None,
            buffered: None,
            delivered_buffered: false,
            stored_outcomes: outcomes::Carrier::default(),
            unclear_outcomes: false,
        },
    );
    work.pending = Some(Value::Handle(id));
}

pub(super) fn schedule_task(
    env: &mut State,
    id: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    register(
        env,
        Some(id.into()),
        Kind::Task,
        file,
        range,
        WorkerExit::unknown(),
        result,
    );
}

pub(super) fn retain_buffered(env: &mut State, worker: &State, id: &str) {
    let Some(object) = worker.pending_return_targets.as_ref().and_then(|targets| {
        targets.first().filter(|id| {
            targets.len() == 1
                && worker
                    .resources
                    .get(*id)
                    .is_some_and(library::buffered_value)
        })
    }) else {
        return;
    };
    let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    else {
        return;
    };
    entry.buffered = Some(BufferedValue {
        object: object.clone(),
        cleanup_origin: worker.duty_origins.get(object).cloned(),
    });
    apply_call_object_effects(env, worker, &BTreeSet::new(), object);
}

pub(super) fn deliver_buffered(env: &mut State, id: &str) {
    let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    else {
        return;
    };
    let Some(buffered) = &entry.buffered else {
        return;
    };
    env.pending_return_targets = Some(BTreeSet::from([buffered.object.clone()]));
    if !entry.delivered_buffered {
        if let Some(origin) = &buffered.cleanup_origin {
            env.duty_origins
                .insert(buffered.object.clone(), origin.clone());
        }
        entry.delivered_buffered = true;
    }
}

pub(super) fn observe(env: &mut State, id: &str) {
    if let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    {
        entry.duty = (entry.duty & !(PENDING | ESCAPED)) | OBSERVED;
        entry.readiness = READY;
    }
}

pub(super) fn may_observe(env: &mut State, id: &str) {
    if let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    {
        entry.duty |= OBSERVED;
    }
}

pub(super) fn cancel(env: &mut State, id: &str) {
    if let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    {
        entry.duty = (entry.duty & !(PENDING | ESCAPED)) | OBSERVED;
        entry.readiness = CANCELLED;
    }
}

pub(super) fn ready(env: &mut State, id: &str) {
    if let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(id))
    {
        entry.readiness = (entry.readiness & CANCELLED) | READY;
    }
}

pub(super) fn owner_finished(env: &mut State, owner: &Kind) {
    if let Some(ownership) = &mut env.ownership {
        for entry in ownership
            .work
            .entries
            .values_mut()
            .filter(|entry| &entry.kind == owner)
        {
            entry.readiness &= !WAITING;
            if entry.readiness == 0 {
                entry.readiness = READY;
            }
        }
    }
}

pub(super) fn owner_may_cancel_waiting(env: &mut State, owner: &Kind) {
    if let Some(ownership) = &mut env.ownership {
        for entry in ownership
            .work
            .entries
            .values_mut()
            .filter(|entry| &entry.kind == owner)
        {
            if entry.readiness & WAITING != 0 && !entry.dispatched {
                entry.readiness |= CANCELLED;
                entry.duty |= OBSERVED;
            }
        }
    }
}

pub(super) fn escape(
    env: &mut State,
    names: &BTreeSet<String>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(ownership) = &mut env.ownership else {
        return;
    };
    let work = &mut ownership.work;
    let ids = names
        .iter()
        .filter_map(|name| work.handle(name).map(|(id, _)| id.to_owned()))
        .collect::<BTreeSet<_>>();
    let mut escaped = false;
    let mut carried = outcomes::Values::new();
    for id in ids {
        if let Some(entry) = work.entries.get(&id) {
            carried.extend(entry.stored_outcomes.ids.iter().cloned());
        }
        escaped |= work.escape(&id);
    }
    outcomes::escape(env, &carried, file, range, result);
    if escaped {
        boundary(
            result,
            file,
            range,
            "work handle escapes before its result or exception is observed",
        );
    }
}

pub(super) fn escape_pending(
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(ownership) = &mut env.ownership else {
        return;
    };
    if let Some(Value::Handle(id)) = ownership.work.pending.take() {
        if ownership.work.escape(&id) {
            boundary(
                result,
                file,
                range,
                "work result handle escapes before observation",
            );
        }
    }
}

pub(super) fn report_scope(
    fallthrough: &State,
    outcomes: &FlowResult,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mut work: BTreeMap<&str, (&Entry, u8, Vec<Span>)> = BTreeMap::new();
    for (state, range) in outcomes
        .all_abrupt()
        .map(|exit| (&exit.state, exit.range))
        .chain(fallthrough.alive.then_some((fallthrough, range)))
    {
        if let Some(ownership) = &state.ownership {
            for (id, entry) in &ownership.work.entries {
                let (_, duties, pending_sites) = work.entry(id).or_insert((entry, 0, Vec::new()));
                *duties |= entry.duty;
                if entry.duty & PENDING != 0 {
                    let site = span(file, range);
                    if !pending_sites.contains(&site) {
                        pending_sites.push(site);
                    }
                }
            }
        }
    }
    for (_, (entry, duties, pending_sites)) in work {
        if duties & PENDING == 0 {
            continue;
        }
        let (symbol, model) = entry.kind.model();
        report(
            result,
            "TP3003",
            &format!("{symbol} result or exception remains unobserved"),
            entry.origin.clone(),
            pending_sites,
            if duties == PENDING {
                "definite"
            } else {
                "possible"
            },
            vec![model.into(), symbol.into()],
        );
    }
}

pub(super) fn report_module_scope(state: &State, result: &mut Inspection) {
    if let Some(ownership) = &state.ownership {
        for entry in ownership
            .work
            .entries
            .values()
            .filter(|entry| entry.duty & PENDING != 0)
        {
            boundary_at_span(
                result,
                entry.origin.clone(),
                "module-scope work observation may occur outside the checked local scope",
            );
        }
    }
}

pub(super) fn worker_exit(definition: &FunctionBody) -> WorkerExit {
    let Some(statement) = definition.statements.first() else {
        return WorkerExit {
            normal: true,
            errors: BTreeSet::new(),
        };
    };
    match statement {
        FlowStatement::Raise {
            exception: Some(exception),
            ..
        } => {
            let callee = call_parts(exception)
                .map(|(callee, _, _)| callee)
                .unwrap_or(exception);
            let exception = expr_name(callee)
                .filter(|name| builtin_exception_match(name, "BaseException") == Some(true))
                .filter(|name| {
                    !definition.module_bindings.functions.contains_key(*name)
                        && !definition.module_bindings.classes.contains_key(*name)
                })
                .map(str::to_owned);
            WorkerExit {
                normal: false,
                errors: BTreeSet::from([exception]),
            }
        }
        FlowStatement::Return { value, .. }
            if value.as_ref().is_none_or(|value| {
                matches!(
                    value.kind,
                    AstExprKind::Primitive
                        | AstExprKind::String(_)
                        | AstExprKind::Boolean(_)
                        | AstExprKind::NoneLiteral
                )
            }) =>
        {
            WorkerExit {
                normal: true,
                errors: BTreeSet::new(),
            }
        }
        _ => WorkerExit::unknown(),
    }
}

pub(super) fn for_worker(env: &State, worker: &AstExpr, functions: &FunctionMap) -> WorkerExit {
    expr_name(worker)
        .and_then(|name| env.function_bindings.get(name))
        .and_then(|key| functions.get(key))
        .map_or_else(WorkerExit::unknown, worker_exit)
}

pub(super) fn bind_worker_contract(env: &mut State, worker: &AstExpr, functions: &FunctionMap) {
    let contract = expr_name(worker)
        .and_then(|name| env.function_bindings.get(name))
        .and_then(|binding| functions.get(binding))
        .and_then(|definition| definition.outcome_return);
    let Some(work) = env.ownership.as_mut().map(|ownership| &mut ownership.work) else {
        return;
    };
    if let Some(Value::Handle(id)) = &work.pending {
        if let Some(entry) = work.entries.get_mut(id) {
            entry.contract = contract;
        }
    }
}

fn named_raise(env: &mut State, range: SourceRange, name: &str) {
    queue_raise(
        env,
        range,
        Some(AstExpr {
            range,
            kind: AstExprKind::Name(format!(":work-error:{name}")),
        }),
    );
}

fn asyncio_profile(env: &State) -> bool {
    selected(env)
        && env.async_context
        && runtime::runtime_profile(env).is_ok_and(|profile| {
            let ownership = env.ownership.as_ref().expect("selected asyncio model");
            profile
                .asyncio(
                    &env.target_python_version,
                    ownership.mode.assumes_default_task_factory()
                        && !ownership.incompatible_task_factory,
                )
                .is_ok()
        })
}

fn numeric_timeout(expr: &AstExpr, file: &SourceFile) -> bool {
    matches!(expr.kind, AstExprKind::Primitive)
        && file
            .source
            .get(expr.range.start..expr.range.end)
            .and_then(|text| text.parse::<f64>().ok())
            .is_some_and(f64::is_finite)
}

fn timeout(arguments: &[AstExpr], keywords: &[CallKeyword], file: &SourceFile) -> Option<bool> {
    match (arguments, keywords) {
        ([], []) => Some(false),
        ([value], []) | ([], [(Some(_), value)])
            if matches!(value.kind, AstExprKind::NoneLiteral) =>
        {
            if keywords.is_empty() || keywords[0].0.as_deref() == Some("timeout") {
                Some(false)
            } else {
                None
            }
        }
        ([value], []) if numeric_timeout(value, file) => Some(true),
        ([], [(Some(name), value)]) if name == "timeout" && numeric_timeout(value, file) => {
            Some(true)
        }
        _ => None,
    }
}

pub(super) fn await_future(
    expr: &AstExpr,
    env: &mut State,
    result: &mut Inspection,
    file: &SourceFile,
) -> bool {
    let Some((id, entry)) = expr_name(expr)
        .and_then(|name| env.ownership.as_ref()?.work.handle(name))
        .filter(|(_, entry)| entry.kind == Kind::AsyncFuture)
        .map(|(id, entry)| (id.to_owned(), entry.clone()))
    else {
        return false;
    };
    if entry.readiness & WAITING != 0 {
        boundary(
            result,
            file,
            expr.range,
            "pending local Future requires an unsupported external setter or callback",
        );
        queue_raise(env, expr.range, None);
    } else {
        deliver(env, &id, &entry, false, expr.range);
        carry_future_result(env, &entry, file, expr.range, result);
    }
    true
}

pub(super) fn combine_future(
    name: &str,
    range: SourceRange,
    mode: asyncio::Delivery,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let Some((id, entry)) = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.work.handle(name))
        .filter(|(_, entry)| entry.kind == Kind::AsyncFuture)
        .map(|(id, entry)| (id.to_owned(), entry.clone()))
    else {
        return;
    };
    if entry.readiness & WAITING != 0 {
        boundary(
            result,
            file,
            range,
            "pending local Future requires an unsupported external setter or callback",
        );
        queue_raise(env, range, None);
    } else if mode == asyncio::Delivery::Collect {
        if entry.readiness == READY {
            observe(env, &id);
            carry_future_result(env, &entry, file, range, result);
        }
    } else if mode != asyncio::Delivery::WaitOnly {
        deliver(env, &id, &entry, false, range);
        carry_future_result(env, &entry, file, range, result);
    }
}

fn carry_future_result(
    env: &mut State,
    entry: &Entry,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    if !env.alive || !entry.outcome.normal {
        return;
    }
    if entry.unclear_outcomes {
        outcomes::escape(env, &entry.stored_outcomes.ids, file, range, result);
    } else {
        env.outcomes.set_value(entry.stored_outcomes.clone());
    }
}

fn deliver(env: &mut State, id: &str, entry: &Entry, check_exception: bool, range: SourceRange) {
    if entry.readiness & CANCELLED != 0 {
        let error = if matches!(entry.kind, Kind::Thread(_)) {
            "concurrent.futures.CancelledError"
        } else {
            "CancelledError"
        };
        let mut cancelled = env.clone();
        cancelled.pending_raises.clear();
        cancel(&mut cancelled, id);
        named_raise(&mut cancelled, range, error);
        env.pending_raises
            .extend(take_pending_raises(&mut cancelled));
        if entry.readiness & (READY | WAITING) == 0 {
            env.alive = false;
            return;
        }
    }
    if !check_exception {
        observe(env, id);
        for error in &entry.outcome.errors {
            if let Some(error) = error {
                named_raise(env, range, error);
            } else {
                queue_raise(env, range, None);
            }
        }
        env.alive &= entry.outcome.normal;
    } else if !entry.outcome.errors.is_empty() {
        if entry.outcome.normal {
            if let Some(entry) = env
                .ownership
                .as_mut()
                .and_then(|ownership| ownership.work.entries.get_mut(id))
            {
                entry.duty |= OBSERVED;
            }
            ready(env, id);
        } else {
            observe(env, id);
        }
    } else {
        ready(env, id);
    }
}

#[allow(clippy::too_many_arguments)]
fn set_future(
    env: &mut State,
    id: &str,
    entry: &Entry,
    method: &str,
    argument: &AstExpr,
    file: &SourceFile,
    range: SourceRange,
) -> bool {
    let exception = if method == "set_exception" {
        let callee = call_parts(argument).map_or(argument, |(callee, _, _)| callee);
        // Call preparation can replace an already evaluated constructor with
        // a primitive. Check its source spelling and the current binding.
        let constructor = || {
            let text = file.source.get(argument.range.start..argument.range.end)?;
            let (name, _) = text.split_once('(')?;
            Some(name.trim())
        };
        let Some(name) = expr_name(callee)
            .or_else(constructor)
            .filter(|name| *name != "StopIteration" && is_unshadowed_builtin_exception(name, env))
        else {
            return false;
        };
        Some(name.to_owned())
    } else {
        None
    };
    if entry.readiness & (READY | CANCELLED) != 0 {
        named_raise(env, range, "asyncio.InvalidStateError");
        if entry.readiness & WAITING == 0 {
            env.alive = false;
            return true;
        }
    }
    let stored = if exception.is_none() {
        let stored = outcomes::argument_carrier(env, argument);
        outcomes::retain_for_work(env, &stored.ids);
        stored
    } else {
        outcomes::Carrier::default()
    };
    let entry = env
        .ownership
        .as_mut()
        .expect("local Future")
        .work
        .entries
        .get_mut(id)
        .expect("tracked Future");
    entry.outcome = WorkerExit {
        normal: exception.is_none(),
        errors: exception.into_iter().map(Some).collect(),
    };
    entry.readiness = READY;
    entry.stored_outcomes = stored;
    entry.unclear_outcomes = false;
    env.outcomes.clear_value();
    true
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if !selected(env) {
        return false;
    }
    if let Some((function, [], [])) = call_parts(expression) {
        if ownership::external_identity(function, env).as_deref()
            == Some("external:asyncio.get_running_loop")
            && asyncio_profile(env)
        {
            env.ownership
                .as_mut()
                .expect("asyncio profile")
                .work
                .pending = Some(Value::RunningLoop);
            return true;
        }
    }
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    let work = &env.ownership.as_ref().expect("selected work model").work;
    if method == "create_future"
        && arguments.is_empty()
        && keywords.is_empty()
        && work.names.get(name) == Some(&Value::RunningLoop)
        && asyncio_profile(env)
    {
        register(
            env,
            None,
            Kind::AsyncFuture,
            file,
            expression.range,
            WorkerExit::unknown(),
            result,
        );
        return true;
    }
    let Some((id, entry)) = work
        .handle(name)
        .map(|(id, entry)| (id.to_owned(), entry.clone()))
    else {
        return false;
    };
    if entry.kind == Kind::Task {
        return false;
    }
    let range = expression.range;
    let no_args = arguments.is_empty() && keywords.is_empty();
    let (symbol, model) = entry.kind.model();
    let matched = match (&entry.kind, method) {
        (Kind::Pool(_), "wait") => timeout(arguments, keywords, file)
            .map(|timed| {
                if !timed {
                    ready(env, &id);
                }
            })
            .is_some(),
        (Kind::Pool(_), "ready")
        | (Kind::Thread(_), "done" | "running")
        | (Kind::AsyncFuture, "done")
            if no_args =>
        {
            true
        }
        (Kind::Pool(_), "successful") if no_args => {
            if entry.readiness & WAITING != 0 {
                named_raise(env, range, "ValueError");
            }
            if entry.readiness & READY == 0 {
                env.alive = false;
            }
            true
        }
        (Kind::Thread(_), "cancel") if no_args => {
            if entry.readiness & WAITING != 0 && !entry.dispatched {
                let mut cancelled = env.clone();
                cancel(&mut cancelled, &id);
                *env = merge(env, &cancelled).0;
            }
            true
        }
        (Kind::AsyncFuture, "cancel") if no_args => {
            if entry.readiness == WAITING {
                cancel(env, &id);
            } else if entry.readiness & WAITING != 0 {
                let mut cancelled = env.clone();
                cancel(&mut cancelled, &id);
                *env = merge(env, &cancelled).0;
            }
            true
        }
        (Kind::AsyncFuture, "set_result" | "set_exception")
            if arguments.len() == 1 && keywords.is_empty() =>
        {
            set_future(env, &id, &entry, method, &arguments[0], file, range)
        }
        (Kind::Pool(_), "get") | (Kind::Thread(_), "result" | "exception") => {
            timeout(arguments, keywords, file)
                .map(|timed| {
                    if timed && entry.readiness & WAITING != 0 {
                        let error = if matches!(entry.kind, Kind::Pool(_)) {
                            "multiprocessing.TimeoutError"
                        } else {
                            "TimeoutError"
                        };
                        named_raise(env, range, error);
                    }
                    deliver(env, &id, &entry, method == "exception", range);
                    if method == "result" && env.alive {
                        if let Some(contract) = entry.contract {
                            outcomes::deliver_work(env, &id, contract, file, range, result);
                        }
                    }
                })
                .is_some()
        }
        (Kind::AsyncFuture, "result" | "exception") if no_args => {
            if entry.readiness & WAITING != 0 {
                named_raise(env, range, "asyncio.InvalidStateError");
                if entry.readiness & (READY | CANCELLED) == 0 {
                    env.alive = false;
                    return true;
                }
            }
            deliver(env, &id, &entry, method == "exception", range);
            if method == "result" {
                carry_future_result(env, &entry, file, range, result);
            }
            true
        }
        _ => false,
    };
    if matched {
        runtime::use_model(symbol, method, model, Ok(()), file, range, result);
    } else {
        escape(env, &BTreeSet::from([name.into()]), file, range, result);
        runtime::use_model(
            symbol,
            method,
            model,
            Err("work operation or arguments are outside the qualified local subset"),
            file,
            range,
            result,
        );
        queue_raise(env, range, None);
    }
    true
}

#[derive(Clone, Copy)]
enum Predicate {
    Ready,
    Running,
    Cancel,
}

pub(super) struct WorkTest {
    id: String,
    predicate: Predicate,
}

pub(super) fn test(
    expr: &AstExpr,
    env: &State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<WorkTest> {
    let (receiver, method, [], []) = called_method(expr)? else {
        return None;
    };
    let (id, entry) = env.ownership.as_ref()?.work.handle(expr_name(receiver)?)?;
    let predicate = match (&entry.kind, method) {
        (Kind::AsyncFuture | Kind::Task | Kind::Thread(_), "done") | (Kind::Pool(_), "ready") => {
            Predicate::Ready
        }
        (Kind::Thread(_), "running") => Predicate::Running,
        (Kind::Thread(_) | Kind::AsyncFuture, "cancel") => Predicate::Cancel,
        _ => return None,
    };
    let (symbol, model) = entry.kind.model();
    runtime::use_model(symbol, method, model, Ok(()), file, expr.range, result);
    Some(WorkTest {
        id: id.into(),
        predicate,
    })
}

pub(super) fn refine(env: &mut State, test: &WorkTest, truth: bool) {
    let Some(entry) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.work.entries.get_mut(&test.id))
    else {
        return;
    };
    match (test.predicate, truth) {
        (Predicate::Ready, true) if entry.kind == Kind::Task => {
            entry.readiness = READY;
        }
        (Predicate::Ready, truth) => {
            entry.readiness &= if truth { READY | CANCELLED } else { WAITING };
            env.alive &= entry.readiness != 0;
        }
        (Predicate::Running, true) => {
            env.alive &= entry.readiness & WAITING != 0;
            entry.readiness = WAITING;
            entry.dispatched = true;
        }
        (Predicate::Running, false) => {}
        (Predicate::Cancel, true) => {
            env.alive &= (entry.readiness & WAITING != 0 && !entry.dispatched)
                || (matches!(entry.kind, Kind::Thread(_)) && entry.readiness & CANCELLED != 0);
            cancel(env, &test.id);
        }
        (Predicate::Cancel, false) if entry.kind == Kind::AsyncFuture => {
            env.alive &= entry.readiness & (READY | CANCELLED) != 0;
        }
        (Predicate::Cancel, false) => {
            entry.readiness &= !CANCELLED;
            env.alive &= entry.readiness != 0;
            entry.dispatched = true;
        }
    }
}

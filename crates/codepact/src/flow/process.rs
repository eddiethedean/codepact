use super::*;

const PROCESS_MODEL: &str = "TPM-PROCESS-001";
const POOL_MODEL: &str = "TPM-POOL-001";
const MAX_HANDLES: usize = 32;
const NEW: u8 = 1;
const STARTED: u8 = 2;
const ENDED: u8 = 4;
const CLOSED: u8 = 8;
const DRAINING: u8 = 16;
const FORCED: u8 = 32;
const JOINED: u8 = 64;
const UNCERTAIN: u8 = 128;
const FORCED_JOINED: u8 = CLOSED;

fn pool_submission_signature() -> &'static CallSignature {
    static SIGNATURE: std::sync::OnceLock<CallSignature> = std::sync::OnceLock::new();
    SIGNATURE.get_or_init(|| CallSignature {
        positional_or_keyword: ["self", "func", "args", "kwds", "callback", "error_callback"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        optional: ["args", "kwds", "callback", "error_callback"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..CallSignature::default()
    })
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct LocalProcesses {
    contexts: BTreeMap<String, String>,
    aliases: BTreeMap<String, String>,
    handles: BTreeMap<String, LocalHandle>,
    pub(super) custom_wait: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct LocalHandle {
    kind: HandleKind,
    states: u8,
    origin: Span,
    admitted_work: bool,
    prelaunch_failure: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Process,
    Pool,
}

impl HandleKind {
    fn model(self) -> &'static str {
        match self {
            Self::Process => PROCESS_MODEL,
            Self::Pool => POOL_MODEL,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Process => "multiprocessing.Process",
            Self::Pool => "multiprocessing.Pool",
        }
    }
}

impl LocalProcesses {
    pub(super) fn join(&mut self, other: &Self) {
        self.custom_wait |= other.custom_wait;
        self.contexts
            .retain(|name, method| other.contexts.get(name) == Some(method));
        self.aliases
            .retain(|name, id| other.aliases.get(name) == Some(id));
        for (id, handle) in &mut self.handles {
            if let Some(other) = other.handles.get(id) {
                handle.states |= other.states;
                handle.admitted_work |= other.admitted_work;
                handle.prelaunch_failure &= other.prelaunch_failure;
            } else {
                handle.states |= UNCERTAIN;
            }
        }
        for (id, handle) in &other.handles {
            self.handles.entry(id.clone()).or_insert_with(|| {
                let mut handle = handle.clone();
                handle.states |= UNCERTAIN;
                handle
            });
        }
    }

    pub(super) fn rebind(&mut self, name: &str) {
        self.aliases.remove(name);
        self.contexts.remove(name);
    }

    pub(super) fn bind_alias(&mut self, name: &str, source: Option<&str>) {
        let id = source.and_then(|source| self.aliases.get(source)).cloned();
        let context = source.and_then(|source| self.contexts.get(source)).cloned();
        self.rebind(name);
        if let Some(id) = id {
            self.aliases.insert(name.to_owned(), id);
        }
        if let Some(context) = context {
            self.contexts.insert(name.to_owned(), context);
        }
    }

    pub(super) fn has_handle(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }

    pub(super) fn has_aliases(&self) -> bool {
        !self.aliases.is_empty()
    }

    pub(super) fn escape(
        &mut self,
        names: &BTreeSet<String>,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        for name in names {
            if let Some(handle) = self
                .aliases
                .get(name)
                .and_then(|id| self.handles.get_mut(id))
            {
                handle.states |= UNCERTAIN;
                boundary(
                    result,
                    file,
                    range,
                    "multiprocessing handle escapes the qualified local protocol",
                );
            }
        }
    }
}

fn context_expression<'a>(value: &'a AstExpr, env: &State) -> Option<Option<&'a str>> {
    let (function, arguments, keywords) = call_parts(value)?;
    if ownership::external_identity(function, env).as_deref()
        != Some("external:multiprocessing.get_context")
    {
        return None;
    }
    Some(match (arguments, keywords) {
        (
            [AstExpr {
                kind: AstExprKind::String(method),
                ..
            }],
            [],
        )
        | (
            [],
            [(
                Some(_),
                AstExpr {
                    kind: AstExprKind::String(method),
                    ..
                },
            )],
        ) if keywords
            .first()
            .is_none_or(|(key, _)| key.as_deref() == Some("method")) =>
        {
            Some(method)
        }
        _ => None,
    })
}

type Constructor<'a> = (
    HandleKind,
    Option<&'a str>,
    &'a [AstExpr],
    &'a [CallKeyword],
);

fn constructor<'a>(value: &'a AstExpr, env: &'a State) -> Option<Constructor<'a>> {
    let (function, arguments, keywords) = call_parts(value)?;
    let kind = match ownership::external_identity(function, env).as_deref() {
        Some("external:multiprocessing.Process") => Some(HandleKind::Process),
        Some("external:multiprocessing.Pool") => Some(HandleKind::Pool),
        _ => None,
    };
    if let Some(kind) = kind {
        return Some((kind, None, arguments, keywords));
    }
    let AstExprKind::Attribute(receiver, name) = &function.kind else {
        return None;
    };
    let kind = match name.as_str() {
        "Process" => HandleKind::Process,
        "Pool" => HandleKind::Pool,
        _ => return None,
    };
    let method = match context_expression(receiver, env) {
        Some(method) => method,
        None => expr_name(receiver)
            .and_then(|name| env.ownership.as_ref()?.processes.contexts.get(name))
            .map(String::as_str)
            .map(Some)?,
    };
    Some((kind, method, arguments, keywords))
}

fn plain_values(expression: &AstExpr) -> bool {
    match &expression.kind {
        AstExprKind::Boolean(_)
        | AstExprKind::Primitive
        | AstExprKind::String(_)
        | AstExprKind::NoneLiteral => true,
        AstExprKind::Tuple(values) => values.iter().all(plain_values),
        _ => false,
    }
}

fn prelaunch_failure(expression: &AstExpr) -> bool {
    matches!(expression.kind, AstExprKind::Tuple(ref items)
        if items.iter().any(|value| matches!(value.kind, AstExprKind::Closure(_)))
            && items.iter().all(|value| plain_values(value) || matches!(value.kind, AstExprKind::Closure(_))))
}

fn worker_is_local(expression: &AstExpr, env: &State, functions: &FunctionMap) -> bool {
    expr_name(expression)
        .filter(|name| !env.shadowed_functions.contains(*name))
        .and_then(|name| env.function_bindings.get(name))
        .and_then(|key| functions.get(key))
        .is_some_and(|worker| {
            worker.supported_binding
                && !worker.is_async
                && !worker
                    .file
                    .path
                    .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
        })
}

fn valid_start_arguments(
    kind: HandleKind,
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
    file: &SourceFile,
    functions: &FunctionMap,
) -> bool {
    match kind {
        HandleKind::Process => {
            let mut target = None;
            let mut args = None;
            if !arguments.is_empty() {
                return false;
            }
            for (name, value) in keywords {
                match name.as_deref() {
                    Some("target") if target.is_none() => target = Some(value),
                    Some("args") if args.is_none() => args = Some(value),
                    _ => return false,
                }
            }
            target.is_some_and(|target| worker_is_local(target, env, functions))
                && args.is_none_or(|args| {
                    matches!(args.kind, AstExprKind::Tuple(_))
                        && (plain_values(args) || (!env.module_scope && prelaunch_failure(args)))
                })
        }
        HandleKind::Pool => {
            let count = match (arguments, keywords) {
                ([count], []) | ([], [(Some(_), count)])
                    if keywords
                        .first()
                        .is_none_or(|(key, _)| key.as_deref() == Some("processes")) =>
                {
                    count
                }
                _ => return false,
            };
            matches!(count.kind, AstExprKind::Primitive)
                && file
                    .source
                    .get(count.range.start..count.range.end)
                    .and_then(|value| value.parse::<usize>().ok())
                    .is_some_and(|count| (1..=MAX_HANDLES).contains(&count))
        }
    }
}

fn modeled_mode(env: &State) -> bool {
    env.models.foundations.is_some()
        && env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.mode.includes_advanced_contracts())
}

pub(super) fn initialize(
    name: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if !modeled_mode(env) {
        return false;
    }
    if let Some(Some(method)) = context_expression(expression, env) {
        let method = method.to_owned();
        queue_raise(env, expression.range, None);
        env.bind_runtime_handle(name)
            .processes
            .contexts
            .insert(name.into(), method);
        return true;
    }
    let Some((kind, method, arguments, keywords)) = constructor(expression, env) else {
        return false;
    };
    let fails_before_launch = kind == HandleKind::Process
        && keywords
            .iter()
            .any(|(name, value)| name.as_deref() == Some("args") && prelaunch_failure(value));
    let applies = runtime::runtime_profile(env)
        .and_then(|profile| profile.spawn(&env.target_python_version, method.unwrap_or_default()))
        .and_then(|()| valid_start_arguments(kind, arguments, keywords, env, file, functions)
            .then_some(()).ok_or("multiprocessing constructor requires a local worker with plain values or a literal Pool size"));
    if !runtime::use_model(
        kind.symbol(),
        "construct-local-handle",
        kind.model(),
        applies,
        file,
        expression.range,
        result,
    ) {
        return false;
    }
    let processes = &env.ownership.as_ref().unwrap().processes;
    let id = format!(
        "{}:{}:{}",
        file.path,
        expression.range.start,
        env.allocation_context.join("/")
    );
    if processes.handles.len() >= MAX_HANDLES
        || (env.loop_depth > 0 && processes.handles.contains_key(&id))
    {
        boundary(
            result,
            file,
            expression.range,
            "multiprocessing handle identity or budget exhausted",
        );
        return false;
    }
    queue_raise(env, expression.range, None);
    let processes = &mut env.bind_runtime_handle(name).processes;
    processes.aliases.insert(name.into(), id.clone());
    processes.handles.insert(
        id,
        LocalHandle {
            kind,
            states: if kind == HandleKind::Process {
                NEW
            } else {
                STARTED
            },
            origin: span(file, expression.range),
            admitted_work: false,
            prelaunch_failure: fails_before_launch,
        },
    );
    true
}

pub(super) fn alias_pool(env: &mut State, name: &str, source: &str) {
    let processes = &mut env.ownership.as_mut().expect("local Pool").processes;
    let id = processes.aliases.get(source).expect("entered Pool").clone();
    env.bind_runtime_handle(name)
        .processes
        .aliases
        .insert(name.into(), id);
}

fn handle(env: &State, alias: &str) -> Option<(String, LocalHandle)> {
    let processes = &env.ownership.as_ref()?.processes;
    let id = processes.aliases.get(alias)?;
    Some((id.clone(), processes.handles.get(id)?.clone()))
}

fn pool_context(expression: &AstExpr, env: &State) -> bool {
    modeled_mode(env)
        && (constructor(expression, env).is_some_and(|(kind, ..)| kind == HandleKind::Pool)
            || expr_name(expression)
                .and_then(|name| handle(env, name))
                .is_some_and(|(_, handle)| handle.kind == HandleKind::Pool))
}

// None leaves generic context handling in charge. Some(false) means the
// recognized handle declined to enter and has already queued its failure.
pub(super) fn enter_pool(
    receiver: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<bool> {
    if !pool_context(expression, env) {
        return None;
    }
    if let Some(source) = expr_name(expression) {
        alias_pool(env, receiver, source);
        Some(live_pool(env, receiver, file, range, result))
    } else {
        initialize(receiver, expression, env, file, functions, result).then_some(true)
    }
}

fn raise_named(env: &mut State, range: SourceRange, kind: &str) {
    queue_raise(
        env,
        range,
        Some(AstExpr {
            range,
            kind: AstExprKind::Name(kind.into()),
        }),
    );
}

fn states(env: &mut State, id: &str, value: u8) {
    env.ownership
        .as_mut()
        .unwrap()
        .processes
        .handles
        .get_mut(id)
        .unwrap()
        .states = value;
}

fn qualified_child_wait(env: &State) -> bool {
    runtime::runtime_profile(env).is_ok_and(|profile| profile.assume_standard_process_wait)
        && !env.ownership.as_ref().unwrap().processes.custom_wait
}

fn permit(
    env: &mut State,
    id: &str,
    handle: &LocalHandle,
    (operation, allowed, exception): (&str, u8, &str),
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let invalid = handle.states & !(allowed | UNCERTAIN);
    let valid = handle.states & allowed;
    if handle.states & UNCERTAIN != 0 {
        boundary(
            result,
            file,
            range,
            "multiprocessing handle state is unresolved on a feasible path",
        );
    }
    if invalid != 0 {
        let definite = valid == 0 && handle.states & UNCERTAIN == 0;
        report(
            result,
            "TP2102",
            &format!(
                "{}.{operation} requires a compatible local handle state",
                handle.kind.symbol()
            ),
            span(file, range),
            vec![handle.origin.clone()],
            if definite { "definite" } else { "possible" },
            vec![format!(
                "{}@{}",
                handle.kind.model(),
                crate::foundation::CATALOG_REVISION
            )],
        );
        states(env, id, invalid | (handle.states & UNCERTAIN));
        raise_named(env, range, exception);
        states(env, id, valid | (handle.states & UNCERTAIN));
    }
    if valid == 0 && handle.states & UNCERTAIN == 0 {
        env.alive = false;
        return false;
    }
    true
}

enum ProcessOperation {
    Start,
    Join(JoinTimeout),
    Close,
    IsAlive,
}

enum JoinTimeout {
    Untimed,
    Timed,
    Unresolved,
}

enum PoolOperation<'a> {
    Submit(BTreeMap<String, &'a AstExpr>),
    Close,
    Terminate,
    Join,
}

enum Operation<'a> {
    Process(ProcessOperation),
    Pool(PoolOperation<'a>),
}

enum RejectedOperation {
    InvalidBinding { effect: &'static str },
    UnknownJoinBinding,
    UnknownSubmissionBinding,
    Unsupported,
}

impl Operation<'_> {
    fn compatible_states(&self) -> (u8, &'static str) {
        match self {
            Self::Process(ProcessOperation::Start) => (NEW, "AssertionError"),
            Self::Process(ProcessOperation::Join(_)) => (STARTED | ENDED, "AssertionError"),
            Self::Process(ProcessOperation::Close) => {
                (NEW | STARTED | ENDED | CLOSED, "ValueError")
            }
            Self::Process(ProcessOperation::IsAlive) => (NEW | STARTED | ENDED, "ValueError"),
            Self::Pool(PoolOperation::Close | PoolOperation::Terminate) => (
                STARTED | DRAINING | FORCED | JOINED | FORCED_JOINED,
                "ValueError",
            ),
            Self::Pool(PoolOperation::Join) => {
                (DRAINING | FORCED | JOINED | FORCED_JOINED, "ValueError")
            }
            Self::Pool(PoolOperation::Submit(_)) => (STARTED, "ValueError"),
        }
    }
}

fn prepare_operation<'a>(
    kind: HandleKind,
    method: &str,
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
) -> Result<Operation<'a>, RejectedOperation> {
    let no_args = arguments.is_empty() && keywords.is_empty();
    let operation = match (kind, method) {
        (HandleKind::Process, "start") if no_args => Operation::Process(ProcessOperation::Start),
        (HandleKind::Process, "close") if no_args => Operation::Process(ProcessOperation::Close),
        (HandleKind::Process, "is_alive") if no_args => {
            Operation::Process(ProcessOperation::IsAlive)
        }
        (HandleKind::Process, "join") => {
            // Join accepts computed timeout values at binding time. They can
            // still leave the wait effect unknown.
            let unpacked = arguments
                .iter()
                .any(|arg| matches!(arg.kind, AstExprKind::Starred(_)))
                || keywords.iter().any(|(name, _)| name.is_none());
            if unpacked {
                return Err(RejectedOperation::UnknownJoinBinding);
            }
            if arguments.len() + keywords.len() > 1
                || keywords
                    .iter()
                    .any(|(name, _)| name.as_deref() != Some("timeout"))
            {
                return Err(RejectedOperation::InvalidBinding {
                    effect: "join-binding-rejected",
                });
            }
            let value = arguments
                .first()
                .or_else(|| keywords.first().map(|(_, value)| value));
            let timeout = match value.map(|value| &value.kind) {
                None | Some(AstExprKind::NoneLiteral) => JoinTimeout::Untimed,
                Some(AstExprKind::Primitive) => JoinTimeout::Timed,
                _ => JoinTimeout::Unresolved,
            };
            Operation::Process(ProcessOperation::Join(timeout))
        }
        (HandleKind::Pool, "apply_async") => {
            let submission = bind_arguments(pool_submission_signature(), arguments, keywords, true)
                .map_err(|failure| match failure {
                    BindingFailure::Invalid => RejectedOperation::InvalidBinding {
                        effect: "submission-rejected",
                    },
                    BindingFailure::Unknown => RejectedOperation::UnknownSubmissionBinding,
                })?;
            Operation::Pool(PoolOperation::Submit(submission))
        }
        (HandleKind::Pool, "close") if no_args => Operation::Pool(PoolOperation::Close),
        (HandleKind::Pool, "terminate") if no_args => Operation::Pool(PoolOperation::Terminate),
        (HandleKind::Pool, "join") if no_args => Operation::Pool(PoolOperation::Join),
        _ => return Err(RejectedOperation::Unsupported),
    };
    Ok(operation)
}

struct OperationEffect {
    name: &'static str,
    applicability: Result<(), &'static str>,
}

impl OperationEffect {
    fn applied(name: &'static str) -> Self {
        Self {
            name,
            applicability: Ok(()),
        }
    }

    fn declined(name: &'static str, reason: &'static str) -> Self {
        Self {
            name,
            applicability: Err(reason),
        }
    }
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> calls::ModelCall {
    if !modeled_mode(env) {
        return calls::ModelCall::Unrecognized;
    }
    if let Some((function, _, _)) = call_parts(expression) {
        if ownership::external_identity(function, env).as_deref() == Some("external:signal.signal")
        {
            env.ownership.as_mut().unwrap().processes.custom_wait = true;
            return calls::ModelCall::Unrecognized;
        }
    }
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return calls::ModelCall::Unrecognized;
    };
    let Some((id, handle)) = expr_name(receiver).and_then(|name| handle(env, name)) else {
        return calls::ModelCall::Unrecognized;
    };
    let call = HandleCall {
        id: &id,
        handle: &handle,
        file,
        range: expression.range,
    };
    let operation = match prepare_operation(handle.kind, method, arguments, keywords) {
        Ok(operation) => operation,
        Err(rejected) => return call.apply_rejection(rejected, method, env, result),
    };
    let (allowed, exception) = operation.compatible_states();
    if !permit(
        env,
        call.id,
        call.handle,
        (method, allowed, exception),
        call.file,
        call.range,
        result,
    ) {
        return calls::ModelCall::Applied;
    }
    let effect = match operation {
        Operation::Process(operation) => call.apply_process(operation, env, result),
        Operation::Pool(operation) => call.apply_pool(operation, env, functions, result),
    };
    call.record_effect(effect.name, effect.applicability, result)
}

struct HandleCall<'a> {
    id: &'a str,
    handle: &'a LocalHandle,
    file: &'a SourceFile,
    range: SourceRange,
}

impl HandleCall<'_> {
    fn state(&self, env: &State) -> u8 {
        env.ownership.as_ref().unwrap().processes.handles[self.id].states
    }

    fn record_effect(
        &self,
        effect: &str,
        applicability: Result<(), &str>,
        result: &mut Inspection,
    ) -> calls::ModelCall {
        if runtime::use_model(
            self.handle.kind.symbol(),
            effect,
            self.handle.kind.model(),
            applicability,
            self.file,
            self.range,
            result,
        ) {
            calls::ModelCall::Applied
        } else {
            calls::ModelCall::Declined
        }
    }

    fn apply_rejection(
        &self,
        rejected: RejectedOperation,
        method: &str,
        env: &mut State,
        result: &mut Inspection,
    ) -> calls::ModelCall {
        let reason = match rejected {
            RejectedOperation::InvalidBinding { effect } => {
                raise_named(env, self.range, "TypeError");
                env.alive = false;
                return self.record_effect(effect, Ok(()), result);
            }
            RejectedOperation::UnknownJoinBinding => {
                "Process.join does not resolve unpacked arguments"
            }
            RejectedOperation::UnknownSubmissionBinding => {
                queue_raise(env, self.range, None);
                return self.record_effect(
                    method,
                    Err("Pool.apply_async argument binding is unresolved"),
                    result,
                );
            }
            RejectedOperation::Unsupported => {
                "operation or arguments are outside the local multiprocessing model"
            }
        };
        let effect = self.record_effect(method, Err(reason), result);
        if matches!(rejected, RejectedOperation::Unsupported) {
            states(env, self.id, self.handle.states | UNCERTAIN);
        }
        queue_raise(env, self.range, None);
        effect
    }

    fn apply_process(
        &self,
        operation: ProcessOperation,
        env: &mut State,
        result: &mut Inspection,
    ) -> OperationEffect {
        let Self {
            id,
            handle,
            file,
            range,
        } = *self;
        let handle_state = self.state(env);
        match operation {
            ProcessOperation::Start if handle.prelaunch_failure => {
                raise_named(env, range, "AttributeError");
                env.alive = false;
                OperationEffect::applied("serialization-rejected-before-launch")
            }
            ProcessOperation::Start => {
                states(env, id, handle_state | UNCERTAIN);
                queue_raise(env, range, None);
                states(env, id, STARTED | (handle_state & UNCERTAIN));
                OperationEffect::applied("start")
            }
            ProcessOperation::Join(timeout) => {
                queue_raise(env, range, None);
                match timeout {
                    JoinTimeout::Untimed if qualified_child_wait(env) => {
                        states(env, id, ENDED | (handle_state & UNCERTAIN));
                        OperationEffect::applied("join")
                    }
                    JoinTimeout::Untimed => OperationEffect::declined(
                        "join",
                        "untimed Process.join requires qualified child reaping and signal handling",
                    ),
                    JoinTimeout::Timed => OperationEffect::applied("join"),
                    JoinTimeout::Unresolved => OperationEffect::declined(
                        "join",
                        "Process.join timeout is outside the qualified literal arguments",
                    ),
                }
            }
            ProcessOperation::Close => {
                if handle_state & STARTED != 0 {
                    report(
                        result,
                        "TP2102",
                        "Process.close may run while the child is still alive",
                        span(file, range),
                        vec![handle.origin.clone()],
                        "possible",
                        vec![format!(
                            "{PROCESS_MODEL}@{}",
                            crate::foundation::CATALOG_REVISION
                        )],
                    );
                    states(env, id, STARTED | (handle_state & UNCERTAIN));
                    raise_named(env, range, "ValueError");
                }
                states(env, id, CLOSED | (handle_state & UNCERTAIN));
                OperationEffect::applied("close")
            }
            ProcessOperation::IsAlive => OperationEffect::applied("is_alive"),
        }
    }

    fn apply_pool(
        &self,
        operation: PoolOperation<'_>,
        env: &mut State,
        functions: &FunctionMap,
        result: &mut Inspection,
    ) -> OperationEffect {
        let Self {
            id,
            handle,
            file,
            range,
        } = *self;
        let handle_state = self.state(env);
        match operation {
            PoolOperation::Submit(submission) => {
                let args = submission.get("args").copied();
                let supported = submission
                    .keys()
                    .all(|name| matches!(name.as_str(), "func" | "args"))
                    && worker_is_local(submission["func"], env, functions)
                    && args.is_none_or(|args| {
                        matches!(args.kind, AstExprKind::Tuple(_))
                            && (plain_values(args) || prelaunch_failure(args))
                    });
                queue_raise(env, range, None);
                if supported {
                    env.ownership
                        .as_mut()
                        .unwrap()
                        .processes
                        .handles
                        .get_mut(id)
                        .unwrap()
                        .admitted_work = true;
                    let effect = if args.is_some_and(prelaunch_failure) {
                        "submission-admitted-with-serialization-failure"
                    } else {
                        "submission-admitted-result-unobserved"
                    };
                    OperationEffect::applied(effect)
                } else {
                    OperationEffect::declined(
                    "apply_async",
                    "Pool.apply_async requires a resolved local worker, plain arguments and no callbacks",
                )
                }
            }
            PoolOperation::Close => {
                queue_raise(env, range, None);
                states(
                    env,
                    id,
                    (handle_state & !STARTED)
                        | if handle_state & STARTED != 0 {
                            DRAINING
                        } else {
                            0
                        },
                );
                OperationEffect::applied("close")
            }
            PoolOperation::Terminate => {
                states(env, id, FORCED | UNCERTAIN);
                queue_raise(env, range, None);
                states(env, id, FORCED | (handle_state & UNCERTAIN));
                OperationEffect::applied("terminate")
            }
            PoolOperation::Join => {
                states(env, id, handle_state | UNCERTAIN);
                queue_raise(env, range, None);
                if qualified_child_wait(env) {
                    let graceful = handle_state & (DRAINING | JOINED) != 0;
                    let forced = handle_state & (FORCED | FORCED_JOINED) != 0;
                    let effect = match (graceful, forced) {
                        (true, false) => "orderly-shutdown-joined",
                        (false, true) => "forced-shutdown-joined",
                        _ => "shutdown-joined-with-mixed-provenance",
                    };
                    if graceful && !forced && handle.admitted_work {
                        runtime::use_model(
                            handle.kind.symbol(),
                            "admitted-work-drained-results-unobserved",
                            POOL_MODEL,
                            Ok(()),
                            file,
                            range,
                            result,
                        );
                    }
                    let joined = (if graceful { JOINED } else { 0 })
                        | (if forced { FORCED_JOINED } else { 0 });
                    states(env, id, joined | (handle_state & UNCERTAIN));
                    OperationEffect::applied(effect)
                } else {
                    OperationEffect::declined(
                        "join",
                        "Pool.join requires qualified child reaping and signal handling",
                    )
                }
            }
        }
    }
}

pub(super) fn finish_pool(
    receiver: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some((id, handle)) = handle(env, receiver) else {
        boundary(
            result,
            file,
            range,
            "Pool context exit has no resolved handle",
        );
        return;
    };
    states(env, &id, FORCED | UNCERTAIN);
    queue_raise(env, range, None);
    let applies = if qualified_child_wait(env) {
        states(env, &id, FORCED_JOINED | (handle.states & UNCERTAIN));
        Ok(())
    } else {
        Err("Pool context exit requires qualified child reaping and signal handling to establish a join")
    };
    runtime::use_model(
        handle.kind.symbol(),
        "forced-context-exit",
        POOL_MODEL,
        applies,
        file,
        range,
        result,
    );
}

fn live_pool(
    env: &mut State,
    name: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let Some((id, handle)) = handle(env, name) else {
        return false;
    };
    permit(
        env,
        &id,
        &handle,
        ("__enter__", STARTED, "ValueError"),
        file,
        range,
        result,
    )
}

pub(super) fn returns_status(expression: &AstExpr, env: &State) -> bool {
    called_method(expression).is_some_and(|(receiver, _, _, _)| {
        expr_name(receiver).is_some_and(|name| handle(env, name).is_some())
    })
}

pub(super) struct TerminationTest {
    id: String,
    ended_when_true: bool,
    excludes_unstarted: bool,
}

pub(super) fn termination_test(
    test: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<TerminationTest> {
    use ruff_python_ast::{CmpOp, Expr, UnaryOp};
    let processes = &env.ownership.as_ref()?.processes;
    if processes.aliases.is_empty() {
        return None;
    }
    fn member(expr: &Expr) -> Option<(&str, &str, bool)> {
        let (attribute, called) = match expr {
            Expr::Attribute(attribute) => (attribute, false),
            Expr::Call(call) if call.arguments.is_empty() => {
                let Expr::Attribute(attribute) = call.func.as_ref() else {
                    return None;
                };
                (attribute, true)
            }
            _ => return None,
        };
        let Expr::Name(name) = attribute.value.as_ref() else {
            return None;
        };
        Some((name.id.as_str(), attribute.attr.as_str(), called))
    }
    let source = file.source.get(test.range.start..test.range.end)?;
    if !source.contains("is_alive") && !source.contains("exitcode") {
        return None;
    }
    let parsed = ruff_python_parser::parse_expression(source)
        .ok()?
        .into_syntax();
    let (name, ended_when_true, excludes_unstarted) = match parsed.body.as_ref() {
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => {
            let (name, "is_alive", true) = member(&unary.operand)? else {
                return None;
            };
            (name, true, false)
        }
        Expr::Compare(compare) if compare.ops.len() == 1 && compare.comparators.len() == 1 => {
            let op = compare.ops[0];
            if !matches!(op, CmpOp::Is | CmpOp::IsNot) {
                return None;
            }
            match (member(&compare.left)?, &compare.comparators[0]) {
                ((name, "is_alive", true), Expr::BooleanLiteral(value)) => {
                    (name, (op == CmpOp::Is) != value.value, false)
                }
                ((name, "exitcode", false), Expr::NoneLiteral(_)) => {
                    (name, op == CmpOp::IsNot, true)
                }
                _ => return None,
            }
        }
        expr => {
            let (name, "is_alive", true) = member(expr)? else {
                return None;
            };
            (name, false, false)
        }
    };
    let (id, handle) = handle(env, name)?;
    if handle.kind != HandleKind::Process {
        return None;
    }
    permit(
        env,
        &id,
        &handle,
        ("status", NEW | STARTED | ENDED, "ValueError"),
        file,
        test.range,
        result,
    );
    Some(TerminationTest {
        id,
        ended_when_true,
        excludes_unstarted,
    })
}

pub(super) fn refine_test(env: &mut State, test: &TerminationTest, truth: bool) {
    let Some(handle) = env
        .ownership
        .as_mut()
        .and_then(|ownership| ownership.processes.handles.get_mut(&test.id))
    else {
        return;
    };
    let previous = handle.states;
    let unknown = previous & UNCERTAIN;
    let updated = if truth == test.ended_when_true {
        let ended = if previous & (STARTED | ENDED) != 0 {
            ENDED
        } else {
            0
        };
        ended
            | unknown
            | if test.excludes_unstarted {
                0
            } else {
                previous & NEW
            }
    } else {
        (previous & STARTED)
            | unknown
            | if test.excludes_unstarted {
                previous & NEW
            } else {
                0
            }
    };
    env.alive &= updated != 0;
    handle.states = updated;
}

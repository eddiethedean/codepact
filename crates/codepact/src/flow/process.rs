use super::*;

mod protocol;
pub(super) use protocol::{
    compound_store, enter_guard, initialize_pipe, leave_guard, protocol_test, refine_protocol_test,
    report_protocol_scope,
};

const PROCESS_MODEL: &str = "TPM-PROCESS-001";
const POOL_MODEL: &str = "TPM-POOL-001";
const SPAWN_MODEL: &str = "TPM-SPAWN-001";
const MAX_HANDLES: usize = 32;
const MAX_CHILDREN: usize = 3;
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
    protocol: protocol::LocalProtocol,
    entry_path: EntryPath,
    workers: BTreeMap<String, PendingWorker>,
}

#[derive(Clone, PartialEq, Eq)]
struct LocalHandle {
    kind: HandleKind,
    states: u8,
    origin: Span,
    admitted_work: bool,
    prelaunch_failure: bool,
    worker: Option<WorkerCandidate>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum EntryPath {
    #[default]
    Unknown,
    ImportTime,
    Protected,
}

#[derive(Clone, PartialEq, Eq)]
enum WorkerCandidate {
    Ready {
        target: String,
        arguments: BTreeMap<String, protocol::SpawnArgument>,
    },
    Invalid,
    ContextMismatch,
    Unknown,
}

#[derive(Clone, PartialEq, Eq)]
struct PendingWorker {
    target: String,
    bindings: protocol::WorkerBindings,
    evaluated: Option<protocol::WorkerEffects>,
    projected: BTreeSet<String>,
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
        self.entry_path = if self.entry_path == other.entry_path {
            self.entry_path
        } else {
            EntryPath::Unknown
        };
        self.protocol.join(&other.protocol);
        self.workers
            .retain(|id, worker| other.workers.get(id) == Some(worker));
        self.contexts
            .retain(|name, method| other.contexts.get(name) == Some(method));
        self.aliases
            .retain(|name, id| other.aliases.get(name) == Some(id));
        for (id, handle) in &mut self.handles {
            if let Some(other) = other.handles.get(id) {
                handle.states |= other.states;
                handle.admitted_work |= other.admitted_work;
                handle.prelaunch_failure &= other.prelaunch_failure;
                if handle.worker != other.worker {
                    handle.worker = Some(WorkerCandidate::Unknown);
                }
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
        self.protocol.rebind(name);
    }

    pub(super) fn inherit_entry(&mut self, parent: &Self, module_scope: bool) {
        self.entry_path = if module_scope && parent.entry_path == EntryPath::Unknown {
            EntryPath::ImportTime
        } else {
            parent.entry_path
        };
    }

    pub(super) fn bind_alias(&mut self, name: &str, source: Option<&str>) {
        let id = source.and_then(|source| self.aliases.get(source)).cloned();
        let context = source.and_then(|source| self.contexts.get(source)).cloned();
        self.aliases.remove(name);
        self.contexts.remove(name);
        self.protocol.bind_alias(name, source);
        if let Some(id) = id {
            self.aliases.insert(name.to_owned(), id);
        }
        if let Some(context) = context {
            self.contexts.insert(name.to_owned(), context);
        }
    }

    pub(super) fn has_handle(&self, name: &str) -> bool {
        self.aliases.contains_key(name) || self.protocol.has_handle(name)
    }

    pub(super) fn has_aliases(&self) -> bool {
        !self.aliases.is_empty() || self.protocol.has_aliases()
    }

    pub(super) fn escape(
        &mut self,
        names: &BTreeSet<String>,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        self.protocol.escape(names, file, range, result);
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

fn launch_matches(env: &State, file: &SourceFile) -> bool {
    runtime::runtime_profile(env)
        .ok()
        .and_then(|profile| profile.spawn_main_script.as_deref())
        .is_some_and(|path| std::path::Path::new(path) == std::path::Path::new(&file.path))
}

pub(super) fn is_main_guard(test: &AstExpr, env: &State, file: &SourceFile) -> bool {
    use ruff_python_ast::{CmpOp, Expr};
    if !env.module_scope
        || !launch_matches(env, file)
        || env.definitely_bound_names.contains("__name__")
    {
        return false;
    }
    let Some(source) = file.source.get(test.range.start..test.range.end) else {
        return false;
    };
    if !source.contains("__main__") {
        return false;
    }
    let Ok(parsed) = ruff_python_parser::parse_expression(source) else {
        return false;
    };
    let Expr::Compare(compare) = parsed.into_syntax().body.as_ref().clone() else {
        return false;
    };
    type MainOperands<'a> = (&'a Expr, &'a Expr);
    let names_main = |(left, right): MainOperands<'_>| {
        matches!(left, Expr::Name(name) if name.id.as_str() == "__name__")
            && matches!(right, Expr::StringLiteral(value) if value.value.to_str() == "__main__")
    };
    compare.ops.as_ref() == [CmpOp::Eq]
        && compare.comparators.len() == 1
        && (names_main((&compare.left, &compare.comparators[0]))
            || names_main((&compare.comparators[0], &compare.left)))
}

pub(super) fn refine_main_guard(env: &mut State, is_guard: bool) {
    if is_guard {
        if let Some(ownership) = env.ownership.as_mut() {
            ownership.processes.entry_path = EntryPath::Protected;
        }
    }
}

fn spawn_keywords(value: &AstExpr, file: &SourceFile) -> Option<Vec<CallKeyword>> {
    let source = file.source.get(value.range.start..value.range.end)?;
    let parsed = ruff_python_parser::parse_expression(source)
        .ok()?
        .into_syntax();
    if !matches!(parsed.body.as_ref(), ruff_python_ast::Expr::Dict(_)) {
        return None;
    }
    let AstExprKind::Collection(values) = &value.kind else {
        return None;
    };
    if values.len() % 2 != 0 {
        return None;
    }
    values
        .chunks_exact(2)
        .map(|pair| {
            let AstExprKind::String(name) = &pair[0].kind else {
                return None;
            };
            Some((Some(name.clone()), pair[1].clone()))
        })
        .collect()
}

fn pure_constructor_arguments(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
) -> bool {
    fn supported(value: &AstExpr, env: &State) -> bool {
        match &value.kind {
            AstExprKind::Name(_)
            | AstExprKind::Boolean(_)
            | AstExprKind::String(_)
            | AstExprKind::Primitive
            | AstExprKind::NoneLiteral
            | AstExprKind::Closure(_) => true,
            AstExprKind::Tuple(items) | AstExprKind::Collection(items) => {
                items.iter().all(|item| supported(item, env))
            }
            AstExprKind::Attribute(receiver, _) => expr_name(receiver)
                .and_then(|name| env.function_bindings.get(name))
                .is_some_and(|binding| binding.starts_with("module:")),
            _ => false,
        }
    }
    arguments.is_empty()
        && keywords
            .iter()
            .all(|(name, value)| name.is_some() && supported(value, env))
}

fn spawn_candidate(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &State,
    file: &SourceFile,
    functions: &FunctionMap,
) -> WorkerCandidate {
    if !arguments.is_empty() {
        return WorkerCandidate::Unknown;
    }
    let mut positions = BTreeMap::new();
    for (key, value) in keywords {
        let Some(key @ ("target" | "args" | "kwargs")) = key.as_deref() else {
            return WorkerCandidate::Unknown;
        };
        if positions.insert(key, value).is_some() {
            return WorkerCandidate::Unknown;
        }
    }
    let Some(target) = positions.get("target") else {
        return WorkerCandidate::Unknown;
    };
    let target_key = match &target.kind {
        AstExprKind::Closure(_) => return WorkerCandidate::Invalid,
        AstExprKind::Name(name) => {
            if env
                .ownership
                .as_ref()
                .is_some_and(|ownership| ownership.has_closure(name))
            {
                return WorkerCandidate::Invalid;
            }
            if env.shadowed_functions.contains(name) {
                return WorkerCandidate::Unknown;
            }
            env.function_bindings.get(name).cloned()
        }
        AstExprKind::Attribute(receiver, member) => expr_name(receiver)
            .and_then(|name| env.function_bindings.get(name))
            .and_then(|binding| binding.strip_prefix("module:"))
            .map(|module| format!("{module}::{member}")),
        _ => None,
    };
    let Some(key) = target_key else {
        return WorkerCandidate::Unknown;
    };
    let Some(worker) = functions.get(&key).filter(|worker| {
        worker.supported_binding
            && !worker.is_async
            && !worker
                .file
                .path
                .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
    }) else {
        return WorkerCandidate::Unknown;
    };
    let args = match positions.get("args").map(|expr| &expr.kind) {
        None => &[][..],
        Some(AstExprKind::Tuple(args)) => args,
        _ => return WorkerCandidate::Unknown,
    };
    if positions
        .get("args")
        .is_some_and(|expr| prelaunch_failure(expr))
    {
        return WorkerCandidate::Invalid;
    }
    let kwargs = match positions.get("kwargs") {
        None => Vec::new(),
        Some(value) => match spawn_keywords(value, file) {
            Some(keywords) => keywords,
            None => return WorkerCandidate::Unknown,
        },
    };
    let Ok(bound) = bind_arguments(&worker.signature, args, &kwargs, false) else {
        return WorkerCandidate::Unknown;
    };
    let arguments = bound
        .into_iter()
        .map(|(param, expression)| {
            protocol::spawn_argument(expression, env, file).map(|argument| (param, argument))
        })
        .collect::<Option<BTreeMap<_, _>>>();
    match arguments {
        Some(arguments)
            if arguments
                .values()
                .any(|arg| matches!(arg, protocol::SpawnArgument::ForkGuard)) =>
        {
            WorkerCandidate::ContextMismatch
        }
        Some(arguments) => WorkerCandidate::Ready {
            target: key,
            arguments,
        },
        None => WorkerCandidate::Unknown,
    }
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
    if protocol::initialize(name, expression, env, file, functions, result) {
        return true;
    }
    let Some((kind, method, arguments, keywords)) = constructor(expression, env) else {
        return false;
    };
    let fails_before_launch = kind == HandleKind::Process
        && keywords
            .iter()
            .any(|(name, value)| name.as_deref() == Some("args") && prelaunch_failure(value));
    let legacy_shape = valid_start_arguments(kind, arguments, keywords, env, file, functions);
    let phase10_requested = kind == HandleKind::Process
        && (!legacy_shape
            || runtime::runtime_profile(env)
                .ok()
                .and_then(|profile| profile.spawn_main_script.as_ref())
                .is_some());
    let worker = (phase10_requested && pure_constructor_arguments(arguments, keywords, env))
        .then(|| spawn_candidate(arguments, keywords, env, file, functions));
    let applies = runtime::runtime_profile(env)
        .and_then(|profile| profile.spawn(&env.target_python_version, method.unwrap_or_default()))
        .and_then(|()| (legacy_shape || worker.is_some())
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
            worker,
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
    let primitive = protocol::call(expression, env, file, functions, result);
    if !matches!(primitive, calls::ModelCall::Unrecognized) {
        return primitive;
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
        Operation::Process(operation) => call.apply_process(operation, env, functions, result),
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
            ProcessOperation::Start if handle.prelaunch_failure => {
                raise_named(env, range, "AttributeError");
                env.alive = false;
                OperationEffect::applied("serialization-rejected-before-launch")
            }
            ProcessOperation::Start => {
                if matches!(handle.worker, Some(WorkerCandidate::Invalid)) {
                    report_spawn_issue(
                        file,
                        range,
                        &handle.origin,
                        "spawn cannot serialize this local or lambda target or argument",
                        result,
                    );
                    record_spawn(
                        file,
                        range,
                        "serialization-rejected-before-launch",
                        Ok(()),
                        result,
                    );
                    raise_named(env, range, "AttributeError");
                    env.alive = false;
                    return OperationEffect::applied("serialization-rejected-before-launch");
                }
                if matches!(handle.worker, Some(WorkerCandidate::ContextMismatch)) {
                    report_spawn_issue(
                        file,
                        range,
                        &handle.origin,
                        "spawn cannot serialize a guard created in a fork context",
                        result,
                    );
                    record_spawn(
                        file,
                        range,
                        "context-mismatch-before-launch",
                        Ok(()),
                        result,
                    );
                    raise_named(env, range, "RuntimeError");
                    env.alive = false;
                    return OperationEffect::applied("context-mismatch-before-launch");
                }
                states(env, id, handle_state | UNCERTAIN);
                queue_raise(env, range, None);
                states(env, id, STARTED | (handle_state & UNCERTAIN));
                if let Some(worker) = &handle.worker {
                    prepare_worker(id, worker, &handle.origin, env, file, range, result);
                }
                OperationEffect::applied("start")
            }
            ProcessOperation::Join(timeout) => {
                queue_raise(env, range, None);
                match timeout {
                    JoinTimeout::Untimed if qualified_child_wait(env) => {
                        complete_worker(id, env, file, range, functions, result);
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
                    let outcome = if args.is_some_and(prelaunch_failure) {
                        work::WorkerExit {
                            normal: false,
                            errors: BTreeSet::from([None]),
                        }
                    } else {
                        work::for_worker(env, submission["func"], functions)
                    };
                    work::register(
                        env,
                        None,
                        work::Kind::Pool(id.into()),
                        file,
                        range,
                        outcome,
                        result,
                    );
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
                        work::owner_finished(env, &work::Kind::Pool(id.into()));
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

fn record_spawn(
    file: &SourceFile,
    range: SourceRange,
    effect: &str,
    applies: Result<(), &str>,
    result: &mut Inspection,
) {
    runtime::use_model(
        "multiprocessing.Process.start",
        effect,
        SPAWN_MODEL,
        applies,
        file,
        range,
        result,
    );
}

fn report_spawn_issue(
    file: &SourceFile,
    range: SourceRange,
    origin: &Span,
    message: &str,
    result: &mut Inspection,
) {
    report(
        result,
        "TP7005",
        message,
        span(file, range),
        vec![origin.clone()],
        "definite",
        vec![format!(
            "{SPAWN_MODEL}@{}",
            crate::foundation::CATALOG_REVISION
        )],
    );
}

fn prepare_worker(
    id: &str,
    candidate: &WorkerCandidate,
    origin: &Span,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let process = &env.ownership.as_ref().unwrap().processes;
    let declared = runtime::runtime_profile(env)
        .ok()
        .and_then(|profile| profile.spawn_main_script.as_deref());
    if process.entry_path == EntryPath::Unknown
        && !env.module_scope
        && env.call_depth == 0
        && declared.is_some()
    {
        // Top-level function bodies are also checked as declarations. Their
        // launch context is checked separately at a source-resolved invocation.
        return;
    }
    if declared.is_none() {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("worker entry requires a declared direct spawn main script"),
            result,
        );
        return;
    }
    if process.entry_path != EntryPath::Protected {
        if (env.module_scope && launch_matches(env, file))
            || process.entry_path == EntryPath::ImportTime
        {
            report_spawn_issue(file, range, origin, "spawn startup runs during import or outside the declared direct script's main guard", result);
        }
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("spawn startup is outside a resolved direct script main guard"),
            result,
        );
        return;
    }
    let WorkerCandidate::Ready { target, arguments } = candidate else {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("spawn target, argument serialization or primitive context is unresolved"),
            result,
        );
        return;
    };
    if process.workers.len() >= MAX_CHILDREN {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("spawn actor budget exceeded"),
            result,
        );
        return;
    }
    let channels = protocol::spawn_channels(env, arguments);
    if process.workers.values().any(|worker| {
        channels
            .iter()
            .any(|channel| worker.bindings.touches(channel))
    }) {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("more than one worker may use this IPC link"),
            result,
        );
        return;
    }
    let Some(bindings) = protocol::prepare_spawn(env, id, arguments) else {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("spawn could not establish distinct local primitive copies"),
            result,
        );
        return;
    };
    env.ownership.as_mut().unwrap().processes.workers.insert(
        id.into(),
        PendingWorker {
            target: target.clone(),
            bindings,
            evaluated: None,
            projected: BTreeSet::new(),
        },
    );
    record_spawn(file, range, "qualified-worker-entry", Ok(()), result);
}

fn evaluate_worker(
    id: &str,
    worker: &PendingWorker,
    parent: &State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<protocol::WorkerEffects> {
    let Some(definition) = functions.get(&worker.target) else {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("the selected spawn target is unavailable"),
            result,
        );
        return None;
    };
    if parent.call_depth >= MAX_CALL_DEPTH || parent.call_stack.contains(&worker.target) {
        record_spawn(
            file,
            range,
            "worker-entry",
            Err("spawn worker execution depth is exhausted"),
            result,
        );
        return None;
    }
    let mut ownership = OwnershipState::for_child(parent, false)?;
    ownership.processes.protocol = protocol::child_protocol(parent, id, &worker.bindings);
    ownership.processes.entry_path = EntryPath::Unknown;
    let (function_bindings, class_bindings) = module_bindings_for_child(parent, definition);
    let mut child = State {
        ownership: Some(ownership),
        alive: true,
        scope_module: definition.file.module.clone(),
        target_python_version: parent.target_python_version.clone(),
        models: Arc::clone(&parent.models),
        protocol_machines: parent.protocol_machines.clone(),
        call_depth: parent.call_depth + 1,
        call_stack: parent.call_stack.clone(),
        function_bindings: function_bindings.clone(),
        function_bindings_complete: true,
        class_bindings: class_bindings.clone(),
        class_bindings_complete: true,
        module_function_bindings: function_bindings,
        module_class_bindings: class_bindings,
        allocation_context: vec![id.into()],
        ..State::default()
    };
    child.call_stack.insert(worker.target.clone());
    let local_names = definition
        .parameters
        .iter()
        .cloned()
        .chain(assigned_names(&definition.statements))
        .collect::<BTreeSet<_>>();
    child
        .definitely_bound_names
        .extend(definition.parameters.iter().cloned());
    child.shadowed_functions.extend(local_names.iter().cloned());
    child.shadowed_classes.extend(local_names);
    let exits = execute(
        &definition.statements,
        &mut child,
        &definition.file,
        &definition.contracts,
        functions,
        result,
        0,
    );
    let exit_range = definition
        .statements
        .last()
        .map(statement_range)
        .unwrap_or(range);
    let mut final_paths = exits
        .raises
        .into_iter()
        .chain(exits.returns)
        .collect::<Vec<_>>();
    if child.alive {
        final_paths.push(FlowExit {
            state: child,
            range: exit_range,
            kind: CompletionKind::Fallthrough,
        });
    }
    for exit in &final_paths {
        report_cleanup_exit(
            result,
            &definition.file,
            exit,
            None,
            exit_range,
            None,
            false,
            functions,
            &BTreeSet::new(),
        );
    }
    let merged = join_states(final_paths.into_iter().map(|exit| exit.state).collect())?;
    Some(protocol::worker_effects(
        parent,
        merged.ownership?.processes.protocol,
    ))
}

fn advance_channel(
    channel: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let Some(processes) = env.ownership.as_ref().map(|ownership| &ownership.processes) else {
        return;
    };
    let mut matching = processes
        .workers
        .iter()
        .filter(|(_, worker)| worker.bindings.touches(channel));
    let Some((id, worker)) = matching.next() else {
        return;
    };
    if matching.next().is_some() || worker.projected.contains(channel) {
        return;
    }
    let (id, mut worker) = (id.clone(), worker.clone());
    if worker.evaluated.is_none() {
        worker.evaluated = evaluate_worker(&id, &worker, env, file, range, functions, result);
    }
    if let Some(effects) = &worker.evaluated {
        protocol::project_channel(env, effects, channel);
        worker.projected.insert(channel.into());
    }
    env.ownership
        .as_mut()
        .unwrap()
        .processes
        .workers
        .insert(id, worker);
}

fn complete_worker(
    id: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    let Some(worker) = env.ownership.as_mut().unwrap().processes.workers.remove(id) else {
        return;
    };
    let effects = worker
        .evaluated
        .clone()
        .or_else(|| evaluate_worker(id, &worker, env, file, range, functions, result));
    if let Some(effects) = effects {
        protocol::complete_worker(env, &effects, id, &worker.projected);
    }
}

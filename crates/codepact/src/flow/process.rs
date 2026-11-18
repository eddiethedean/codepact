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
        bind_handle_name(env, name);
        env.ownership
            .as_mut()
            .unwrap()
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
    bind_handle_name(env, name);
    let processes = &mut env.ownership.as_mut().unwrap().processes;
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
    bind_handle_name(env, name);
    env.ownership
        .as_mut()
        .unwrap()
        .processes
        .aliases
        .insert(name.into(), id);
}

fn bind_handle_name(env: &mut State, name: &str) {
    env.ownership.as_mut().unwrap().rebind(name);
    env.places.remove(name);
    env.unknown_places.remove(name);
    env.possibly_unbound_places.remove(name);
    env.coroutine_aliases.remove(name);
    env.unknown_coroutine_aliases.remove(name);
    env.definitely_bound_names.insert(name.into());
    env.shadowed_classes.insert(name.into());
    env.shadowed_functions.insert(name.into());
}

fn handle(env: &State, alias: &str) -> Option<(String, LocalHandle)> {
    let processes = &env.ownership.as_ref()?.processes;
    let id = processes.aliases.get(alias)?;
    Some((id.clone(), processes.handles.get(id)?.clone()))
}

pub(super) fn pool_context(expression: &AstExpr, env: &State) -> bool {
    modeled_mode(env)
        && (constructor(expression, env).is_some_and(|(kind, ..)| kind == HandleKind::Pool)
            || expr_name(expression)
                .and_then(|name| handle(env, name))
                .is_some_and(|(_, handle)| handle.kind == HandleKind::Pool))
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

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if !modeled_mode(env) {
        return false;
    }
    if let Some((function, _, _)) = call_parts(expression) {
        if ownership::external_identity(function, env).as_deref() == Some("external:signal.signal")
        {
            env.ownership.as_mut().unwrap().processes.custom_wait = true;
            return false;
        }
    }
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return false;
    };
    let Some((id, handle)) = expr_name(receiver).and_then(|name| handle(env, name)) else {
        return false;
    };
    let no_args = arguments.is_empty() && keywords.is_empty();
    if handle.kind == HandleKind::Process && method == "join" {
        let unpacked = arguments
            .iter()
            .any(|arg| matches!(arg.kind, AstExprKind::Starred(_)))
            || keywords.iter().any(|(name, _)| name.is_none());
        if unpacked {
            runtime::use_model(
                handle.kind.symbol(),
                method,
                PROCESS_MODEL,
                Err("Process.join does not resolve unpacked arguments"),
                file,
                expression.range,
                result,
            );
            queue_raise(env, expression.range, None);
            return true;
        }
        if arguments.len() + keywords.len() > 1
            || keywords
                .iter()
                .any(|(name, _)| name.as_deref() != Some("timeout"))
        {
            raise_named(env, expression.range, "TypeError");
            env.alive = false;
            runtime::use_model(
                handle.kind.symbol(),
                "join-binding-rejected",
                PROCESS_MODEL,
                Ok(()),
                file,
                expression.range,
                result,
            );
            return true;
        }
    }
    let submission = if handle.kind == HandleKind::Pool && method == "apply_async" {
        match bind_arguments(pool_submission_signature(), arguments, keywords, true) {
            Ok(bindings) => bindings,
            Err(BindingFailure::Invalid) => {
                raise_named(env, expression.range, "TypeError");
                env.alive = false;
                runtime::use_model(
                    handle.kind.symbol(),
                    "submission-rejected",
                    POOL_MODEL,
                    Ok(()),
                    file,
                    expression.range,
                    result,
                );
                return true;
            }
            Err(BindingFailure::Unknown) => {
                queue_raise(env, expression.range, None);
                runtime::use_model(
                    handle.kind.symbol(),
                    method,
                    POOL_MODEL,
                    Err("Pool.apply_async argument binding is unresolved"),
                    file,
                    expression.range,
                    result,
                );
                return true;
            }
        }
    } else {
        BTreeMap::new()
    };
    let (allowed, exception) = match (handle.kind, method) {
        (HandleKind::Process, "start") if no_args => (NEW, "AssertionError"),
        (HandleKind::Process, "join") => (STARTED | ENDED, "AssertionError"),
        (HandleKind::Process, "close") if no_args => (NEW | STARTED | ENDED | CLOSED, "ValueError"),
        (HandleKind::Process, "is_alive") if no_args => (NEW | STARTED | ENDED, "ValueError"),
        (HandleKind::Pool, "close" | "terminate") if no_args => (
            STARTED | DRAINING | FORCED | JOINED | FORCED_JOINED,
            "ValueError",
        ),
        (HandleKind::Pool, "join") if no_args => {
            (DRAINING | FORCED | JOINED | FORCED_JOINED, "ValueError")
        }
        (HandleKind::Pool, "apply_async") => (STARTED, "ValueError"),
        _ => {
            runtime::use_model(
                handle.kind.symbol(),
                method,
                handle.kind.model(),
                Err("operation or arguments are outside the local multiprocessing model"),
                file,
                expression.range,
                result,
            );
            states(env, &id, handle.states | UNCERTAIN);
            queue_raise(env, expression.range, None);
            return true;
        }
    };
    let range = expression.range;
    if !permit(
        env,
        &id,
        &handle,
        (method, allowed, exception),
        file,
        range,
        result,
    ) {
        return true;
    }
    let handle_state = env.ownership.as_ref().unwrap().processes.handles[&id].states;
    let mut applies = Ok(());
    let mut effect = method;
    match (handle.kind, method) {
        (HandleKind::Process, "start") if handle.prelaunch_failure => {
            effect = "serialization-rejected-before-launch";
            raise_named(env, range, "AttributeError");
            env.alive = false;
        }
        (HandleKind::Process, "start") => {
            states(env, &id, handle_state | UNCERTAIN);
            queue_raise(env, range, None);
            states(env, &id, STARTED | (handle_state & UNCERTAIN));
        }
        (HandleKind::Process, "join") => {
            let timeout = arguments
                .first()
                .or_else(|| keywords.first().map(|(_, value)| value));
            let untimed =
                timeout.is_none_or(|value| matches!(value.kind, AstExprKind::NoneLiteral));
            let timed = timeout.is_some_and(|value| matches!(value.kind, AstExprKind::Primitive));
            queue_raise(env, range, None);
            if untimed && qualified_child_wait(env) {
                states(env, &id, ENDED | (handle_state & UNCERTAIN));
            } else if untimed {
                applies = Err(
                    "untimed Process.join requires qualified child reaping and signal handling",
                );
            } else if !timed {
                applies = Err("Process.join timeout is outside the qualified literal arguments");
            }
        }
        (HandleKind::Process, "close") => {
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
                states(env, &id, STARTED | (handle_state & UNCERTAIN));
                raise_named(env, range, "ValueError");
            }
            states(env, &id, CLOSED | (handle_state & UNCERTAIN));
        }
        (HandleKind::Pool, "apply_async") => {
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
                    .get_mut(&id)
                    .unwrap()
                    .admitted_work = true;
                effect = if args.is_some_and(prelaunch_failure) {
                    "submission-admitted-with-serialization-failure"
                } else {
                    "submission-admitted-result-unobserved"
                };
            } else {
                applies = Err("Pool.apply_async requires a resolved local worker, plain arguments and no callbacks");
            }
        }
        (HandleKind::Pool, "close") => {
            queue_raise(env, range, None);
            states(
                env,
                &id,
                (handle_state & !STARTED)
                    | if handle_state & STARTED != 0 {
                        DRAINING
                    } else {
                        0
                    },
            );
        }
        (HandleKind::Pool, "terminate") => {
            states(env, &id, FORCED | UNCERTAIN);
            queue_raise(env, range, None);
            states(env, &id, FORCED | (handle_state & UNCERTAIN));
        }
        (HandleKind::Pool, "join") => {
            states(env, &id, handle_state | UNCERTAIN);
            queue_raise(env, range, None);
            if qualified_child_wait(env) {
                let graceful = handle_state & (DRAINING | JOINED) != 0;
                let forced = handle_state & (FORCED | FORCED_JOINED) != 0;
                effect = match (graceful, forced) {
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
                let joined =
                    (if graceful { JOINED } else { 0 }) | (if forced { FORCED_JOINED } else { 0 });
                states(env, &id, joined | (handle_state & UNCERTAIN));
            } else {
                applies = Err("Pool.join requires qualified child reaping and signal handling");
            }
        }
        _ => {}
    }
    runtime::use_model(
        handle.kind.symbol(),
        effect,
        handle.kind.model(),
        applies,
        file,
        range,
        result,
    );
    true
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

pub(super) fn live_pool(
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

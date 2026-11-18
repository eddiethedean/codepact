use super::*;

const MAX_PRIMITIVES: usize = 8;
const MAX_MESSAGES: usize = 16;
const MAX_PERMITS: u8 = 16;
const MAX_MESSAGE_BYTES: usize = 256;
const IPC_CODE: &str = "TP7003";
const SYNC_CODE: &str = "TP7004";
const QUEUE_MODEL: &str = "TPM-IPC-001";
const PIPE_MODEL: &str = "TPM-IPC-002";
const SYNC_MODEL: &str = "TPM-SYNC-002";

#[derive(Clone, Copy, PartialEq, Eq)]
enum IpcKind {
    Queue,
    JoinableQueue,
    SimpleQueue,
    Connection,
}

impl IpcKind {
    fn symbol(self) -> &'static str {
        match self {
            Self::Queue => "multiprocessing.Queue",
            Self::JoinableQueue => "multiprocessing.JoinableQueue",
            Self::SimpleQueue => "multiprocessing.SimpleQueue",
            Self::Connection => "multiprocessing.Connection",
        }
    }

    fn model(self) -> &'static str {
        match self {
            Self::Queue | Self::JoinableQueue => QUEUE_MODEL,
            Self::SimpleQueue | Self::Connection => PIPE_MODEL,
        }
    }

    fn has_feeder(self) -> bool {
        matches!(self, Self::Queue | Self::JoinableQueue)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Endpoint {
    kind: IpcKind,
    channel: String,
    side: usize,
    reader: bool,
    writer: bool,
    closed: bool,
    feeder: bool,
    joined: bool,
    bypassed: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Message {
    origin: Span,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Channel {
    buffered: [Vec<Message>; 2],
    received: BTreeMap<String, Message>,
    dropped: Vec<Message>,
    unfinished: usize,
    capacity: Option<usize>,
    invalid_accounting: bool,
    unknown: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncKind {
    Lock,
    RLock,
    Semaphore,
    BoundedSemaphore,
}

impl SyncKind {
    fn symbol(self) -> &'static str {
        match self {
            Self::Lock => "multiprocessing.Lock",
            Self::RLock => "multiprocessing.RLock",
            Self::Semaphore => "multiprocessing.Semaphore",
            Self::BoundedSemaphore => "multiprocessing.BoundedSemaphore",
        }
    }
}

#[derive(Clone, Copy, Ord, PartialOrd, PartialEq, Eq)]
struct Counter {
    available: u8,
    held: u8,
}

#[derive(Clone, PartialEq, Eq)]
struct Guard {
    kind: SyncKind,
    limit: Option<u8>,
    counts: BTreeSet<Counter>,
}

#[derive(Clone, PartialEq, Eq)]
enum PrimitiveKind {
    Endpoint(Endpoint),
    Guard(Guard),
    Shared {
        lock: Option<String>,
        array_len: Option<usize>,
    },
}

#[derive(Clone, PartialEq, Eq)]
struct Primitive {
    kind: PrimitiveKind,
    origin: Span,
    owner: String,
    unknown: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::flow) struct AcquireTest {
    id: String,
    acquired: BTreeSet<Counter>,
    failed: BTreeSet<Counter>,
    positive: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct LocalProtocol {
    aliases: BTreeMap<String, String>,
    primitives: BTreeMap<String, Primitive>,
    channels: BTreeMap<String, Channel>,
    bool_results: BTreeMap<String, AcquireTest>,
    plain_values: BTreeSet<String>,
    foreign_fork_guards: BTreeSet<String>,
    actor: String,
}

impl LocalProtocol {
    pub(super) fn rebind(&mut self, name: &str) {
        self.aliases.remove(name);
        self.bool_results.remove(name);
        self.plain_values.remove(name);
        self.foreign_fork_guards.remove(name);
    }

    pub(super) fn bind_alias(&mut self, name: &str, source: Option<&str>) {
        let object = source.and_then(|source| self.aliases.get(source)).cloned();
        let predicate = source
            .and_then(|source| self.bool_results.get(source))
            .cloned();
        let plain = source.is_some_and(|source| self.plain_values.contains(source));
        let foreign_fork = source.is_some_and(|source| self.foreign_fork_guards.contains(source));
        self.rebind(name);
        if let Some(id) = object {
            self.aliases.insert(name.to_owned(), id);
        }
        if let Some(predicate) = predicate {
            self.bool_results.insert(name.to_owned(), predicate);
        }
        if plain {
            self.plain_values.insert(name.to_owned());
        }
        if foreign_fork {
            self.foreign_fork_guards.insert(name.to_owned());
        }
    }

    pub(super) fn has_handle(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }

    pub(super) fn has_aliases(&self) -> bool {
        !self.aliases.is_empty()
    }

    pub(super) fn join(&mut self, other: &Self) {
        self.aliases
            .retain(|name, id| other.aliases.get(name) == Some(id));
        self.bool_results
            .retain(|name, test| other.bool_results.get(name) == Some(test));
        self.plain_values
            .retain(|name| other.plain_values.contains(name));
        self.foreign_fork_guards
            .retain(|name| other.foreign_fork_guards.contains(name));
        for (id, primitive) in &mut self.primitives {
            match other.primitives.get(id) {
                Some(right) => match (&mut primitive.kind, &right.kind) {
                    (PrimitiveKind::Guard(left), PrimitiveKind::Guard(right_guard))
                        if left.kind == right_guard.kind && left.limit == right_guard.limit =>
                    {
                        left.counts.extend(right_guard.counts.iter().copied());
                        primitive.unknown |= right.unknown;
                    }
                    (left, right_kind) => primitive.unknown |= left != right_kind || right.unknown,
                },
                None => primitive.unknown = true,
            }
        }
        for (id, primitive) in &other.primitives {
            self.primitives.entry(id.clone()).or_insert_with(|| {
                let mut primitive = primitive.clone();
                primitive.unknown = true;
                primitive
            });
        }
        for (id, channel) in &mut self.channels {
            channel.unknown |= other.channels.get(id) != Some(channel);
        }
        for (id, channel) in &other.channels {
            self.channels.entry(id.clone()).or_insert_with(|| Channel {
                unknown: true,
                ..channel.clone()
            });
        }
    }

    fn primitive(&self, name: &str) -> Option<(&str, &Primitive)> {
        let id = self.aliases.get(name)?;
        Some((id, self.primitives.get(id)?))
    }

    fn invalidate_tests(&mut self, id: &str) {
        self.bool_results.retain(|_, test| test.id != id);
    }

    pub(super) fn escape(
        &mut self,
        names: &BTreeSet<String>,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        for name in names {
            let Some(id) = self.aliases.get(name).cloned() else {
                continue;
            };
            self.invalidate_tests(&id);
            let primitive = self.primitives.get_mut(&id).unwrap();
            primitive.unknown = true;
            if let PrimitiveKind::Endpoint(endpoint) = &primitive.kind {
                self.channels.get_mut(&endpoint.channel).unwrap().unknown = true;
            }
            boundary(
                result,
                file,
                range,
                "multiprocessing primitive escapes its qualified local protocol",
            );
        }
    }
}

fn protocol(env: &State) -> Option<&LocalProtocol> {
    Some(&env.ownership.as_ref()?.processes.protocol)
}

fn protocol_mut(env: &mut State) -> &mut LocalProtocol {
    &mut env
        .ownership
        .as_mut()
        .expect("modeled process")
        .processes
        .protocol
}

pub(super) fn plain_message(expr: &AstExpr, env: &State, file: &SourceFile) -> bool {
    plain_size(expr, env, file, 0).is_some_and(|size| size <= MAX_MESSAGE_BYTES)
}

fn plain_size(expr: &AstExpr, env: &State, file: &SourceFile, depth: usize) -> Option<usize> {
    if depth >= 4 {
        return None;
    }
    match &expr.kind {
        AstExprKind::Boolean(_) | AstExprKind::NoneLiteral => Some(16),
        AstExprKind::Primitive => {
            let source = file.source.get(expr.range.start..expr.range.end)?;
            let parsed = ruff_python_parser::parse_expression(source).ok()?;
            matches!(
                parsed.into_syntax().body.as_ref(),
                ruff_python_ast::Expr::NumberLiteral(_)
            )
            .then_some(source.len().saturating_add(32))
        }
        AstExprKind::String(value) => Some(value.len().saturating_add(32)),
        AstExprKind::Name(name) => protocol(env)?
            .plain_values
            .contains(name)
            .then_some(MAX_MESSAGE_BYTES),
        AstExprKind::Tuple(values) if values.len() <= MAX_MESSAGES => {
            values.iter().try_fold(32usize, |total, value| {
                Some(total.saturating_add(plain_size(value, env, file, depth + 1)?))
            })
        }
        // Bytes literals are lowered as unsupported syntax with the source range intact.
        AstExprKind::OtherCalls(calls, names) if calls.is_empty() && names.is_empty() => file
            .source
            .get(expr.range.start..expr.range.end)
            .filter(|source| source.len() <= MAX_MESSAGE_BYTES)
            .and_then(|source| {
                let parsed = ruff_python_parser::parse_expression(source).ok()?;
                matches!(
                    parsed.into_syntax().body.as_ref(),
                    ruff_python_ast::Expr::BytesLiteral(_)
                )
                .then_some(source.len().saturating_add(32))
            }),
        _ => None,
    }
}

fn rejected_payload(expr: &AstExpr, env: &State) -> bool {
    match &expr.kind {
        AstExprKind::Closure(_) => true,
        AstExprKind::Name(name) => env
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.has_closure(name)),
        AstExprKind::Tuple(items) => items.iter().any(|item| rejected_payload(item, env)),
        _ => false,
    }
}

fn nonnegative(file: &SourceFile, expr: &AstExpr) -> Option<usize> {
    if !matches!(expr.kind, AstExprKind::Primitive) {
        return None;
    }
    file.source
        .get(expr.range.start..expr.range.end)?
        .replace('_', "")
        .parse()
        .ok()
}

fn integer(file: &SourceFile, expr: &AstExpr) -> bool {
    file.source
        .get(expr.range.start..expr.range.end)
        .is_some_and(|source| source.replace('_', "").parse::<i64>().is_ok())
}

fn binding<'a>(
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
    names: &[&str],
) -> Option<Vec<Option<&'a AstExpr>>> {
    if arguments.len() > names.len() {
        return None;
    }
    let mut values = vec![None; names.len()];
    for (slot, arg) in values.iter_mut().zip(arguments) {
        if matches!(arg.kind, AstExprKind::Starred(_)) {
            return None;
        }
        *slot = Some(arg);
    }
    for (name, arg) in keywords {
        let index = names
            .iter()
            .position(|slot| Some(*slot) == name.as_deref())?;
        if values[index].replace(arg).is_some() {
            return None;
        }
    }
    Some(values)
}

fn boolean(value: Option<&AstExpr>, default: bool) -> Option<bool> {
    match value.map(|value| &value.kind) {
        None => Some(default),
        Some(AstExprKind::Boolean(value)) => Some(*value),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum ConstructorKind {
    Queue(IpcKind),
    Pipe,
    Guard(SyncKind),
    Shared(bool),
}

impl ConstructorKind {
    fn model(self) -> &'static str {
        match self {
            Self::Queue(kind) => kind.model(),
            Self::Pipe => PIPE_MODEL,
            Self::Guard(_) | Self::Shared(_) => SYNC_MODEL,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Queue(kind) => kind.symbol(),
            Self::Pipe => "multiprocessing.Pipe",
            Self::Guard(kind) => kind.symbol(),
            Self::Shared(false) => "multiprocessing.Value",
            Self::Shared(true) => "multiprocessing.Array",
        }
    }
}

type ProtocolConstructor<'a> = (
    ConstructorKind,
    Option<&'a str>,
    &'a [AstExpr],
    &'a [CallKeyword],
);

fn protocol_constructor<'a>(value: &'a AstExpr, env: &'a State) -> Option<ProtocolConstructor<'a>> {
    let (function, arguments, keywords) = call_parts(value)?;
    let direct = ownership::external_identity(function, env);
    let (name, method) = if let Some(name) = direct
        .as_deref()
        .and_then(|id| id.strip_prefix("external:multiprocessing."))
    {
        (name, None)
    } else {
        let AstExprKind::Attribute(receiver, name) = &function.kind else {
            return None;
        };
        let method = match context_expression(receiver, env) {
            Some(method) => method,
            None => Some(
                env.ownership
                    .as_ref()?
                    .processes
                    .contexts
                    .get(expr_name(receiver)?)?
                    .as_str(),
            ),
        };
        (name.as_str(), method)
    };
    let kind = match name {
        "Queue" => ConstructorKind::Queue(IpcKind::Queue),
        "JoinableQueue" => ConstructorKind::Queue(IpcKind::JoinableQueue),
        "SimpleQueue" => ConstructorKind::Queue(IpcKind::SimpleQueue),
        "Pipe" => ConstructorKind::Pipe,
        "Lock" => ConstructorKind::Guard(SyncKind::Lock),
        "RLock" => ConstructorKind::Guard(SyncKind::RLock),
        "Semaphore" => ConstructorKind::Guard(SyncKind::Semaphore),
        "BoundedSemaphore" => ConstructorKind::Guard(SyncKind::BoundedSemaphore),
        "Value" => ConstructorKind::Shared(false),
        "Array" => ConstructorKind::Shared(true),
        _ => return None,
    };
    Some((kind, method, arguments, keywords))
}

fn construct_allowed(
    env: &State,
    kind: ConstructorKind,
    method: Option<&str>,
    supported: bool,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let applies = runtime::runtime_profile(env)
        .and_then(|profile| profile.spawn(&env.target_python_version, method.unwrap_or_default()))
        .and_then(|()| supported.then_some(()).ok_or("multiprocessing primitive requires qualified literal arguments and exact local bindings"));
    runtime::use_model(
        kind.symbol(),
        "construct",
        kind.model(),
        applies,
        file,
        range,
        result,
    )
}

fn fresh_id(env: &State, file: &SourceFile, range: SourceRange) -> String {
    format!(
        "{}:{}:{}",
        file.path,
        range.start,
        env.allocation_context.join("/")
    )
}

fn budget(
    env: &State,
    ids: &[&str],
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let state = protocol(env).expect("modeled process primitive");
    if state.primitives.len() + ids.len() > MAX_PRIMITIVES
        || ids.iter().any(|id| state.primitives.contains_key(*id))
    {
        boundary(
            result,
            file,
            range,
            "multiprocessing primitive identity or budget exhausted",
        );
        return false;
    }
    true
}

fn bind_primitive(env: &mut State, name: &str, id: String, kind: PrimitiveKind, origin: Span) {
    let state = &mut env.bind_runtime_handle(name).processes.protocol;
    state.aliases.insert(name.into(), id.clone());
    state.primitives.insert(
        id,
        Primitive {
            kind,
            origin,
            owner: state.actor.clone(),
            unknown: false,
        },
    );
}

fn bind_existing(env: &mut State, name: &str, id: String) {
    env.bind_runtime_handle(name)
        .processes
        .protocol
        .aliases
        .insert(name.into(), id);
}

fn make_guard(kind: SyncKind, initial: u8) -> PrimitiveKind {
    let limit = match kind {
        SyncKind::Lock | SyncKind::RLock => Some(1),
        SyncKind::BoundedSemaphore => Some(initial),
        SyncKind::Semaphore => None,
    };
    PrimitiveKind::Guard(Guard {
        kind,
        limit,
        counts: BTreeSet::from([Counter {
            available: initial,
            held: 0,
        }]),
    })
}

fn endpoint(
    kind: IpcKind,
    channel: String,
    side: usize,
    reader: bool,
    writer: bool,
) -> PrimitiveKind {
    PrimitiveKind::Endpoint(Endpoint {
        kind,
        channel,
        side,
        reader,
        writer,
        closed: false,
        feeder: false,
        joined: false,
        bypassed: false,
    })
}

fn init_guard(
    kind: SyncKind,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
    file: &SourceFile,
) -> Option<u8> {
    if matches!(kind, SyncKind::Lock | SyncKind::RLock) {
        return (args.is_empty() && kwargs.is_empty()).then_some(1);
    }
    let [value] = binding(args, kwargs, &["value"])?.try_into().ok()?;
    let initial = value.map_or(Some(1), |value| nonnegative(file, value))?;
    (initial <= MAX_PERMITS as usize).then_some(initial as u8)
}

fn init_shared(
    array: bool,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
    env: &State,
    file: &SourceFile,
) -> Option<(Option<usize>, Option<Option<String>>)> {
    if args.len() != 2 || !matches!(&args[0].kind, AstExprKind::String(value) if value == "i") {
        return None;
    }
    let array_len = if array {
        let len = match &args[1].kind {
            AstExprKind::Tuple(values) if values.iter().all(|v| integer(file, v)) => values.len(),
            _ => nonnegative(file, &args[1])?,
        };
        Some((1..=8).contains(&len).then_some(len)?)
    } else {
        integer(file, &args[1]).then_some(())?;
        None
    };
    let lock = match kwargs {
        [] => None,
        [(Some(key), value)] if key == "lock" => match &value.kind {
            AstExprKind::Boolean(true) | AstExprKind::NoneLiteral => None,
            AstExprKind::Boolean(false) => Some(None),
            AstExprKind::Name(name) => {
                let (id, primitive) = protocol(env)?.primitive(name)?;
                if primitive.unknown
                    || !matches!(&primitive.kind, PrimitiveKind::Guard(guard) if guard.kind == SyncKind::RLock)
                {
                    return None;
                }
                Some(Some(id.to_owned()))
            }
            _ => return None,
        },
        _ => return None,
    };
    Some((array_len, lock))
}

pub(super) fn initialize(
    name: &str,
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    if let Some((receiver, method, args, kwargs)) = called_method(expr) {
        if args.is_empty() && kwargs.is_empty() {
            if let Some((_, primitive)) =
                expr_name(receiver).and_then(|receiver| protocol(env)?.primitive(receiver))
            {
                match (&primitive.kind, method) {
                    (PrimitiveKind::Shared { lock, .. }, "get_lock") => {
                        if !primitive.unknown {
                            if let Some(lock) = lock.clone() {
                                bind_existing(env, name, lock);
                                return true;
                            }
                        }
                        runtime::use_model(
                            "multiprocessing.sharedctypes.get_lock",
                            "guard-identity",
                            SYNC_MODEL,
                            Err("shared storage has no resolved wrapper guard"),
                            file,
                            expr.range,
                            result,
                        );
                        return false;
                    }
                    (PrimitiveKind::Endpoint(_), "get" | "recv") => {
                        let received =
                            receive(expr, receiver, method, env, file, functions, result);
                        if received.is_some() && env.alive {
                            let state = &mut env.bind_runtime_handle(name).processes.protocol;
                            if received == Some(true) {
                                state.plain_values.insert(name.to_owned());
                            }
                        }
                        return received.is_some();
                    }
                    _ => {}
                }
            }
        }
        if method == "acquire" && nonblocking(args, kwargs) == Some(true) {
            if let Some(test) = try_acquire(receiver, expr.range, env, file, result) {
                env.bind_runtime_handle(name)
                    .processes
                    .protocol
                    .bool_results
                    .insert(name.into(), test);
                return true;
            }
        }
    }
    let Some((kind, method, args, kwargs)) = protocol_constructor(expr, env) else {
        return false;
    };
    let id = fresh_id(env, file, expr.range);
    let origin = allocation_origin(file, expr.range);
    match kind {
        ConstructorKind::Queue(ipc) => {
            let shape = if ipc == IpcKind::SimpleQueue {
                (args.is_empty() && kwargs.is_empty()).then_some(None)
            } else {
                binding(args, kwargs, &["maxsize"]).and_then(|values| {
                    values[0]
                        .map_or(Some(0), |value| nonnegative(file, value))
                        .map(|cap| (cap > 0).then_some(cap))
                })
            };
            if !construct_allowed(env, kind, method, shape.is_some(), file, expr.range, result)
                || !budget(env, &[&id], file, expr.range, result)
            {
                return false;
            }
            queue_raise(env, expr.range, None);
            protocol_mut(env).channels.insert(
                id.clone(),
                Channel {
                    capacity: shape.unwrap(),
                    ..Channel::default()
                },
            );
            bind_primitive(
                env,
                name,
                id.clone(),
                endpoint(ipc, id, 0, true, true),
                origin,
            );
        }
        ConstructorKind::Pipe => {
            runtime::use_model(
                kind.symbol(),
                "construct",
                kind.model(),
                Err("Pipe requires fixed two-name unpacking"),
                file,
                expr.range,
                result,
            );
            return false;
        }
        ConstructorKind::Guard(sync) => {
            let initial = init_guard(sync, args, kwargs, file);
            if method == Some("fork")
                && initial.is_some()
                && runtime::runtime_profile(env)
                    .is_ok_and(|profile| profile.applicability(&env.target_python_version).is_ok())
            {
                construct_allowed(env, kind, method, true, file, expr.range, result);
                queue_raise(env, expr.range, None);
                env.bind_runtime_handle(name)
                    .processes
                    .protocol
                    .foreign_fork_guards
                    .insert(name.into());
                return true;
            }
            if !construct_allowed(
                env,
                kind,
                method,
                initial.is_some(),
                file,
                expr.range,
                result,
            ) || !budget(env, &[&id], file, expr.range, result)
            {
                return false;
            }
            queue_raise(env, expr.range, None);
            bind_primitive(env, name, id, make_guard(sync, initial.unwrap()), origin);
        }
        ConstructorKind::Shared(array) => {
            let shared = init_shared(array, args, kwargs, env, file);
            if !construct_allowed(
                env,
                kind,
                method,
                shared.is_some(),
                file,
                expr.range,
                result,
            ) {
                return false;
            }
            let (array_len, lock) = shared.unwrap();
            let guard_id = format!("{id}:lock");
            let needed = if lock.is_none() {
                vec![id.as_str(), guard_id.as_str()]
            } else {
                vec![id.as_str()]
            };
            if !budget(env, &needed, file, expr.range, result) {
                return false;
            }
            queue_raise(env, expr.range, None);
            let lock = lock.unwrap_or_else(|| {
                let owner = protocol(env).unwrap().actor.clone();
                protocol_mut(env).primitives.insert(
                    guard_id.clone(),
                    Primitive {
                        kind: make_guard(SyncKind::RLock, 1),
                        origin: origin.clone(),
                        owner,
                        unknown: false,
                    },
                );
                Some(guard_id)
            });
            bind_primitive(
                env,
                name,
                id,
                PrimitiveKind::Shared { lock, array_len },
                origin,
            );
        }
    }
    true
}

pub(in crate::flow) fn initialize_pipe(
    value: &AstExpr,
    target: Option<&AstExpr>,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if !modeled_mode(env) {
        return false;
    }
    let Some((kind @ ConstructorKind::Pipe, method, args, kwargs)) =
        protocol_constructor(value, env)
    else {
        return false;
    };
    let names = target.and_then(|target| {
        let (AstExprKind::Tuple(names) | AstExprKind::Collection(names)) = &target.kind else {
            return None;
        };
        let [left, right] = names.as_slice() else {
            return None;
        };
        let (left, right) = (expr_name(left)?, expr_name(right)?);
        (left != right).then_some((left, right))
    });
    let duplex = binding(args, kwargs, &["duplex"]).and_then(|values| boolean(values[0], true));
    if !construct_allowed(
        env,
        kind,
        method,
        names.is_some() && duplex.is_some(),
        file,
        value.range,
        result,
    ) {
        return false;
    }
    let (left, right) = names.unwrap();
    let duplex = duplex.unwrap();
    let channel = fresh_id(env, file, value.range);
    let left_id = format!("{channel}:0");
    let right_id = format!("{channel}:1");
    if !budget(env, &[&left_id, &right_id], file, value.range, result) {
        return false;
    }
    queue_raise(env, value.range, None);
    protocol_mut(env)
        .channels
        .insert(channel.clone(), Channel::default());
    let origin = span(file, value.range);
    bind_primitive(
        env,
        left,
        left_id,
        endpoint(IpcKind::Connection, channel.clone(), 0, true, duplex),
        origin.clone(),
    );
    bind_primitive(
        env,
        right,
        right_id,
        endpoint(IpcKind::Connection, channel, 1, duplex, true),
        origin,
    );
    true
}

fn model_use(
    symbol: &str,
    model: &str,
    effect: &str,
    applies: Result<(), &str>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> calls::ModelCall {
    if runtime::use_model(symbol, effect, model, applies, file, range, result) {
        calls::ModelCall::Applied
    } else {
        calls::ModelCall::Declined
    }
}

fn issue(
    (code, model): (&str, &str),
    message: &str,
    origin: &Span,
    file: &SourceFile,
    range: SourceRange,
    certainty: &str,
    result: &mut Inspection,
) {
    report(
        result,
        code,
        message,
        span(file, range),
        vec![origin.clone()],
        certainty,
        vec![format!("{model}@{}", crate::foundation::CATALOG_REVISION)],
    );
}

fn fails(env: &mut State, range: SourceRange, kind: &str) {
    raise_named(env, range, kind);
    env.alive = false;
}

fn endpoint_lookup<'a>(
    env: &'a State,
    expression: &AstExpr,
) -> Option<(&'a str, &'a Primitive, &'a Endpoint)> {
    let (id, primitive) = protocol(env)?.primitive(expr_name(expression)?)?;
    let PrimitiveKind::Endpoint(endpoint) = &primitive.kind else {
        return None;
    };
    Some((id, primitive, endpoint))
}

fn connection_peer(state: &LocalProtocol, ep: &Endpoint, writers: bool) -> Option<bool> {
    let mut uncertain = false;
    for primitive in state.primitives.values() {
        let PrimitiveKind::Endpoint(other) = &primitive.kind else {
            continue;
        };
        if other.channel != ep.channel || other.side == ep.side {
            continue;
        }
        if (writers && other.writer) || (!writers && other.reader) {
            uncertain |= primitive.unknown;
            if !primitive.unknown && !other.closed {
                return Some(true);
            }
        }
    }
    (!uncertain).then_some(false)
}

fn set_endpoint(env: &mut State, id: &str, endpoint: Endpoint) {
    protocol_mut(env).primitives.get_mut(id).unwrap().kind = PrimitiveKind::Endpoint(endpoint);
}

fn ipc_effect(
    ep: &Endpoint,
    effect: &str,
    applies: Result<(), &str>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> calls::ModelCall {
    model_use(
        ep.kind.symbol(),
        ep.kind.model(),
        effect,
        applies,
        file,
        range,
        result,
    )
}

fn live_endpoint(
    ep: &Endpoint,
    primitive: &Primitive,
    method: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    if primitive.unknown {
        ipc_effect(
            ep,
            method,
            Err("local IPC endpoint state is unresolved"),
            file,
            range,
            result,
        );
        queue_raise(env, range, None);
        return false;
    }
    if ep.closed {
        issue(
            (IPC_CODE, ep.kind.model()),
            "IPC operation uses a closed local endpoint",
            &primitive.origin,
            file,
            range,
            "definite",
            result,
        );
        let exception = match ep.kind {
            IpcKind::Queue | IpcKind::JoinableQueue => "ValueError",
            IpcKind::SimpleQueue | IpcKind::Connection => "OSError",
        };
        ipc_effect(ep, "closed-use", Ok(()), file, range, result);
        fails(env, range, exception);
        return false;
    }
    true
}

fn receive(
    expr: &AstExpr,
    receiver: &AstExpr,
    method: &str,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> Option<bool> {
    let (_, primitive, ep) = endpoint_lookup(env, receiver)?;
    let (primitive, ep) = (primitive.clone(), ep.clone());
    if (ep.kind == IpcKind::Connection) != (method == "recv") {
        return None;
    }
    let range = expr.range;
    if !live_endpoint(&ep, &primitive, method, env, file, range, result) {
        return Some(false);
    }
    if !ep.reader {
        issue(
            (IPC_CODE, ep.kind.model()),
            "cannot receive on a write-only connection",
            &primitive.origin,
            file,
            range,
            "definite",
            result,
        );
        ipc_effect(&ep, "wrong-direction", Ok(()), file, range, result);
        fails(env, range, "OSError");
        return Some(false);
    }
    let source = if ep.kind == IpcKind::Connection {
        1 - ep.side
    } else {
        0
    };
    if protocol(env)?.actor.is_empty() {
        super::advance_channel(&ep.channel, env, file, range, functions, result);
    }
    let state = protocol(env)?;
    let channel = &state.channels[&ep.channel];
    if !channel.unknown
        && channel.buffered[source].is_empty()
        && ep.kind == IpcKind::Connection
        && connection_peer(state, &ep, true) == Some(false)
    {
        ipc_effect(&ep, "eof", Ok(()), file, range, result);
        fails(env, range, "EOFError");
        return Some(false);
    }
    if channel.unknown || channel.buffered[source].is_empty() {
        ipc_effect(
            &ep,
            "receive",
            Err("IPC receipt lacks a qualified available delivery and wait"),
            file,
            range,
            result,
        );
        queue_raise(env, range, None);
        return Some(false);
    }
    if ep.kind == IpcKind::JoinableQueue {
        crate::review::operations::record_protocol(
            "process-queue-accounting",
            span(file, range),
            vec![
                ep.kind.symbol().into(),
                super::super::protocol_guard_identity(file, &primitive.origin),
                "match-local-receipt-to-task-done".into(),
            ],
        );
    }
    let actor = state.actor.clone();
    let channel = protocol_mut(env).channels.get_mut(&ep.channel).unwrap();
    let item = channel.buffered[source].remove(0);
    if ep.kind == IpcKind::JoinableQueue && channel.received.insert(actor, item).is_some() {
        channel.unknown = true;
        ipc_effect(
            &ep,
            "receive",
            Err("more than one received item is outstanding for this consumer"),
            file,
            range,
            result,
        );
        return Some(true);
    }
    ipc_effect(&ep, "receive", Ok(()), file, range, result);
    Some(true)
}

fn ipc_call(
    expr: &AstExpr,
    (receiver, method, args, kwargs): MethodCall<'_>,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> calls::ModelCall {
    use calls::ModelCall::{Applied, Declined, Unrecognized};
    let Some((id, primitive, ep)) = endpoint_lookup(env, receiver) else {
        return Unrecognized;
    };
    let (id, primitive, mut ep) = (id.to_owned(), primitive.clone(), ep.clone());
    let range = expr.range;
    if matches!(method, "get" | "recv")
        && args.is_empty()
        && kwargs.is_empty()
        && receive(expr, receiver, method, env, file, functions, result).is_some()
    {
        return Applied;
    }
    match (method, args, kwargs) {
        ("close", [], []) if !primitive.unknown => {
            ep.closed = true;
            set_endpoint(env, &id, ep.clone());
            ipc_effect(&ep, "close", Ok(()), file, range, result)
        }
        ("join_thread", [], []) if ep.kind.has_feeder() => {
            if primitive.unknown || ep.bypassed {
                return ipc_effect(
                    &ep,
                    "feeder-completion",
                    Err("local feeder completion is unresolved or bypassed"),
                    file,
                    range,
                    result,
                );
            }
            if !ep.closed {
                issue(
                    (IPC_CODE, ep.kind.model()),
                    "join_thread requires close on this local queue copy",
                    &primitive.origin,
                    file,
                    range,
                    "definite",
                    result,
                );
                fails(env, range, "AssertionError");
                return ipc_effect(&ep, "feeder-completion", Ok(()), file, range, result);
            }
            let pending = &protocol(env).unwrap().channels[&ep.channel];
            if ep.feeder && (pending.unknown || pending.buffered[0].len() > 1) {
                return ipc_effect(
                    &ep,
                    "feeder-completion",
                    Err("local feeder wait needs a qualified reader drain"),
                    file,
                    range,
                    result,
                );
            }
            ep.joined = true;
            set_endpoint(env, &id, ep.clone());
            ipc_effect(&ep, "feeder-completion", Ok(()), file, range, result)
        }
        ("cancel_join_thread", [], []) if ep.kind.has_feeder() => {
            ep.bypassed = true;
            set_endpoint(env, &id, ep.clone());
            ipc_effect(
                &ep,
                "feeder-bypass",
                Err("cancel_join_thread bypasses the local feeder guarantee"),
                file,
                range,
                result,
            )
        }
        ("put" | "send", [value], []) if (method == "send") == (ep.kind == IpcKind::Connection) => {
            if !live_endpoint(&ep, &primitive, method, env, file, range, result) {
                return Applied;
            }
            if !ep.writer {
                issue(
                    (IPC_CODE, ep.kind.model()),
                    "cannot send on a read-only connection",
                    &primitive.origin,
                    file,
                    range,
                    "definite",
                    result,
                );
                fails(env, range, "OSError");
                return ipc_effect(&ep, "wrong-direction", Ok(()), file, range, result);
            }
            let channel = &protocol(env).unwrap().channels[&ep.channel];
            let rejects = rejected_payload(value, env);
            if rejects && !ep.kind.has_feeder() {
                fails(env, range, "AttributeError");
                return ipc_effect(
                    &ep,
                    "caller-serialization-rejected",
                    Ok(()),
                    file,
                    range,
                    result,
                );
            }
            if !rejects && !plain_message(value, env, file) {
                protocol_mut(env)
                    .channels
                    .get_mut(&ep.channel)
                    .unwrap()
                    .unknown = true;
                if !ep.kind.has_feeder() {
                    queue_raise(env, range, None);
                }
                let effect = if ep.kind.has_feeder() {
                    "feeder-serialization"
                } else {
                    "caller-serialization"
                };
                return ipc_effect(
                    &ep,
                    effect,
                    Err("IPC payload is outside the bounded plain-value transport"),
                    file,
                    range,
                    result,
                );
            }
            if ep.kind == IpcKind::Connection
                && connection_peer(protocol(env).unwrap(), &ep, false) == Some(false)
                && !channel.unknown
            {
                fails(env, range, "BrokenPipeError");
                return ipc_effect(&ep, "broken-delivery", Ok(()), file, range, result);
            }
            let full = channel
                .capacity
                .is_some_and(|capacity| channel.buffered[0].len() >= capacity);
            let message_count = protocol(env)
                .unwrap()
                .channels
                .values()
                .map(|link| {
                    link.buffered.iter().map(Vec::len).sum::<usize>()
                        + link.received.len()
                        + link.dropped.len()
                })
                .sum::<usize>();
            let origin = span(file, range);
            let repeated = channel.buffered[ep.side]
                .iter()
                .chain(channel.received.values())
                .chain(&channel.dropped)
                .any(|item| item.origin == origin);
            if channel.unknown || full || repeated || message_count >= MAX_MESSAGES {
                protocol_mut(env)
                    .channels
                    .get_mut(&ep.channel)
                    .unwrap()
                    .unknown = true;
                return ipc_effect(
                    &ep,
                    "admission",
                    Err(
                        "IPC admission needs a provable wait and distinct bounded message identity",
                    ),
                    file,
                    range,
                    result,
                );
            }
            let channel = protocol_mut(env).channels.get_mut(&ep.channel).unwrap();
            if !rejects {
                channel.buffered[ep.side].push(Message { origin });
            } else if ep.kind == IpcKind::JoinableQueue {
                channel.dropped.push(Message { origin });
            }
            if ep.kind == IpcKind::JoinableQueue {
                channel.unfinished += 1;
            }
            if ep.kind.has_feeder() {
                ep.feeder = true;
                set_endpoint(env, &id, ep.clone());
                if rejects {
                    issue(
                        (IPC_CODE, ep.kind.model()),
                        "queue feeder rejects the payload after local admission",
                        &primitive.origin,
                        file,
                        range,
                        "definite",
                        result,
                    );
                    ipc_effect(
                        &ep,
                        "feeder-serialization-rejected-after-admission",
                        Ok(()),
                        file,
                        range,
                        result,
                    );
                }
                ipc_effect(&ep, "buffer-admission", Ok(()), file, range, result)
            } else {
                ipc_effect(&ep, "write", Ok(()), file, range, result)
            }
        }
        ("task_done", [], []) if ep.kind == IpcKind::JoinableQueue => {
            if primitive.unknown {
                return ipc_effect(
                    &ep,
                    "item-accounting",
                    Err("JoinableQueue accounting receiver is unresolved"),
                    file,
                    range,
                    result,
                );
            }
            let state = protocol_mut(env);
            let channel = state.channels.get_mut(&ep.channel).unwrap();
            if channel.unknown {
                return ipc_effect(
                    &ep,
                    "item-accounting",
                    Err("JoinableQueue item or counter correspondence is unresolved"),
                    file,
                    range,
                    result,
                );
            }
            let duty = channel.received.get(&state.actor).cloned();
            if duty.is_none() {
                issue(
                    (IPC_CODE, ep.kind.model()),
                    "task_done has no matching receipt on this JoinableQueue",
                    &primitive.origin,
                    file,
                    range,
                    "definite",
                    result,
                );
                channel.invalid_accounting = true;
            }
            if channel.unfinished == 0 {
                fails(env, range, "ValueError");
                return ipc_effect(&ep, "counter-rejection", Ok(()), file, range, result);
            }
            channel.unfinished -= 1;
            if duty.is_some() {
                channel.received.remove(&state.actor);
                ipc_effect(&ep, "item-accounting", Ok(()), file, range, result)
            } else {
                ipc_effect(&ep, "counter-only", Ok(()), file, range, result)
            }
        }
        ("join", [], []) if ep.kind == IpcKind::JoinableQueue => {
            if protocol(env).unwrap().channels[&ep.channel].unfinished != 0 {
                super::advance_channel(&ep.channel, env, file, range, functions, result);
            }
            let channel = &protocol(env).unwrap().channels[&ep.channel];
            if primitive.unknown
                || channel.unknown
                || channel.invalid_accounting
                || channel.unfinished != 0
            {
                let origin = channel
                    .received
                    .values()
                    .next()
                    .map(|item| &item.origin)
                    .unwrap_or(&primitive.origin);
                if !channel.unknown && !channel.received.is_empty() {
                    issue(
                        (IPC_CODE, ep.kind.model()),
                        "JoinableQueue.join leaves a known receipt without matching accounting",
                        origin,
                        file,
                        range,
                        "possible",
                        result,
                    );
                }
                return ipc_effect(
                    &ep,
                    "accounting-join",
                    Err("the joined accounting cohort and zero notification are not established"),
                    file,
                    range,
                    result,
                );
            }
            ipc_effect(&ep, "accounting-join", Ok(()), file, range, result)
        }
        ("poll", [timeout], [])
            if ep.kind == IpcKind::Connection && nonnegative(file, timeout) == Some(0) =>
        {
            if !live_endpoint(&ep, &primitive, method, env, file, range, result) {
                return Applied;
            }
            if !ep.reader {
                issue(
                    (IPC_CODE, ep.kind.model()),
                    "cannot poll a write-only connection",
                    &primitive.origin,
                    file,
                    range,
                    "definite",
                    result,
                );
                fails(env, range, "OSError");
                return ipc_effect(&ep, "wrong-direction", Ok(()), file, range, result);
            }
            let state = protocol(env).unwrap();
            let channel = &state.channels[&ep.channel];
            let effect = if !channel.buffered[1 - ep.side].is_empty() {
                "ready-message"
            } else if connection_peer(state, &ep, true) == Some(false) && !channel.unknown {
                "ready-eof"
            } else {
                "poll-status"
            };
            ipc_effect(
                &ep,
                effect,
                (!channel.unknown)
                    .then_some(())
                    .ok_or("connection readiness is unresolved"),
                file,
                range,
                result,
            )
        }
        _ => {
            ipc_effect(
                &ep,
                method,
                Err("IPC operation or argument shape is outside the qualified profile"),
                file,
                range,
                result,
            );
            queue_raise(env, range, None);
            Declined
        }
    }
}

fn guard_lookup<'a>(
    env: &'a State,
    receiver: &AstExpr,
) -> Option<(&'a str, &'a Primitive, &'a Guard)> {
    let (id, primitive) = protocol(env)?.primitive(expr_name(receiver)?)?;
    let PrimitiveKind::Guard(guard) = &primitive.kind else {
        return None;
    };
    Some((id, primitive, guard))
}

fn nonblocking(args: &[AstExpr], kwargs: &[CallKeyword]) -> Option<bool> {
    let values = binding(args, kwargs, &["block", "timeout"])?;
    if values[1].is_some_and(|value| !matches!(value.kind, AstExprKind::NoneLiteral)) {
        return None;
    }
    boolean(values[0], true).map(|block| !block)
}

fn acquired(guard: &Guard, current: Counter) -> Option<Counter> {
    if current.held >= MAX_PERMITS {
        return None;
    }
    if guard.kind == SyncKind::RLock && current.held > 0 {
        return Some(Counter {
            held: current.held + 1,
            ..current
        });
    }
    Some(Counter {
        available: current.available.checked_sub(1)?,
        held: current.held + 1,
    })
}

fn try_acquire(
    receiver: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<AcquireTest> {
    let (id, primitive, guard) = guard_lookup(env, receiver)?;
    let (id, guard) = (id.to_owned(), guard.clone());
    if primitive.unknown || guard.counts.iter().any(|count| count.held >= MAX_PERMITS) {
        model_use(
            guard.kind.symbol(),
            SYNC_MODEL,
            "nonblocking-acquire",
            Err("guard availability or acquisition depth is unresolved"),
            file,
            range,
            result,
        );
        queue_raise(env, range, None);
        return Some(AcquireTest {
            id,
            acquired: BTreeSet::new(),
            failed: BTreeSet::new(),
            positive: true,
        });
    }
    let mut test = AcquireTest {
        id: id.clone(),
        acquired: BTreeSet::new(),
        failed: BTreeSet::new(),
        positive: true,
    };
    for current in guard.counts.iter().copied() {
        if let Some(next) = acquired(&guard, current) {
            test.acquired.insert(next);
        } else {
            test.failed.insert(current);
        }
    }
    set_counts(
        env,
        &id,
        test.acquired.union(&test.failed).copied().collect(),
    );
    model_use(
        guard.kind.symbol(),
        SYNC_MODEL,
        "nonblocking-acquire",
        Ok(()),
        file,
        range,
        result,
    );
    Some(test)
}

fn set_counts(env: &mut State, id: &str, counts: BTreeSet<Counter>) {
    let protocol = protocol_mut(env);
    protocol.invalidate_tests(id);
    let PrimitiveKind::Guard(guard) = &mut protocol.primitives.get_mut(id).unwrap().kind else {
        unreachable!()
    };
    guard.counts = counts;
}

fn blocking_acquire(
    receiver: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<bool> {
    let (id, primitive, guard) = guard_lookup(env, receiver)?;
    let (id, guard) = (id.to_owned(), guard.clone());
    let counts = guard
        .counts
        .iter()
        .copied()
        .map(|count| acquired(&guard, count))
        .collect::<Option<BTreeSet<_>>>();
    if primitive.unknown || counts.is_none() {
        protocol_mut(env).primitives.get_mut(&id).unwrap().unknown = true;
        model_use(
            guard.kind.symbol(),
            SYNC_MODEL,
            "acquire",
            Err("blocking acquisition requires a qualified permit or peer release"),
            file,
            range,
            result,
        );
        // The attempted acquisition cannot establish a cleanup duty for this
        // context. Retain the earlier acquired contexts in the exceptional state.
        queue_raise(env, range, None);
        return Some(false);
    }
    set_counts(env, &id, counts.unwrap());
    model_use(
        guard.kind.symbol(),
        SYNC_MODEL,
        "acquire",
        Ok(()),
        file,
        range,
        result,
    );
    Some(true)
}

fn release(
    receiver: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<()> {
    let (id, primitive, guard) = guard_lookup(env, receiver)?;
    let (id, primitive, guard) = (id.to_owned(), primitive.clone(), guard.clone());
    if primitive.unknown {
        model_use(
            guard.kind.symbol(),
            SYNC_MODEL,
            "release",
            Err("release effect is unresolved for this multiprocessing guard"),
            file,
            range,
            result,
        );
        queue_raise(env, range, None);
        return Some(());
    }
    let no_duty = guard.counts.iter().filter(|count| count.held == 0).count();
    if no_duty != 0 {
        issue(
            (SYNC_CODE, SYNC_MODEL),
            "release has no matching acquisition by this actor",
            &primitive.origin,
            file,
            range,
            if no_duty == guard.counts.len() {
                "definite"
            } else {
                "possible"
            },
            result,
        );
    }
    let mut failed = BTreeSet::new();
    let mut normal = BTreeSet::new();
    for counter in guard.counts.iter().copied() {
        if guard.kind == SyncKind::RLock && counter.held == 0 {
            failed.insert(counter);
        } else if guard.kind == SyncKind::RLock && counter.held > 1 {
            normal.insert(Counter {
                held: counter.held - 1,
                ..counter
            });
        } else if guard.limit.is_some_and(|limit| counter.available >= limit) {
            failed.insert(counter);
        } else if counter.available >= MAX_PERMITS {
            model_use(
                guard.kind.symbol(),
                SYNC_MODEL,
                "release",
                Err("ordinary semaphore counter bound exceeded"),
                file,
                range,
                result,
            );
            protocol_mut(env).primitives.get_mut(&id).unwrap().unknown = true;
            queue_raise(env, range, None);
            return Some(());
        } else {
            normal.insert(Counter {
                held: counter.held.saturating_sub(1),
                available: counter.available + 1,
            });
        }
    }
    if !failed.is_empty() {
        let exception = if guard.kind == SyncKind::RLock {
            "AssertionError"
        } else {
            "ValueError"
        };
        set_counts(env, &id, failed);
        raise_named(env, range, exception);
    }
    if normal.is_empty() {
        env.alive = false;
    } else {
        set_counts(env, &id, normal);
    }
    model_use(
        guard.kind.symbol(),
        SYNC_MODEL,
        "release",
        Ok(()),
        file,
        range,
        result,
    );
    Some(())
}

pub(in crate::flow) fn protocol_test(
    test: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<AcquireTest> {
    if let Some(name) = expr_name(test) {
        return protocol(env)?.bool_results.get(name).cloned();
    }
    if let Some((receiver, "acquire", args, kwargs)) = called_method(test) {
        return (nonblocking(args, kwargs) == Some(true))
            .then(|| try_acquire(receiver, test.range, env, file, result))
            .flatten();
    }
    if protocol(env)?.primitives.is_empty() {
        return None;
    }
    use ruff_python_ast::{CmpOp, Expr, UnaryOp};
    let source = file.source.get(test.range.start..test.range.end)?;
    let expression = ruff_python_parser::parse_expression(source)
        .ok()?
        .into_syntax();
    let (operand, positive) = match expression.body.as_ref() {
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => (unary.operand.as_ref(), false),
        Expr::Compare(compare) if compare.ops.len() == 1 && compare.comparators.len() == 1 => {
            let Expr::BooleanLiteral(boolean) = &compare.comparators[0] else {
                return None;
            };
            let positive = match compare.ops[0] {
                CmpOp::Is | CmpOp::Eq => boolean.value,
                CmpOp::IsNot | CmpOp::NotEq => !boolean.value,
                _ => return None,
            };
            (compare.left.as_ref(), positive)
        }
        _ => return None,
    };
    let mut predicate = match operand {
        Expr::Name(name) => protocol(env)?.bool_results.get(name.id.as_str()).cloned()?,
        Expr::Call(call) => {
            let Expr::Attribute(member) = call.func.as_ref() else {
                return None;
            };
            let Expr::Name(receiver) = member.value.as_ref() else {
                return None;
            };
            if member.attr.as_str() != "acquire" {
                return None;
            }
            let supported = match (
                call.arguments.args.as_ref(),
                call.arguments.keywords.as_ref(),
            ) {
                ([Expr::BooleanLiteral(block)], []) => !block.value,
                ([], [keyword]) => {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "block")
                        && matches!(&keyword.value, Expr::BooleanLiteral(block) if !block.value)
                }
                _ => false,
            };
            if !supported {
                return None;
            }
            try_acquire(
                &AstExpr {
                    kind: AstExprKind::Name(receiver.id.to_string()),
                    range: test.range,
                },
                test.range,
                env,
                file,
                result,
            )?
        }
        _ => return None,
    };
    predicate.positive = positive;
    Some(predicate)
}

pub(in crate::flow) fn refine_protocol_test(env: &mut State, test: &AcquireTest, truth: bool) {
    if test.acquired.is_empty() && test.failed.is_empty() {
        return;
    }
    let counts = if truth == test.positive {
        &test.acquired
    } else {
        &test.failed
    };
    if counts.is_empty() {
        env.alive = false;
    } else {
        set_counts(env, &test.id, counts.clone());
    }
}

pub(in crate::flow) fn enter_guard(
    receiver: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> Option<bool> {
    if !modeled_mode(env) {
        return None;
    }
    let reference = expr_name(expression).and_then(|name| {
        let (id, primitive) = protocol(env)?.primitive(name)?;
        matches!(primitive.kind, PrimitiveKind::Guard(_)).then(|| id.to_owned())
    });
    if let Some(id) = reference {
        bind_existing(env, receiver, id);
    } else if protocol_constructor(expression, env)
        .is_some_and(|(kind, ..)| matches!(kind, ConstructorKind::Guard(_)))
        || called_method(expression).is_some_and(|(receiver, method, args, kwargs)| {
            method == "get_lock"
                && args.is_empty()
                && kwargs.is_empty()
                && expr_name(receiver)
                    .and_then(|name| protocol(env)?.primitive(name))
                    .is_some_and(|(_, primitive)| {
                        matches!(primitive.kind, PrimitiveKind::Shared { .. })
                    })
        })
    {
        if !initialize(receiver, expression, env, file, &FunctionMap::new(), result) {
            queue_raise(env, range, None);
            env.alive = false;
            return Some(false);
        }
    } else {
        return None;
    }
    blocking_acquire(
        &AstExpr {
            kind: AstExprKind::Name(receiver.into()),
            range,
        },
        range,
        env,
        file,
        result,
    )
}

pub(in crate::flow) fn leave_guard(
    receiver: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    release(
        &AstExpr {
            kind: AstExprKind::Name(receiver.into()),
            range,
        },
        range,
        env,
        file,
        result,
    );
}

pub(in crate::flow) fn compound_store(
    target: &AstExpr,
    value: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let (receiver, index) = match &target.kind {
        AstExprKind::Attribute(receiver, name) if name == "value" => (receiver, None),
        AstExprKind::Subscript(receiver, values) if values.len() == 1 => {
            (receiver, Some(&values[0]))
        }
        _ => return false,
    };
    let Some((_, primitive)) = expr_name(receiver).and_then(|name| protocol(env)?.primitive(name))
    else {
        return false;
    };
    let PrimitiveKind::Shared { lock, array_len } = &primitive.kind else {
        return false;
    };
    let symbol = if array_len.is_some() {
        "multiprocessing.Array"
    } else {
        "multiprocessing.Value"
    };
    if !integer(file, value) {
        model_use(
            symbol,
            SYNC_MODEL,
            "guarded-compound-update",
            Err("compound update requires a bounded literal integral operand"),
            file,
            range,
            result,
        );
        return false;
    }
    let supported_operator = file
        .source
        .get(target.range.end..value.range.start)
        .is_some_and(|operator| matches!(operator.trim(), "+=" | "-=" | "*=" | "&=" | "|=" | "^="));
    let known_index = match (array_len, index) {
        (None, None) => true,
        (Some(len), Some(index)) => nonnegative(file, index).is_some_and(|index| index < *len),
        _ => false,
    };
    let guard = lock
        .as_ref()
        .and_then(|id| protocol(env)?.primitives.get(id));
    if primitive.unknown
        || !known_index
        || !supported_operator
        || guard.is_none_or(|guard| guard.unknown)
    {
        model_use(
            symbol,
            SYNC_MODEL,
            "guarded-compound-update",
            Err("the shared value, literal index or exact recursive guard is unresolved"),
            file,
            range,
            result,
        );
        return true;
    }
    let guard = guard.unwrap();
    let PrimitiveKind::Guard(held) = &guard.kind else {
        unreachable!()
    };
    let unprotected = held.counts.iter().filter(|count| count.held == 0).count();
    let identity = super::super::protocol_guard_identity(file, &guard.origin);
    let ambiguous = protocol(env).unwrap().primitives.values().any(|other| {
        matches!(other.kind, PrimitiveKind::Guard(_))
            && other.origin != guard.origin
            && super::super::protocol_guard_identity(file, &other.origin) == identity
    });
    if ambiguous {
        boundary(
            result,
            file,
            target.range,
            "normalized allocation anchors do not uniquely identify this process guard for review",
        );
    } else {
        crate::review::operations::record_protocol(
            "process-guarded-compound-update",
            span(file, target.range),
            vec![symbol.into(), identity],
        );
    }
    if unprotected != 0 {
        issue(
            (SYNC_CODE, SYNC_MODEL),
            "compound shared update lacks the exact wrapper lock for the read and write",
            &guard.origin,
            file,
            range,
            if unprotected == held.counts.len() {
                "definite"
            } else {
                "possible"
            },
            result,
        );
    }
    model_use(
        symbol,
        SYNC_MODEL,
        "guarded-compound-update",
        Ok(()),
        file,
        range,
        result,
    );
    true
}

pub(super) fn call(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> calls::ModelCall {
    use calls::ModelCall::{Applied, Unrecognized};
    let Some((receiver, method, args, kwargs)) = called_method(expr) else {
        return Unrecognized;
    };
    if let Some((_, _, guard)) = guard_lookup(env, receiver) {
        return match method {
            "acquire" => match nonblocking(args, kwargs) {
                Some(true) => {
                    try_acquire(receiver, expr.range, env, file, result);
                    Applied
                }
                Some(false) => {
                    blocking_acquire(receiver, expr.range, env, file, result);
                    Applied
                }
                None => model_use(
                    guard.kind.symbol(),
                    SYNC_MODEL,
                    method,
                    Err("acquire requires supported literal blocking arguments"),
                    file,
                    expr.range,
                    result,
                ),
            },
            "release" if args.is_empty() && kwargs.is_empty() => {
                release(receiver, expr.range, env, file, result);
                Applied
            }
            _ => model_use(
                guard.kind.symbol(),
                SYNC_MODEL,
                method,
                Err("operation is outside the multiprocessing guard profile"),
                file,
                expr.range,
                result,
            ),
        };
    }
    ipc_call(
        expr,
        (receiver, method, args, kwargs),
        env,
        file,
        functions,
        result,
    )
}

pub(in crate::flow) fn report_protocol_scope(
    state: &State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(protocol) = protocol(state) else {
        return;
    };
    for primitive in protocol.primitives.values() {
        if primitive.unknown {
            continue;
        }
        match &primitive.kind {
            PrimitiveKind::Guard(guard) => {
                let held = guard.counts.iter().filter(|count| count.held > 0).count();
                if held > 0 {
                    issue(
                        (SYNC_CODE, SYNC_MODEL),
                        "multiprocessing guard remains acquired at scope exit",
                        &primitive.origin,
                        file,
                        range,
                        if held == guard.counts.len() {
                            "definite"
                        } else {
                            "possible"
                        },
                        result,
                    );
                }
            }
            PrimitiveKind::Endpoint(endpoint) => {
                if !endpoint.closed && primitive.owner == protocol.actor {
                    issue(
                        (IPC_CODE, endpoint.kind.model()),
                        "IPC endpoint remains locally open at scope exit",
                        &primitive.origin,
                        file,
                        range,
                        "definite",
                        result,
                    );
                }
                if endpoint.feeder && !endpoint.joined && !endpoint.bypassed {
                    issue(
                        (IPC_CODE, endpoint.kind.model()),
                        "local queue feeder cleanup remains unestablished at scope exit",
                        &primitive.origin,
                        file,
                        range,
                        "definite",
                        result,
                    );
                }
            }
            PrimitiveKind::Shared { .. } => {}
        }
    }
    for channel in protocol
        .channels
        .values()
        .filter(|channel| !channel.unknown)
    {
        if let Some(item) = channel.received.get(&protocol.actor) {
            issue(
                (IPC_CODE, QUEUE_MODEL),
                "received JoinableQueue item remains unacknowledged at scope exit",
                &item.origin,
                file,
                range,
                "definite",
                result,
            );
        }
        if protocol.actor.is_empty() {
            if let Some(item) = channel.dropped.first() {
                issue(
                    (IPC_CODE, QUEUE_MODEL),
                    "feeder-rejected JoinableQueue item leaves accounting outstanding",
                    &item.origin,
                    file,
                    range,
                    "definite",
                    result,
                );
            }
        }
    }
}

fn allocation_origin(file: &SourceFile, range: SourceRange) -> Span {
    let start = file.source[..range.start]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    span(
        file,
        SourceRange {
            start,
            end: range.end,
        },
    )
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum SpawnArgument {
    Plain,
    Primitive(String),
    ForkGuard,
}

pub(super) fn spawn_argument(
    expr: &AstExpr,
    env: &State,
    file: &SourceFile,
) -> Option<SpawnArgument> {
    if plain_message(expr, env, file) {
        return Some(SpawnArgument::Plain);
    }
    let name = expr_name(expr)?;
    let state = protocol(env)?;
    if state.foreign_fork_guards.contains(name) {
        return Some(SpawnArgument::ForkGuard);
    }
    let (id, primitive) = state.primitive(name)?;
    if primitive.unknown
        || matches!(&primitive.kind, PrimitiveKind::Endpoint(endpoint) if endpoint.closed)
    {
        return None;
    }
    Some(SpawnArgument::Primitive(id.to_owned()))
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct WorkerBindings {
    aliases: BTreeMap<String, String>,
    plain: BTreeSet<String>,
    channels: BTreeSet<String>,
}

impl WorkerBindings {
    pub(super) fn touches(&self, channel: &str) -> bool {
        self.channels.contains(channel)
    }
}

pub(super) fn prepare_spawn(
    env: &mut State,
    actor: &str,
    arguments: &BTreeMap<String, SpawnArgument>,
) -> Option<WorkerBindings> {
    let protocol = protocol(env)?;
    let mut bindings = WorkerBindings::default();
    let mut copies = BTreeMap::new();
    for (name, arg) in arguments {
        let id = match arg {
            SpawnArgument::Primitive(id) => id,
            SpawnArgument::Plain => {
                bindings.plain.insert(name.clone());
                continue;
            }
            SpawnArgument::ForkGuard => return None,
        };
        let primitive = protocol.primitives.get(id)?;
        if primitive.unknown {
            return None;
        }
        let binding = match &primitive.kind {
            PrimitiveKind::Endpoint(endpoint) if !endpoint.closed => {
                let child_id = format!("{actor}::{id}");
                if protocol.primitives.contains_key(&child_id) {
                    return None;
                }
                bindings.channels.insert(endpoint.channel.clone());
                let mut endpoint = endpoint.clone();
                endpoint.feeder = false;
                endpoint.joined = false;
                endpoint.bypassed = false;
                let mut copy = primitive.clone();
                copy.owner = actor.to_owned();
                copy.kind = PrimitiveKind::Endpoint(endpoint);
                copies.insert(child_id.clone(), copy);
                child_id
            }
            PrimitiveKind::Endpoint(_) => return None,
            PrimitiveKind::Guard(_) | PrimitiveKind::Shared { .. } => id.clone(),
        };
        bindings.aliases.insert(name.clone(), binding);
    }
    if protocol.primitives.len() + copies.len() > MAX_PRIMITIVES {
        return None;
    }
    protocol_mut(env).primitives.extend(copies);
    Some(bindings)
}

pub(super) fn child_protocol(
    parent: &State,
    actor: &str,
    bindings: &WorkerBindings,
) -> LocalProtocol {
    let source = protocol(parent).expect("resolved worker protocol");
    let mut child = LocalProtocol {
        actor: actor.into(),
        aliases: bindings.aliases.clone(),
        plain_values: bindings.plain.clone(),
        primitives: source.primitives.clone(),
        channels: source.channels.clone(),
        ..LocalProtocol::default()
    };
    for primitive in child.primitives.values_mut() {
        if let PrimitiveKind::Guard(guard) = &mut primitive.kind {
            guard.counts = guard
                .counts
                .iter()
                .map(|value| Counter { held: 0, ..*value })
                .collect();
        }
    }
    child
}

fn project_worker_guards_and_close_copies(parent: &mut State, child: &LocalProtocol, actor: &str) {
    let state = protocol_mut(parent);
    for (id, primitive) in &mut state.primitives {
        let Some(applied) = child.primitives.get(id) else {
            continue;
        };
        match (&mut primitive.kind, &applied.kind) {
            (PrimitiveKind::Endpoint(endpoint), PrimitiveKind::Endpoint(_))
                if primitive.owner == actor =>
            {
                // The child completed; its operating-system endpoint copies are
                // gone. Worker-side explicit close was checked at each exit.
                endpoint.closed = true;
                primitive.unknown = false;
            }
            (PrimitiveKind::Guard(before), PrimitiveKind::Guard(after)) => {
                let counts = before
                    .counts
                    .iter()
                    .flat_map(|previous| {
                        after.counts.iter().map(move |observed| Counter {
                            held: previous.held,
                            available: observed.available,
                        })
                    })
                    .collect();
                before.counts = counts;
                primitive.unknown |=
                    applied.unknown || after.counts.iter().any(|count| count.held != 0);
            }
            _ => {}
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct WorkerEffects {
    before: BTreeMap<String, Channel>,
    after: LocalProtocol,
}

pub(super) fn worker_effects(parent: &State, after: LocalProtocol) -> WorkerEffects {
    WorkerEffects {
        before: protocol(parent).unwrap().channels.clone(),
        after,
    }
}

pub(super) fn spawn_channels(
    env: &State,
    args: &BTreeMap<String, SpawnArgument>,
) -> BTreeSet<String> {
    let state = protocol(env).unwrap();
    args.values()
        .filter_map(|arg| {
            let SpawnArgument::Primitive(id) = arg else {
                return None;
            };
            let PrimitiveKind::Endpoint(endpoint) = &state.primitives.get(id)?.kind else {
                return None;
            };
            Some(endpoint.channel.clone())
        })
        .collect()
}

pub(super) fn project_channel(parent: &mut State, effects: &WorkerEffects, id: &str) {
    let state = protocol_mut(parent);
    let Some(current) = state.channels.get_mut(id) else {
        return;
    };
    if let (Some(before), Some(after)) = (effects.before.get(id), effects.after.channels.get(id)) {
        if before == current {
            *current = after.clone();
        } else if before != after {
            current.unknown = true;
        }
    }
}

pub(super) fn complete_worker(
    parent: &mut State,
    effects: &WorkerEffects,
    actor: &str,
    projected: &BTreeSet<String>,
) {
    let channels = effects
        .before
        .keys()
        .filter(|id| !projected.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for channel in channels {
        project_channel(parent, effects, &channel);
    }
    project_worker_guards_and_close_copies(parent, &effects.after, actor);
}

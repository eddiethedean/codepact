//! Bounded asyncio queue, guard and signal protocols.

use super::*;
use crate::adoption;
use std::collections::VecDeque;

const QUEUE_MODEL: &str = "TPM-QUEUE-001";
const SYNC_MODEL: &str = "TPM-SYNC-001";
const QUEUE_RULE: &str = "TP7001";
const SYNC_RULE: &str = "TP7002";
const MAX_HANDLES: usize = 8;
const MAX_MESSAGES: usize = 16;
const MAX_ACTORS: usize = 4;

type Actor = Option<String>;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Protocols {
    handles: BTreeMap<String, Handle>,
    actor: Actor,
    ordered_workers: BTreeSet<String>,
    suspended: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Handle {
    origin: Span,
    review_identity: String,
    kind: Kind,
    uncertain: bool,
    state: Protocol,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Queue,
    Lock,
    Semaphore,
    BoundedSemaphore,
    Condition,
    Event,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Queue => "Queue",
            Self::Lock => "Lock",
            Self::Semaphore => "Semaphore",
            Self::BoundedSemaphore => "BoundedSemaphore",
            Self::Condition => "Condition",
            Self::Event => "Event",
        }
    }

    fn model(self) -> &'static str {
        match self {
            Self::Queue => QUEUE_MODEL,
            _ => SYNC_MODEL,
        }
    }

    fn acquisition(self) -> bool {
        matches!(
            self,
            Self::Lock | Self::Semaphore | Self::BoundedSemaphore | Self::Condition
        )
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Protocol {
    Queue(Queue),
    Permits(Permits),
    Condition(Condition),
    Event(Event),
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Queue {
    capacity: usize,
    counter: usize,
    shutdown: bool,
    zero_notifications: usize,
    buffered: VecDeque<Message>,
    received: Vec<Receipt>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Event {
    set: bool,
    signals: usize,
}

#[derive(Clone, PartialEq, Eq)]
struct Condition {
    lock: String,
    notifications: usize,
}

#[derive(Clone, PartialEq, Eq)]
struct Message {
    identity: String,
    admission: Span,
}

#[derive(Clone, PartialEq, Eq)]
struct Receipt {
    message: Message,
    site: Span,
    actor: Actor,
}

#[derive(Clone, PartialEq, Eq)]
struct Permit {
    site: Span,
    actor: Actor,
}

#[derive(Clone, PartialEq, Eq)]
struct Permits {
    available: usize,
    maximum: Option<usize>,
    acquired: Vec<Permit>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Duty {
    code: &'static str,
    origin: Span,
    contract: Vec<String>,
    related: Vec<Span>,
}

impl Duty {
    fn record(&self, closed: bool) {
        crate::review::operations::record(|| crate::review::operations::CleanupSite {
            kind: self.code,
            origin: self.origin.clone(),
            contract: self.contract.clone(),
            completion: "protocol-duty".into(),
            closed,
        });
    }
}

impl Handle {
    fn contract(&self) -> Vec<String> {
        vec![
            format!(
                "{}@{}",
                self.kind.model(),
                crate::foundation::CATALOG_REVISION
            ),
            self.review_identity.clone(),
        ]
    }

    fn duties(&self, actor: &Actor) -> BTreeSet<Duty> {
        match &self.state {
            Protocol::Queue(queue) => queue
                .received
                .iter()
                .filter(|receipt| &receipt.actor == actor)
                .map(|receipt| Duty {
                    code: QUEUE_RULE,
                    origin: receipt.site.clone(),
                    contract: self.contract(),
                    related: vec![
                        self.origin.clone(),
                        receipt.message.admission.clone(),
                        receipt.site.clone(),
                    ],
                })
                .collect(),
            Protocol::Permits(permits) => permits
                .acquired
                .iter()
                .filter(|permit| &permit.actor == actor)
                .map(|permit| Duty {
                    code: SYNC_RULE,
                    origin: permit.site.clone(),
                    contract: self.contract(),
                    related: vec![self.origin.clone(), permit.site.clone()],
                })
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    fn join(&mut self, other: &Self) {
        if self == other {
            return;
        }
        self.uncertain = true;
        match (&mut self.state, &other.state) {
            (Protocol::Queue(left), Protocol::Queue(right)) => {
                for receipt in &right.received {
                    if left.received.len() < MAX_MESSAGES && !left.received.contains(receipt) {
                        left.received.push(receipt.clone());
                    }
                }
            }
            (Protocol::Permits(left), Protocol::Permits(right)) => {
                for permit in &right.acquired {
                    if left.acquired.len() < MAX_MESSAGES && !left.acquired.contains(permit) {
                        left.acquired.push(permit.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

impl Protocols {
    pub(super) fn join(&mut self, other: &Self) {
        self.suspended |= other.suspended;
        for (id, handle) in &mut self.handles {
            if let Some(right) = other.handles.get(id) {
                handle.join(right);
            } else {
                handle.uncertain = true;
            }
        }
        for (id, handle) in &other.handles {
            if !self.handles.contains_key(id) && self.handles.len() < MAX_HANDLES {
                let mut handle = handle.clone();
                handle.uncertain = true;
                self.handles.insert(id.clone(), handle);
            }
        }
    }

    pub(super) fn copy_from(&mut self, other: &Self, objects: &BTreeSet<String>) {
        for id in objects {
            if let Some(handle) = other.handles.get(id) {
                self.handles.insert(id.clone(), handle.clone());
                if let Protocol::Condition(condition) = &handle.state {
                    if let Some(lock_handle) = other.handles.get(&condition.lock) {
                        self.handles
                            .insert(condition.lock.clone(), lock_handle.clone());
                    }
                }
            }
        }
    }

    pub(super) fn apply(&mut self, other: &Self, extra_objects: &BTreeSet<String>) {
        let objects = self.handles.keys().chain(extra_objects).cloned().collect();
        self.copy_from(other, &objects);
        if self.actor == other.actor {
            self.suspended |= other.suspended;
        }
    }

    pub(super) fn inherit_actor(&mut self, other: &Self) {
        self.actor = other.actor.clone();
        self.ordered_workers.clone_from(&other.ordered_workers);
        self.suspended = other.suspended;
    }

    pub(super) fn begin_actor(&mut self, task: &str) {
        self.actor = Some(task.to_owned());
        self.suspended = false;
    }

    pub(super) fn set_ordered_workers(&mut self, workers: BTreeSet<String>) {
        self.ordered_workers = workers;
    }

    pub(super) fn has_handle(&self, id: &str) -> bool {
        self.handles.contains_key(id)
    }

    pub(super) fn suspended(&self) -> bool {
        self.suspended
    }

    fn live_messages(&self) -> usize {
        self.handles
            .values()
            .map(|handle| match &handle.state {
                Protocol::Queue(queue) => queue.buffered.len() + queue.received.len(),
                _ => 0,
            })
            .sum()
    }

    pub(super) fn mark_uncertain(&mut self, id: &str) {
        let lock = self.handles.get_mut(id).and_then(|handle| {
            handle.uncertain = true;
            match &handle.state {
                Protocol::Condition(condition) => Some(condition.lock.clone()),
                _ => None,
            }
        });
        if let Some(lock) = lock.and_then(|lock| self.handles.get_mut(&lock)) {
            lock.uncertain = true;
        }
    }

    pub(super) fn apply_worker(&mut self, id: &str, other: &Self, possible: bool) {
        let objects = BTreeSet::from([id.to_owned()]);
        if possible {
            let mut outcomes = Self::default();
            outcomes.copy_from(other, &objects);
            for (id, handle) in &outcomes.handles {
                if let Some(existing) = self.handles.get_mut(id) {
                    existing.join(handle);
                }
            }
        } else {
            self.copy_from(other, &objects);
        }
    }
}

fn protocols(env: &State) -> Option<&Protocols> {
    env.ownership
        .as_ref()
        .map(|ownership| &ownership.async_protocols)
}

fn protocols_mut(env: &mut State) -> &mut Protocols {
    &mut env
        .ownership
        .as_mut()
        .expect("reviewed asyncio scope")
        .async_protocols
}

fn constructor(expression: &AstExpr, env: &State) -> Option<Kind> {
    if env.models.foundations.is_none() || env.ownership.is_none() {
        return None;
    }
    let (callee, _, _) = call_parts(expression)?;
    match ownership::external_identity(callee, env)?.as_str() {
        "external:asyncio.Queue" => Some(Kind::Queue),
        "external:asyncio.Lock" => Some(Kind::Lock),
        "external:asyncio.Semaphore" => Some(Kind::Semaphore),
        "external:asyncio.BoundedSemaphore" => Some(Kind::BoundedSemaphore),
        "external:asyncio.Condition" => Some(Kind::Condition),
        "external:asyncio.Event" => Some(Kind::Event),
        _ => None,
    }
}

pub(super) fn is_constructor(expression: &AstExpr, env: &State) -> bool {
    constructor(expression, env).is_some()
}

pub(super) fn is_method(expression: &AstExpr, env: &State) -> bool {
    called_method(expression).is_some_and(|(receiver, _, _, _)| {
        expr_name(receiver).is_some_and(|name| recognize(name, env).is_some())
    })
}

fn applicable(env: &State) -> Result<(), &'static str> {
    let default_factory = env.ownership.as_ref().is_some_and(|ownership| {
        ownership.mode.assumes_default_task_factory() && !ownership.incompatible_task_factory
    });
    runtime::runtime_profile(env)?.asyncio(&env.target_python_version, default_factory)
}

fn recognize<'a>(name: &str, env: &'a State) -> Option<(&'a str, &'a Handle)> {
    let domain = protocols(env)?;
    env.places
        .get(name)?
        .iter()
        .find_map(|id| domain.handles.get(id).map(|handle| (id.as_str(), handle)))
}

fn exact(name: &str, id: &str, env: &State) -> bool {
    env.places
        .get(name)
        .is_some_and(|targets| targets.len() == 1)
        && !env.unknown_places.contains(name)
        && !env.possibly_unbound_places.contains(name)
        && !env.escaped.contains(id)
}

pub(super) enum HeldLock {
    Held(Span),
    Unheld(Span),
    Unknown,
}

pub(super) fn held_lock(env: &State, name: &str) -> HeldLock {
    let Some(id) = lock_binding(env, name) else {
        return HeldLock::Unknown;
    };
    held_lock_by_id(env, id)
}

pub(super) fn lock_binding<'a>(env: &'a State, name: &str) -> Option<&'a str> {
    let (id, handle) = recognize(name, env)?;
    (handle.kind == Kind::Lock && exact(name, id, env)).then_some(id)
}

pub(super) fn held_lock_by_id(env: &State, id: &str) -> HeldLock {
    let Some(domain) = protocols(env) else {
        return HeldLock::Unknown;
    };
    let Some(handle) = domain.handles.get(id) else {
        return HeldLock::Unknown;
    };
    if handle.kind != Kind::Lock
        || handle.uncertain
        || env.escaped.contains(id)
        || has_peer(env, id)
    {
        return HeldLock::Unknown;
    }
    let actor = &domain.actor;
    match &handle.state {
        Protocol::Permits(permits)
            if permits.available == 0
                && permits.acquired.iter().any(|permit| &permit.actor == actor) =>
        {
            HeldLock::Held(handle.origin.clone())
        }
        Protocol::Permits(_) => HeldLock::Unheld(handle.origin.clone()),
        _ => HeldLock::Unknown,
    }
}

fn one_argument<'a>(
    arguments: &'a [AstExpr],
    keywords: &'a [CallKeyword],
    name: &str,
) -> Result<Option<&'a AstExpr>, &'static str> {
    match (arguments, keywords) {
        ([], []) => Ok(None),
        ([argument], []) => Ok(Some(argument)),
        ([], [(Some(keyword), argument)]) if keyword == name => Ok(Some(argument)),
        _ => Err("unsupported asyncio protocol arguments"),
    }
}

fn integer(expression: &AstExpr, file: &SourceFile) -> Option<usize> {
    if !matches!(expression.kind, AstExprKind::Primitive) {
        return None;
    }
    let text = file
        .source
        .get(expression.range.start..expression.range.end)?
        .replace('_', "");
    let mut chars = text.chars();
    let prefix = (chars.next(), chars.next());
    let (digits, radix) = match prefix {
        (Some('0'), Some('x' | 'X')) => (chars.as_str(), 16),
        (Some('0'), Some('o' | 'O')) => (chars.as_str(), 8),
        (Some('0'), Some('b' | 'B')) => (chars.as_str(), 2),
        _ => (text.as_str(), 10),
    };
    usize::from_str_radix(digits, radix).ok()
}

fn literal_size(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    name: &str,
    default: usize,
    file: &SourceFile,
) -> Result<usize, &'static str> {
    one_argument(arguments, keywords, name)?.map_or(Ok(default), |value| {
        integer(value, file).ok_or("asyncio size requires an exact nonnegative integer literal")
    })
}

fn allocation_identity(file: &SourceFile, range: SourceRange, env: &State) -> String {
    format!(
        ":async-protocol:{}:{}:{}",
        file.path,
        range.start,
        env.allocation_context.join("/")
    )
}

fn handle_identity(file: &SourceFile, site: &Span) -> String {
    let mut anchor = site.clone();
    anchor.column = 1;
    anchor.end_line = anchor.line;
    anchor.end_column = file
        .source
        .lines()
        .nth(anchor.line.saturating_sub(1))
        .map_or(site.end_column, |line| line.len() + 1);
    adoption::review_site_anchor(
        &file.source,
        &anchor,
        &mut adoption::ReviewAnchorCache::default(),
    )
}

fn initialize(
    kind: Kind,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let (_, args, kwargs) = call_parts(expression).expect("constructor call");
    let id = allocation_identity(file, expression.range, env);
    let origin = span(file, expression.range);
    let identity = handle_identity(file, &origin);
    let mut implicit_lock = None;
    let initial = (|| match kind {
        Kind::Queue => Ok(Protocol::Queue(Queue {
            capacity: literal_size(args, kwargs, "maxsize", 0, file)?,
            ..Queue::default()
        })),
        Kind::Semaphore | Kind::BoundedSemaphore => {
            let value = literal_size(args, kwargs, "value", 1, file)?;
            if value == 0 {
                return Err(
                    "the asyncio semaphore profile requires a positive initial permit count",
                );
            }
            Ok(Protocol::Permits(Permits {
                available: value,
                maximum: (kind == Kind::BoundedSemaphore).then_some(value),
                acquired: Vec::new(),
            }))
        }
        Kind::Condition => match one_argument(args, kwargs, "lock")? {
            None
            | Some(AstExpr {
                kind: AstExprKind::NoneLiteral,
                ..
            }) => {
                let lock = format!("{id}:lock");
                implicit_lock = Some(lock.clone());
                Ok(Protocol::Condition(Condition {
                    lock,
                    notifications: 0,
                }))
            }
            Some(value) => {
                let name = expr_name(value).ok_or("condition lock identity is unresolved")?;
                let (id, handle) =
                    recognize(name, env).ok_or("condition lock identity is unresolved")?;
                if handle.kind != Kind::Lock || !exact(name, id, env) {
                    return Err("condition lock identity is unresolved");
                }
                Ok(Protocol::Condition(Condition {
                    lock: id.to_owned(),
                    notifications: 0,
                }))
            }
        },
        Kind::Lock | Kind::Event if args.is_empty() && kwargs.is_empty() => {
            Ok(if kind == Kind::Lock {
                new_lock()
            } else {
                Protocol::Event(Event::default())
            })
        }
        _ => Err("unsupported asyncio protocol constructor arguments"),
    })();
    let domain = protocols(env).expect("protocol constructor");
    let available = applicable(env).and_then(|()| {
        if domain.handles.contains_key(&id) {
            return Err("asyncio protocol has multiple allocations at one analyzed site");
        }
        if domain.handles.len() + 1 + usize::from(implicit_lock.is_some()) > MAX_HANDLES {
            return Err("asyncio protocol primitive budget exceeded");
        }
        initial.as_ref().map(|_| ()).map_err(|reason| *reason)
    });
    let symbol = format!("asyncio.{}", kind.name());
    if !runtime::use_model(
        &symbol,
        "construct",
        kind.model(),
        available,
        file,
        expression.range,
        result,
    ) {
        protocols_mut(env).mark_uncertain(&id);
        queue_raise(env, expression.range, None);
        return;
    }
    let domain = protocols_mut(env);
    if let Some(lock) = implicit_lock {
        domain.handles.insert(
            lock,
            Handle {
                origin: origin.clone(),
                review_identity: identity.clone(),
                kind: Kind::Lock,
                uncertain: false,
                state: new_lock(),
            },
        );
    }
    domain.handles.insert(
        id.clone(),
        Handle {
            origin,
            review_identity: identity,
            kind,
            uncertain: false,
            state: initial.expect("applicable constructor"),
        },
    );
    env.pending_return_targets = Some(BTreeSet::from([id]));
}

fn new_lock() -> Protocol {
    Protocol::Permits(Permits {
        available: 1,
        maximum: Some(1),
        acquired: Vec::new(),
    })
}

fn issue(
    result: &mut Inspection,
    code: &'static str,
    text: &str,
    location: Span,
    related: Vec<Span>,
    possible: bool,
) {
    report(
        result,
        code,
        text,
        location,
        related,
        if possible { "possible" } else { "definite" },
        Vec::new(),
    );
}

fn failure(env: &mut State, range: SourceRange, exception: &str) {
    queue_raise(
        env,
        range,
        Some(AstExpr {
            range,
            kind: AstExprKind::Name(format!(":exception:{exception}")),
        }),
    );
    env.alive = false;
}

fn cancel_if_blocked(env: &mut State, range: SourceRange) {
    tasks::allow_tasks_to_start(env);
    queue_blocked_cancellation(env, range);
}

fn queue_blocked_cancellation(env: &mut State, range: SourceRange) {
    let mut interrupted = env.clone();
    interrupted.alive = true;
    interrupted.pending_raises.clear();
    asyncio::queue_deadline_cancellations(env, range, &interrupted, false);
    env.pending_raises.push(FlowExit {
        state: interrupted,
        range,
        kind: CompletionKind::Cancelled(None),
    });
}

fn has_peer(env: &State, id: &str) -> bool {
    let Some(ownership) = &env.ownership else {
        return false;
    };
    let guard = effective_lock(id, env);
    ownership
        .groups
        .values()
        .flat_map(|group| &group.tasks)
        .any(|(task_id, scheduled)| {
            !scheduled.cancelled_before_start
                && (ownership.async_protocols.actor.is_some() || scheduled.may_have_started)
                && ownership.async_protocols.actor.as_ref() != Some(task_id)
                && !ownership
                    .async_protocols
                    .actor
                    .as_ref()
                    .is_some_and(|actor| {
                        !ownership.async_protocols.suspended
                            && ownership.async_protocols.ordered_workers.contains(actor)
                            && ownership.async_protocols.ordered_workers.contains(task_id)
                    })
                && env.coroutines.get(task_id).is_some_and(|record| {
                    !record.consumed
                        && record.captures.values().any(|capture| {
                            capture.targets.contains(id)
                                || guard.as_ref().is_some_and(|guard| {
                                    capture.targets.iter().any(|target| {
                                        target == guard
                                            || effective_lock(target, env).as_ref() == Some(guard)
                                    })
                                })
                        })
                })
        })
}

fn suspend(
    id: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    reason: &str,
) {
    protocols_mut(env).suspended = true;
    cancel_if_blocked(env, range);
    boundary(result, file, range, reason);
    // The pending path before suspension retained its exact duties. Any later
    // progress depends on an unmodeled peer operation.
    protocols_mut(env).mark_uncertain(id);
}

fn recognized_operation(kind: Kind, method: &str) -> bool {
    match kind {
        Kind::Queue => matches!(
            method,
            "put"
                | "put_nowait"
                | "get"
                | "get_nowait"
                | "task_done"
                | "join"
                | "shutdown"
                | "qsize"
                | "empty"
                | "full"
        ),
        Kind::Lock | Kind::Semaphore | Kind::BoundedSemaphore => {
            matches!(method, "acquire" | "release" | "locked")
        }
        Kind::Condition => matches!(
            method,
            "acquire" | "release" | "locked" | "wait" | "notify" | "notify_all" | "wait_for"
        ),
        Kind::Event => matches!(method, "set" | "clear" | "is_set" | "wait"),
    }
}

fn asynchronous_operation(method: &str) -> bool {
    matches!(
        method,
        "put" | "get" | "join" | "acquire" | "wait" | "wait_for"
    )
}

fn valid_value(expression: &AstExpr, env: &State) -> bool {
    match &expression.kind {
        AstExprKind::Name(name) => {
            !env.places.contains_key(name)
                && !env.unknown_places.contains(name)
                && !env.unknown_coroutine_aliases.contains(name)
                && !env.coroutine_aliases.contains_key(name)
                && env.definitely_bound_names.contains(name)
        }
        AstExprKind::Primitive
        | AstExprKind::String(_)
        | AstExprKind::Boolean(_)
        | AstExprKind::NoneLiteral => true,
        AstExprKind::Tuple(values) => values.iter().all(|value| valid_value(value, env)),
        _ => false,
    }
}

fn queue_arguments(
    method: &str,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
    env: &State,
) -> Result<bool, &'static str> {
    match method {
        "put" | "put_nowait" => {
            if one_argument(args, kwargs, "item")?.is_some_and(|value| valid_value(value, env)) {
                Ok(false)
            } else {
                Err("queue admission requires one bounded plain payload")
            }
        }
        "shutdown" => match one_argument(args, kwargs, "immediate")? {
            None => Ok(false),
            Some(AstExpr {
                kind: AstExprKind::Boolean(value),
                ..
            }) => Ok(*value),
            _ => Err("queue shutdown mode must be a known boolean"),
        },
        _ if args.is_empty() && kwargs.is_empty() => Ok(false),
        _ => Err("unsupported asyncio queue arguments"),
    }
}

fn queue_effect(
    id: &str,
    method: &str,
    immediate_shutdown: bool,
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let location = span(file, expr.range);
    let domain = protocols(env).expect("queue domain");
    let actor = domain.actor.clone();
    let message_limit =
        matches!(method, "put" | "put_nowait") && domain.live_messages() >= MAX_MESSAGES;
    let identity = allocation_identity(file, expr.range, env);
    let handle = protocols_mut(env)
        .handles
        .get_mut(id)
        .expect("queue receiver");
    let contract = handle.contract();
    let Protocol::Queue(queue) = &mut handle.state else {
        unreachable!("queue operation requires the queue model");
    };
    match method {
        "put" | "put_nowait" if queue.shutdown => failure(env, expr.range, "asyncio.QueueShutDown"),
        "put_nowait" if queue.capacity != 0 && queue.buffered.len() >= queue.capacity => {
            failure(env, expr.range, "asyncio.QueueFull");
        }
        "put" if queue.capacity != 0 && queue.buffered.len() >= queue.capacity => {
            suspend(
                id,
                env,
                file,
                expr.range,
                result,
                "a full asyncio queue requires an unresolved peer receive",
            );
        }
        "put" | "put_nowait" => {
            let known = queue
                .buffered
                .iter()
                .chain(queue.received.iter().map(|r| &r.message));
            if message_limit {
                handle.uncertain = true;
                boundary(
                    result,
                    file,
                    expr.range,
                    "asyncio queue live message budget exceeded",
                );
            } else if known
                .into_iter()
                .any(|message| message.identity == identity)
            {
                handle.uncertain = true;
                boundary(
                    result,
                    file,
                    expr.range,
                    "repeated queue admission identity is unresolved",
                );
            } else {
                queue.counter += 1;
                queue.buffered.push_back(Message {
                    identity,
                    admission: location,
                });
            }
        }
        "get" | "get_nowait" if queue.buffered.is_empty() && queue.shutdown => {
            failure(env, expr.range, "asyncio.QueueShutDown");
        }
        "get_nowait" if queue.buffered.is_empty() => failure(env, expr.range, "asyncio.QueueEmpty"),
        "get" if queue.buffered.is_empty() => {
            suspend(
                id,
                env,
                file,
                expr.range,
                result,
                "an empty asyncio queue requires an unresolved peer admission",
            );
        }
        "get" | "get_nowait" => {
            let message = queue.buffered.pop_front().expect("ready receiver");
            if queue.received.iter().any(|receipt| receipt.actor == actor) {
                boundary(
                    result,
                    file,
                    expr.range,
                    "queue acknowledgment correspondence is ambiguous after overlapping receives",
                );
            }
            queue.received.push(Receipt {
                message,
                site: location,
                actor,
            });
        }
        "task_done" => {
            let mut matches = queue
                .received
                .iter()
                .enumerate()
                .filter_map(|(index, receipt)| (receipt.actor == actor).then_some(index));
            let first = matches.next();
            let ambiguous = matches.next().is_some();
            if queue.counter == 0 || first.is_none() {
                issue(
                    result,
                    QUEUE_RULE,
                    "queue task_done() lacks a pending receive for this consumer",
                    location,
                    vec![handle.origin.clone()],
                    false,
                );
            } else if ambiguous {
                boundary(
                    result,
                    file,
                    expr.range,
                    "queue acknowledgment correspondence is ambiguous",
                );
                handle.uncertain = true;
            } else if let Some(index) = first {
                let receipt = queue.received.remove(index);
                Duty {
                    code: QUEUE_RULE,
                    origin: receipt.site,
                    contract,
                    related: Vec::new(),
                }
                .record(true);
            }
            if queue.counter == 0 {
                failure(env, expr.range, "ValueError");
            } else {
                queue.counter -= 1;
                if queue.counter == 0 {
                    queue.zero_notifications = queue.zero_notifications.saturating_add(1);
                }
            }
        }
        "join" if queue.counter != 0 => {
            suspend(id, env, file, expr.range, result, "queue join is waiting on unfinished accounting without an established acknowledgment");
        }
        "join" => {
            runtime::use_model(
                "asyncio.Queue.join",
                "counter-zero",
                QUEUE_MODEL,
                Ok(()),
                file,
                expr.range,
                result,
            );
        }
        "shutdown" => {
            queue.shutdown = true;
            if immediate_shutdown {
                let unfinished = queue.counter != 0;
                queue.counter = queue.counter.saturating_sub(queue.buffered.len());
                if unfinished && queue.counter == 0 {
                    queue.zero_notifications = queue.zero_notifications.saturating_add(1);
                }
                for message in queue.buffered.drain(..) {
                    crate::foundation::record_model_use(
                        "asyncio.Queue.shutdown",
                        "abandon-buffered-item",
                        &format!("{QUEUE_MODEL}@{}", crate::foundation::CATALOG_REVISION),
                        message.admission,
                        Ok(()),
                    );
                }
            }
        }
        _ => {}
    }
}

fn effective_lock(id: &str, env: &State) -> Option<String> {
    let domain = protocols(env)?;
    let receiver = domain.handles.get(id)?;
    match &receiver.state {
        Protocol::Condition(condition) => Some(condition.lock.clone()),
        Protocol::Permits(_) => Some(id.to_owned()),
        _ => None,
    }
}

fn sync_effect(
    id: &str,
    method: &str,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let site = span(file, range);
    let actor = protocols(env)
        .expect("synchronization domain")
        .actor
        .clone();
    let origin = protocols(env).expect("synchronization domain").handles[id]
        .origin
        .clone();
    if let Some(lock) = effective_lock(id, env) {
        let unstable = env.escaped.contains(&lock);
        let lock_handle = protocols_mut(env)
            .handles
            .get_mut(&lock)
            .expect("guard identity");
        if unstable || lock_handle.uncertain {
            boundary(
                result,
                file,
                range,
                "asyncio acquisition or release depends on an unresolved guard state",
            );
            queue_raise(env, range, None);
            return;
        }
        let kind = lock_handle.kind;
        let contract = lock_handle.contract();
        let Protocol::Permits(permits) = &mut lock_handle.state else {
            unreachable!("effective guard carries permits");
        };
        match method {
            "acquire" if permits.available == 0 => {
                suspend(
                    &lock,
                    env,
                    file,
                    range,
                    result,
                    "asyncio acquire waits on an unresolved peer release",
                );
            }
            "acquire" if permits.acquired.len() >= MAX_MESSAGES => {
                lock_handle.uncertain = true;
                boundary(
                    result,
                    file,
                    range,
                    "asyncio guard acquisition budget exceeded",
                );
            }
            "acquire" => {
                permits.available -= 1;
                permits.acquired.push(Permit { site, actor });
            }
            "release" => {
                let paired = permits
                    .acquired
                    .iter()
                    .rposition(|permit| permit.actor == actor);
                if paired.is_none() || permits.maximum == Some(permits.available) {
                    issue(
                        result,
                        SYNC_RULE,
                        "asyncio release lacks a matching acquisition in this actor",
                        site,
                        vec![origin],
                        false,
                    );
                }
                if permits.maximum == Some(permits.available) {
                    failure(
                        env,
                        range,
                        if kind == Kind::Lock {
                            "RuntimeError"
                        } else {
                            "ValueError"
                        },
                    );
                    return;
                }
                if let Some(index) = paired {
                    let permit = permits.acquired.remove(index);
                    Duty {
                        code: SYNC_RULE,
                        origin: permit.site,
                        contract,
                        related: Vec::new(),
                    }
                    .record(true);
                }
                if let Some(available) = permits.available.checked_add(1) {
                    permits.available = available;
                } else {
                    lock_handle.uncertain = true;
                    boundary(
                        result,
                        file,
                        range,
                        "asyncio permit count exceeds the analysis representation",
                    );
                }
            }
            "wait" | "notify" | "notify_all" => {
                if permits.available != 0
                    || !permits.acquired.iter().any(|permit| permit.actor == actor)
                {
                    issue(
                        result,
                        SYNC_RULE,
                        "condition operation requires its exact held lock",
                        site,
                        vec![origin],
                        false,
                    );
                }
                if permits.available != 0 {
                    failure(env, range, "RuntimeError");
                } else if method == "wait" {
                    // CPython reacquires the exact lock before returning or
                    // propagating cancellation. The wakeup supplies no predicate.
                    protocols_mut(env).suspended = true;
                    cancel_if_blocked(env, range);
                    boundary(
                        result,
                        file,
                        range,
                        "condition wait depends on an unresolved peer signal",
                    );
                } else if let Protocol::Condition(condition) = &mut protocols_mut(env)
                    .handles
                    .get_mut(id)
                    .expect("condition receiver")
                    .state
                {
                    condition.notifications = condition.notifications.saturating_add(1);
                }
            }
            _ => {}
        }
    } else {
        let Protocol::Event(event) =
            &mut protocols_mut(env).handles.get_mut(id).expect("event").state
        else {
            unreachable!("synchronization operation requires an event or guard");
        };
        match method {
            "set" if !event.set => {
                event.set = true;
                event.signals = event.signals.saturating_add(1);
            }
            "clear" => event.set = false,
            "wait" if !event.set => suspend(
                id,
                env,
                file,
                range,
                result,
                "event wait depends on an unresolved peer signal",
            ),
            _ => {}
        }
    }
}

pub(super) fn call(
    expr: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if let Some(kind) = constructor(expr, env) {
        initialize(kind, expr, env, file, result);
        return true;
    }
    let Some((receiver, method, args, kwargs)) = called_method(expr) else {
        return false;
    };
    let Some(name) = expr_name(receiver) else {
        return false;
    };
    let Some((id, handle)) = recognize(name, env) else {
        return false;
    };
    let id = id.to_owned();
    let kind = handle.kind;
    let awaited = env
        .awaited_call
        .as_ref()
        .is_some_and(|(path, range)| path == &file.path && *range == expr.range);
    let async_operation = asynchronous_operation(method);
    let arguments = if kind == Kind::Queue {
        queue_arguments(method, args, kwargs, env)
    } else if args.is_empty() && kwargs.is_empty() && method != "wait_for" {
        Ok(false)
    } else {
        Err("unsupported asyncio synchronization arguments or callback predicate")
    };
    let applicable = applicable(env).and_then(|()| {
        if !recognized_operation(kind, method) {
            Err("operation on the asyncio primitive falls outside the reviewed protocol")
        } else if !exact(name, &id, env) || handle.uncertain {
            Err("asyncio protocol receiver or current state is unresolved")
        } else if async_operation && !awaited {
            Err("asyncio protocol coroutine requires a direct await")
        } else if awaited && !async_operation {
            Err("asyncio protocol operation is synchronous")
        } else if has_peer(env, &id) {
            Err("asyncio protocol effect depends on an unresolved shared-task ordering")
        } else {
            arguments.map(|_| ())
        }
    });
    let symbol = format!("asyncio.{}.{}", kind.name(), method);
    if !runtime::use_model(
        &symbol,
        method,
        kind.model(),
        applicable,
        file,
        expr.range,
        result,
    ) {
        if awaited || !async_operation || !recognized_operation(kind, method) {
            protocols_mut(env).mark_uncertain(&id);
            queue_raise(env, expr.range, None);
            if awaited && async_operation {
                protocols_mut(env).suspended = true;
                cancel_if_blocked(env, expr.range);
            }
        }
        return true;
    }
    let contract = handle.contract();
    if kind == Kind::Queue {
        let family = match method {
            "put" | "put_nowait" => Some("asyncio-queue-admission"),
            "get" | "get_nowait" => Some("asyncio-queue-receipt"),
            "join" => Some("asyncio-queue-join"),
            _ => None,
        };
        if let Some(family) = family {
            crate::review::operations::record_protocol(family, span(file, expr.range), contract);
        }
        queue_effect(
            &id,
            method,
            arguments.expect("reviewed queue arguments"),
            expr,
            env,
            file,
            result,
        );
    } else {
        sync_effect(&id, method, expr.range, env, file, result);
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn await_call(
    awaited: &AstExpr,
    range: SourceRange,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> bool {
    let Some((receiver, method, args, kwargs)) = called_method(awaited) else {
        return false;
    };
    let Some((name, (id, handle))) =
        expr_name(receiver).and_then(|name| recognize(name, env).map(|resolved| (name, resolved)))
    else {
        return false;
    };
    if let Some(wait) = wait_for_peer(name, id, handle, method, args, kwargs, env, functions) {
        // The selected peer runs without suspension. A cancelled waiter can
        // resume before the peer starts or after its established effects.
        queue_blocked_cancellation(env, range);
        wait.release_guard(awaited.range, env, file, result);
        asyncio::deliver_task(
            &wait.task,
            asyncio::Delivery::WaitOnly,
            range,
            env,
            file,
            contracts,
            functions,
            result,
        );
        if !env.alive {
            return true;
        }
        wait.reacquire_guard(awaited.range, env, file, result);
        cancel_if_blocked(env, range);
        if wait.observe(awaited.range, env, file, result) {
            return true;
        }
    }
    let previous = env.awaited_call.replace((file.path.clone(), awaited.range));
    let pending = env.pending_raises.len();
    consume_expr(awaited, env, file, contracts, functions, result, depth + 1);
    env.awaited_call = previous.clone();
    for exit in env.pending_raises.iter_mut().skip(pending) {
        exit.state.awaited_call = previous.clone();
    }
    true
}

enum Wake {
    QueueJoin(usize),
    EventSignal(usize),
    ConditionSignal {
        notifications: usize,
        lock: String,
        permit: Permit,
    },
    RetryOperation,
}

struct PeerWait {
    id: String,
    task: String,
    wake: Wake,
}

impl PeerWait {
    fn release_guard(
        &self,
        range: SourceRange,
        env: &mut State,
        file: &SourceFile,
        result: &mut Inspection,
    ) {
        let Wake::ConditionSignal { lock, .. } = &self.wake else {
            return;
        };
        let Protocol::Permits(permits) = &mut protocols_mut(env)
            .handles
            .get_mut(lock)
            .expect("condition lock")
            .state
        else {
            unreachable!("condition lock has a permit");
        };
        permits.acquired.clear();
        permits.available = 1;
        runtime::use_model(
            "asyncio.Condition.wait",
            "release-for-wait",
            SYNC_MODEL,
            Ok(()),
            file,
            range,
            result,
        );
    }

    fn reacquire_guard(
        &self,
        range: SourceRange,
        env: &mut State,
        file: &SourceFile,
        result: &mut Inspection,
    ) {
        let Wake::ConditionSignal { lock, permit, .. } = &self.wake else {
            return;
        };
        let escaped = env.escaped.contains(lock);
        let domain = protocols_mut(env);
        let handle = domain.handles.get_mut(lock).expect("condition lock");
        let Protocol::Permits(permits) = &mut handle.state else {
            unreachable!("condition lock has a permit");
        };
        if escaped || handle.uncertain || permits.available != 1 || !permits.acquired.is_empty() {
            domain.mark_uncertain(&self.id);
            boundary(
                result,
                file,
                range,
                "condition wait cannot establish guard reacquisition",
            );
            return;
        }
        permits.available = 0;
        permits.acquired.push(permit.clone());
        runtime::use_model(
            "asyncio.Condition.wait",
            "reacquire-after-wait",
            SYNC_MODEL,
            Ok(()),
            file,
            range,
            result,
        );
    }

    fn observe(
        &self,
        range: SourceRange,
        env: &State,
        file: &SourceFile,
        result: &mut Inspection,
    ) -> bool {
        let Some(handle) = protocols(env)
            .and_then(|p| p.handles.get(&self.id))
            .filter(|handle| !handle.uncertain && !env.escaped.contains(&self.id))
        else {
            return false;
        };
        let (symbol, effect, model) = match (&self.wake, &handle.state) {
            (Wake::QueueJoin(generation), Protocol::Queue(queue))
                if queue.zero_notifications > *generation =>
            {
                crate::review::operations::record_protocol(
                    "asyncio-queue-join",
                    span(file, range),
                    handle.contract(),
                );
                ("asyncio.Queue.join", "counter-zero-notified", QUEUE_MODEL)
            }
            (Wake::EventSignal(generation), Protocol::Event(event))
                if event.signals > *generation =>
            {
                ("asyncio.Event.wait", "signal-observed", SYNC_MODEL)
            }
            (Wake::ConditionSignal { notifications, .. }, Protocol::Condition(condition))
                if condition.notifications > *notifications =>
            {
                ("asyncio.Condition.wait", "signal-observed", SYNC_MODEL)
            }
            _ => return false,
        };
        runtime::use_model(symbol, effect, model, Ok(()), file, range, result)
    }
}

#[allow(clippy::too_many_arguments)]
fn wait_for_peer(
    name: &str,
    id: &str,
    handle: &Handle,
    method: &str,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
    env: &State,
    functions: &FunctionMap,
) -> Option<PeerWait> {
    let ownership = env.ownership.as_ref()?;
    if applicable(env).is_err()
        || ownership.async_protocols.actor.is_some()
        || !exact(name, id, env)
        || handle.uncertain
        || has_peer(env, id)
    {
        return None;
    }
    let wake = match (&handle.state, method) {
        (Protocol::Queue(queue), "join")
            if queue.counter != 0 && args.is_empty() && kwargs.is_empty() =>
        {
            Wake::QueueJoin(queue.zero_notifications)
        }
        (Protocol::Queue(queue), "get")
            if !queue.shutdown
                && queue.buffered.is_empty()
                && args.is_empty()
                && kwargs.is_empty() =>
        {
            Wake::RetryOperation
        }
        (Protocol::Queue(queue), "put")
            if !queue.shutdown
                && queue.capacity != 0
                && queue.buffered.len() >= queue.capacity
                && queue_arguments(method, args, kwargs, env).is_ok() =>
        {
            Wake::RetryOperation
        }
        (Protocol::Event(event), "wait") if !event.set && args.is_empty() && kwargs.is_empty() => {
            Wake::EventSignal(event.signals)
        }
        (Protocol::Condition(condition), "wait") if args.is_empty() && kwargs.is_empty() => {
            let lock = ownership.async_protocols.handles.get(&condition.lock)?;
            let Protocol::Permits(permits) = &lock.state else {
                return None;
            };
            let [permit] = permits.acquired.as_slice() else {
                return None;
            };
            if lock.uncertain
                || env.escaped.contains(&condition.lock)
                || permits.available != 0
                || permit.actor != ownership.async_protocols.actor
            {
                return None;
            }
            Wake::ConditionSignal {
                notifications: condition.notifications,
                lock: condition.lock.clone(),
                permit: permit.clone(),
            }
        }
        _ => return None,
    };
    let mut workers = ownership.groups.values().flat_map(|group| {
        group.tasks.iter().filter_map(move |(task_id, task)| {
            env.coroutines
                .get(task_id)
                .filter(|record| !record.consumed)
                .map(|record| (group, task_id, task, record))
        })
    });
    let (group, task_id, task, record) = workers.next()?;
    if workers.next().is_some()
        || group.incomplete
        || task.possible
        || task.cancel_requested
        || task.may_have_started
        || record.ambiguous
        || task.cancelled_before_start
        || !record
            .captures
            .values()
            .any(|capture| capture.targets.contains(id))
    {
        return None;
    }
    let exact_worker = if matches!(wake, Wake::ConditionSignal { .. }) {
        condition_notifier(record, env, functions, id)
    } else {
        straight_line_worker(record, env, functions, false)
    };
    if !exact_worker {
        return None;
    }
    let (alias, _) = ownership
        .task_names
        .iter()
        .find(|(_, (_, candidate))| candidate == task_id)?;
    Some(PeerWait {
        id: id.to_owned(),
        task: alias.clone(),
        wake,
    })
}

pub(super) fn test(expression: &AstExpr, env: &State) -> Option<bool> {
    let (receiver, method, args, kwargs) = called_method(expression)?;
    if !args.is_empty() || !kwargs.is_empty() || applicable(env).is_err() {
        return None;
    }
    let name = expr_name(receiver)?;
    let (id, handle) = recognize(name, env)?;
    if handle.uncertain || !exact(name, id, env) || has_peer(env, id) {
        return None;
    }
    match (&handle.state, method) {
        (Protocol::Queue(queue), "empty") => Some(queue.buffered.is_empty()),
        (Protocol::Queue(queue), "full") => {
            Some(queue.capacity != 0 && queue.buffered.len() >= queue.capacity)
        }
        (Protocol::Permits(permits), "locked") => Some(permits.available == 0),
        (Protocol::Event(event), "is_set") => Some(event.set),
        _ => None,
    }
}

pub(super) fn enter(
    expression: &AstExpr,
    asynchronous: bool,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<String> {
    let (id, named_exact) = if let Some(name) = expr_name(expression) {
        let (id, _) = recognize(name, env)?;
        (id.to_owned(), exact(name, id, env))
    } else {
        let id = env.pending_return_targets.as_ref()?.first()?.clone();
        (id, true)
    };
    let handle = protocols(env)?.handles.get(&id)?;
    let kind = handle.kind;
    let symbol = format!("asyncio.{}", kind.name());
    let applicability = applicable(env).and_then(|()| {
        if !asynchronous || !kind.acquisition() {
            Err("asyncio context requires async with on a reviewed guard")
        } else if !named_exact || handle.uncertain || has_peer(env, &id) {
            Err("asyncio context guard state or shared-task ordering is unresolved")
        } else {
            Ok(())
        }
    });
    if !runtime::use_model(
        &symbol,
        "context-entry",
        kind.model(),
        applicability,
        file,
        expression.range,
        result,
    ) {
        queue_raise(env, expression.range, None);
        return Some(String::new());
    }
    sync_effect(&id, "acquire", expression.range, env, file, result);
    Some(id)
}

pub(super) fn leave(
    id: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    if id.is_empty() {
        boundary(
            result,
            file,
            range,
            "asyncio context cleanup lacks an established guard entry",
        );
        return;
    }
    let Some(handle) = protocols(env).and_then(|domain| domain.handles.get(id)) else {
        boundary(
            result,
            file,
            range,
            "asyncio context cleanup guard is unresolved",
        );
        return;
    };
    let symbol = format!("asyncio.{}", handle.kind.name());
    runtime::use_model(
        &symbol,
        "context-exit",
        handle.kind.model(),
        Ok(()),
        file,
        range,
        result,
    );
    sync_effect(id, "release", range, env, file, result);
}

fn report_duties(
    states: &[&State],
    actor: &Actor,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mut counts = BTreeMap::<Duty, (usize, bool)>::new();
    for (state, domain) in states
        .iter()
        .filter_map(|state| protocols(state).map(|domain| (*state, domain)))
    {
        let mut path = BTreeMap::new();
        for (id, handle) in &domain.handles {
            let uncertain = handle.uncertain || state.escaped.contains(id);
            for duty in handle.duties(actor) {
                path.entry(duty)
                    .and_modify(|possible| *possible |= uncertain)
                    .or_insert(uncertain);
            }
        }
        for (duty, uncertain) in path {
            let (count, possible) = counts.entry(duty).or_default();
            *count += 1;
            *possible |= uncertain;
        }
    }
    for (duty, (count, possible)) in counts {
        let message = if duty.code == QUEUE_RULE {
            "asyncio queue receipt leaves an outstanding accounting duty"
        } else {
            "asyncio guard acquisition leaves an outstanding release duty"
        };
        duty.record(false);
        let mut related = duty.related;
        related.push(span(file, range));
        report(
            result,
            duty.code,
            message,
            duty.origin,
            related,
            if possible || count < states.len() {
                "possible"
            } else {
                "definite"
            },
            duty.contract,
        );
    }
}

pub(super) fn report_scope(
    env: &State,
    exits: &FlowResult,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let states: Vec<_> = env
        .alive
        .then_some(env)
        .into_iter()
        .chain(exits.all_abrupt().map(|exit| &exit.state))
        .collect();
    report_duties(&states, &None, file, range, result);
}

pub(super) fn report_worker(
    states: &[&State],
    actor: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    report_duties(states, &Some(actor.to_owned()), file, range, result);
}

pub(super) fn supported_actor_count(records: usize) -> bool {
    records < MAX_ACTORS
}

fn worker_arguments<'a>(
    record: &'a CoroutineRecord,
    env: &State,
    functions: &'a FunctionMap,
) -> Option<(&'a FunctionBody, BTreeMap<String, &'a AstExpr>)> {
    let (callee, args, kwargs) = call_parts(&record.invocation)?;
    let definition = expr_name(callee)
        .and_then(|name| env.function_bindings.get(name))
        .and_then(|binding| functions.get(binding))
        .filter(|definition| definition.is_async)?;
    let bindings = bind_arguments(&definition.signature, args, kwargs, false).ok()?;
    Some((definition, bindings))
}

fn condition_notifier(
    record: &CoroutineRecord,
    env: &State,
    functions: &FunctionMap,
    id: &str,
) -> bool {
    let Some((definition, bindings)) = worker_arguments(record, env, functions) else {
        return false;
    };
    let [FlowStatement::With {
        items,
        body,
        asynchronous: true,
        ..
    }] = definition.statements.as_ref()
    else {
        return false;
    };
    let ([item], [FlowStatement::Expression { value, .. }]) = (items.as_slice(), body.as_slice())
    else {
        return false;
    };
    let Some(parameter) = expr_name(&item.context) else {
        return false;
    };
    let Some(capture) = bindings
        .get(parameter)
        .and_then(|argument| record.captures.get(expr_name(argument)?))
    else {
        return false;
    };
    !item.unsupported_target
        && item.target.is_none()
        && !capture.unknown
        && capture.targets.len() == 1
        && capture.targets.contains(id)
        && called_method(value).is_some_and(|(receiver, method, args, kwargs)| {
            expr_name(receiver) == Some(parameter)
                && matches!(method, "notify" | "notify_all")
                && args.is_empty()
                && kwargs.is_empty()
        })
}

fn straight_line_operation(
    expression: &AstExpr,
    arguments: &BTreeSet<String>,
    allow_await: bool,
) -> bool {
    let (expression, awaited) = match &expression.kind {
        AstExprKind::Await(inner) => (inner.as_ref(), true),
        _ => (expression, false),
    };
    if awaited && !allow_await {
        return false;
    }
    let Some((receiver, method, values, keywords)) = called_method(expression) else {
        return !awaited
            && match &expression.kind {
                AstExprKind::Name(name) => !arguments.contains(name),
                AstExprKind::Primitive
                | AstExprKind::String(_)
                | AstExprKind::Boolean(_)
                | AstExprKind::NoneLiteral => true,
                _ => false,
            };
    };
    expr_name(receiver).is_some_and(|name| arguments.contains(name))
        && keywords.iter().all(|(name, _)| name.is_some())
        && (if awaited {
            matches!(method, "put" | "get" | "join" | "wait" | "acquire")
        } else {
            matches!(
                method,
                "put_nowait"
                    | "get_nowait"
                    | "task_done"
                    | "release"
                    | "set"
                    | "clear"
                    | "shutdown"
            )
        })
        && values
            .iter()
            .chain(keywords.iter().map(|(_, value)| value))
            .all(|value| {
                matches!(
                    value.kind,
                    AstExprKind::Primitive
                        | AstExprKind::String(_)
                        | AstExprKind::Boolean(_)
                        | AstExprKind::NoneLiteral
                        | AstExprKind::Name(_)
                )
            })
}

pub(super) fn straight_line_worker(
    record: &CoroutineRecord,
    env: &State,
    functions: &FunctionMap,
    allow_await: bool,
) -> bool {
    let Some((definition, bindings)) = worker_arguments(record, env, functions) else {
        return false;
    };
    let tracked = bindings
        .into_iter()
        .filter_map(|(parameter, argument)| {
            let capture = record.captures.get(expr_name(argument)?)?;
            (capture.targets.len() == 1
                && !capture.unknown
                && capture
                    .targets
                    .iter()
                    .all(|id| protocols(env).is_some_and(|p| p.has_handle(id))))
            .then_some(parameter)
        })
        .collect::<BTreeSet<_>>();
    definition
        .statements
        .iter()
        .all(|statement| match statement {
            FlowStatement::Assign { name, value, .. } if !tracked.contains(name) => {
                straight_line_operation(value, &tracked, allow_await)
            }
            FlowStatement::Expression { value, .. } => {
                straight_line_operation(value, &tracked, allow_await)
            }
            FlowStatement::Return { value, .. } => value
                .as_ref()
                .is_none_or(|value| straight_line_operation(value, &tracked, allow_await)),
            FlowStatement::Pass { .. } => true,
            _ => false,
        })
}

//! Effects of exact installed HTTP and SQL profiles. General receiver dispatch
//! and ownership are handled by the surrounding flow evaluator.
use super::*;
use crate::foundation::{self, ProviderOrigin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resource {
    Client,
    AsyncClient,
    Body,
    Stream,
    Engine,
    AsyncEngine,
    Connection,
    Session,
    AsyncSession,
    Result(ResultFamily),
    Transaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultFamily {
    Rows,
    Scalars,
    Unknown,
}

const FILTERED_RESULT: &str = "result-filtered";

impl Resource {
    fn name(self) -> &'static str {
        match self {
            Self::Client => "Client",
            Self::AsyncClient => "AsyncClient",
            Self::Body | Self::Stream => "Response",
            Self::Engine => "Engine",
            Self::AsyncEngine => "AsyncEngine",
            Self::Connection => "Connection",
            Self::Session => "Session",
            Self::AsyncSession => "AsyncSession",
            Self::Result(ResultFamily::Scalars) => "ScalarResult",
            Self::Result(_) => "Result",
            Self::Transaction => "SessionTransaction",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Stream => "stream",
            Self::Result(ResultFamily::Unknown) => "unknown-result",
            _ => self.name(),
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "Client" => Self::Client,
            "AsyncClient" => Self::AsyncClient,
            "body" => Self::Body,
            "stream" => Self::Stream,
            "Engine" => Self::Engine,
            "AsyncEngine" => Self::AsyncEngine,
            "Connection" => Self::Connection,
            "Session" => Self::Session,
            "AsyncSession" => Self::AsyncSession,
            "Result" => Self::Result(ResultFamily::Rows),
            "ScalarResult" => Self::Result(ResultFamily::Scalars),
            "unknown-result" => Self::Result(ResultFamily::Unknown),
            "SessionTransaction" => Self::Transaction,
            _ => return None,
        })
    }

    fn cleanup(self) -> &'static str {
        match self {
            Self::AsyncClient => "aclose",
            Self::Engine | Self::AsyncEngine => "dispose",
            Self::Transaction => "rollback",
            _ => "close",
        }
    }

    fn asynchronous(self) -> bool {
        matches!(
            self,
            Self::AsyncClient | Self::AsyncEngine | Self::AsyncSession
        )
    }
}

const ORIGIN_PREFIX: &str = "reviewed-library:";

fn library_resource(contract: &Contract) -> Option<(&str, Resource)> {
    let entry = contract
        .provenance
        .iter()
        .find_map(|entry| entry.origin.strip_prefix(ORIGIN_PREFIX))?;
    let (scope, tag) = entry.split_once(':')?;
    Some((scope, Resource::from_tag(tag)?))
}

pub(super) fn buffered_value(contract: &Contract) -> bool {
    matches!(
        library_resource(contract),
        Some((_, Resource::Body | Resource::Result(_)))
    )
}

fn object_for<'a>(value: &AstExpr, env: &'a State) -> Option<(&'a str, &'a str, Resource)> {
    let name = expr_name(value)?;
    let targets = env.places.get(name)?;
    let object = targets.first().filter(|_| targets.len() == 1)?;
    let (scope, resource) = library_resource(env.resources.get(object)?)?;
    Some((object, scope, resource))
}

fn ready(scope: &str, asynchronous: bool, env: &State) -> Result<(), &'static str> {
    let runtime = runtime::runtime_profile(env)?;
    runtime.applicability(&env.target_python_version)?;
    if asynchronous {
        let default_factory = env.ownership.as_ref().is_some_and(|ownership| {
            ownership.mode.assumes_default_task_factory() && !ownership.incompatible_task_factory
        });
        runtime.asyncio(&env.target_python_version, default_factory)?;
    }
    let inputs = env.models.foundations.as_ref().expect("applicable runtime");
    let provider_ready = |name: &str| {
        inputs.providers.get(name).is_some_and(|provider| {
            provider.origin == "reviewed-library" && provider.reason.is_none()
        })
    };
    if !provider_ready(scope) {
        return Err("the selected library artifact does not match the reviewed wheel");
    }
    for dependency in foundation::reviewed_library_dependencies(scope) {
        if (asynchronous || !matches!(*dependency, "aiosqlite" | "anyio"))
            && !provider_ready(dependency)
        {
            return Err("the selected library transport or database dependency is outside the reviewed profile");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn use_effect(
    scope: &str,
    symbol: &str,
    effect: &str,
    status: Result<(), &'static str>,
    file: &SourceFile,
    range: SourceRange,
    env: &State,
    result: &mut Inspection,
) -> bool {
    let origin = env
        .models
        .foundations
        .as_ref()
        .and_then(|inputs| inputs.providers.get(scope))
        .map(|provider| ProviderOrigin {
            public_symbol: symbol.into(),
            canonical_symbol: canonical_symbol(scope, symbol),
            distribution: provider.distribution.clone(),
            version: provider.version.clone(),
            artifact_sha256: provider.artifact_sha256.clone(),
            location: None,
            replaced_origin: None,
        });
    foundation::record_use(
        symbol,
        effect,
        "reviewed-library",
        span(file, range),
        status,
        origin,
    );
    if let Err(reason) = status {
        boundary(result, file, range, reason);
    }
    status.is_ok()
}

fn canonical_symbol(scope: &str, symbol: &str) -> String {
    let tail = symbol
        .strip_prefix(scope)
        .and_then(|tail| tail.strip_prefix('.'))
        .unwrap_or(symbol);
    if scope == "sqlmodel" && tail == "create_engine" {
        return "sqlalchemy.engine.create.create_engine".into();
    }
    if scope == "sqlalchemy" {
        match tail {
            "orm.Session" => return "sqlalchemy.orm.session.Session".into(),
            "engine.create_engine" => return "sqlalchemy.engine.create.create_engine".into(),
            "ext.asyncio.AsyncSession" => {
                return "sqlalchemy.ext.asyncio.session.AsyncSession".into()
            }
            "ext.asyncio.create_async_engine" => {
                return "sqlalchemy.ext.asyncio.engine.create_async_engine".into()
            }
            _ => {}
        }
    }
    let implementation = match (scope, tail.split('.').next().unwrap_or_default()) {
        ("httpx" | "httpx2", "Client" | "AsyncClient") => "_client",
        ("httpx" | "httpx2", "Response") => "_models",
        ("sqlalchemy", "Session" | "SessionTransaction") => "orm.session",
        ("sqlalchemy", "Engine" | "Connection") => "engine.base",
        ("sqlalchemy", "create_engine") => "engine.create",
        ("sqlalchemy", "Result" | "ScalarResult") => "engine.result",
        ("sqlalchemy", "AsyncSession") => "ext.asyncio.session",
        ("sqlalchemy", "AsyncEngine" | "create_async_engine") => "ext.asyncio.engine",
        ("sqlmodel", "Session") => "orm.session",
        ("sqlmodel", "AsyncSession") => "ext.asyncio.session",
        _ => return symbol.to_owned(),
    };
    format!("{scope}.{implementation}.{tail}")
}

fn allocate(
    scope: &str,
    kind: Resource,
    file: &SourceFile,
    range: SourceRange,
    env: &mut State,
    result: &mut Inspection,
) {
    let symbol = format!("{scope}.{}", kind.name());
    let contract = Contract {
        schema_version: crate::SCHEMA_VERSION,
        base_symbol: canonical_symbol(scope, &symbol),
        display_name: symbol,
        lifecycle: Some(crate::Lifecycle {
            closed_by: kind.cleanup().into(),
        }),
        access: "owned".into(),
        source: span(file, range),
        provenance: vec![crate::Provenance {
            origin: format!("{ORIGIN_PREFIX}{scope}:{}", kind.tag()),
            trust: "reviewed-model".into(),
        }],
    };
    runtime::allocate_modeled_resource(env, contract, file, range, result);
    if matches!(kind, Resource::Client | Resource::AsyncClient) {
        if let Some(object) = env
            .pending_return_targets
            .as_ref()
            .and_then(|targets| targets.first())
            .cloned()
        {
            set_transaction(&object, "client-new", env);
        }
    }
    if kind == Resource::Body {
        for object in env.pending_return_targets.iter().flatten() {
            env.duty_origins.remove(object);
        }
    }
    if matches!(kind, Resource::Result(_)) {
        for object in env.pending_return_targets.iter().flatten() {
            env.protocol_states
                .insert(object.clone(), BTreeSet::from(["result-open".into()]));
        }
    }
}

fn constructor(symbol: &str) -> Option<(&str, Resource)> {
    let (scope, member) = symbol.split_once('.')?;
    let kind = match (scope, member) {
        ("httpx" | "httpx2", "Client") => Resource::Client,
        ("httpx" | "httpx2", "AsyncClient") => Resource::AsyncClient,
        ("sqlalchemy" | "sqlmodel", "create_engine" | "engine.create_engine") => Resource::Engine,
        ("sqlalchemy", "ext.asyncio.create_async_engine") => Resource::AsyncEngine,
        ("sqlalchemy" | "sqlmodel", "Session" | "orm.Session") => Resource::Session,
        (
            "sqlalchemy" | "sqlmodel",
            "ext.asyncio.AsyncSession" | "ext.asyncio.session.AsyncSession",
        ) => Resource::AsyncSession,
        ("etlantic_sqlmodel", "control_plane.session.session_scope") => Resource::Session,
        _ => return None,
    };
    Some((scope, kind))
}

fn constructor_arguments(
    kind: Resource,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
    env: &State,
) -> bool {
    match kind {
        Resource::Client | Resource::AsyncClient => {
            args.is_empty()
                && kwargs.iter().any(|(name, value)| {
                    name.as_deref() == Some("trust_env")
                        && value.kind == AstExprKind::Boolean(false)
                })
                && kwargs.iter().all(|(name, value)| {
                    matches!(
                        name.as_deref(),
                        Some("trust_env" | "follow_redirects" | "http2")
                    ) && value.kind == AstExprKind::Boolean(false)
                })
        }
        Resource::Engine | Resource::AsyncEngine => {
            let prefix = if kind == Resource::Engine {
                "sqlite:///"
            } else {
                "sqlite+aiosqlite:///"
            };
            matches!(args, [AstExpr { kind: AstExprKind::String(url), .. }] if url.strip_prefix(prefix).is_some_and(|path| !path.is_empty() && path != ":memory:" && !path.contains(['?', '#'])))
                && kwargs.is_empty()
        }
        Resource::Session | Resource::AsyncSession => {
            let bind = match (args, kwargs) {
                ([bind], []) => Some(bind),
                ([], [(Some(name), bind)]) if name == "bind" => Some(bind),
                _ => None,
            };
            bind.and_then(|value| object_for(value, env))
                .is_some_and(|(_, _, bound)| {
                    if kind == Resource::Session {
                        matches!(bound, Resource::Engine | Resource::Connection)
                    } else {
                        bound == Resource::AsyncEngine
                    }
                })
        }
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Close,
    Request,
    ReadBody,
    Connect,
    Query(ResultFamily),
    Consume,
    First,
    Filter,
    Begin,
    Commit,
    Rollback,
}

fn operation(kind: Resource, method: &str) -> Option<Operation> {
    let op = match (kind, method) {
        (Resource::Client | Resource::AsyncClient, "get" | "request") => Operation::Request,
        (Resource::Stream | Resource::Body, "read") => Operation::ReadBody,
        (Resource::Engine, "connect") => Operation::Connect,
        (Resource::Connection | Resource::Session | Resource::AsyncSession, "execute") => {
            Operation::Query(ResultFamily::Rows)
        }
        (Resource::Connection | Resource::Session | Resource::AsyncSession, "scalars") => {
            Operation::Query(ResultFamily::Scalars)
        }
        (Resource::Session | Resource::AsyncSession, "exec") => {
            Operation::Query(ResultFamily::Unknown)
        }
        (Resource::Result(_), "all" | "fetchall") => Operation::Consume,
        (Resource::Result(_), "first") => Operation::First,
        (Resource::Result(ResultFamily::Rows), "scalars" | "mappings") => Operation::Filter,
        (Resource::Session | Resource::Connection, "begin") => Operation::Begin,
        (
            Resource::Connection
            | Resource::Session
            | Resource::AsyncSession
            | Resource::Transaction,
            "commit",
        ) => Operation::Commit,
        (
            Resource::Connection
            | Resource::Session
            | Resource::AsyncSession
            | Resource::Transaction,
            "rollback",
        ) => Operation::Rollback,
        _ if method == kind.cleanup() => Operation::Close,
        _ => return None,
    };
    Some(op)
}

fn sqlmodel_result(args: &[AstExpr], env: &State) -> ResultFamily {
    let [statement] = args else {
        return ResultFamily::Unknown;
    };
    let scalar = match statement.kind {
        AstExprKind::EvaluatedSqlModelSelect { scalar } => Some(scalar),
        _ => sqlmodel_select_is_scalar(statement, env),
    };
    match scalar {
        Some(true) => ResultFamily::Scalars,
        Some(false) => ResultFamily::Rows,
        None => ResultFamily::Unknown,
    }
}

pub(super) fn sqlmodel_select_is_scalar(statement: &AstExpr, env: &State) -> Option<bool> {
    env.models.foundations.as_ref()?;
    let (select, columns, keywords) = call_parts(statement)?;
    if columns.is_empty()
        || !keywords.is_empty()
        || columns
            .iter()
            .any(|column| matches!(column.kind, AstExprKind::Starred(_)))
        || providers::public_name(select, env).as_deref() != Some("sqlmodel.select")
    {
        return None;
    }
    Some(columns.len() == 1)
}

fn arguments_supported(
    op: Operation,
    method: &str,
    args: &[AstExpr],
    kwargs: &[CallKeyword],
) -> bool {
    let literal_url = |argument: &AstExpr| matches!(&argument.kind, AstExprKind::String(value) if value.starts_with("http://"));
    kwargs.is_empty()
        && match op {
            Operation::Request if method == "get" => matches!(args, [url] if literal_url(url)),
            Operation::Request => {
                matches!(args, [AstExpr { kind: AstExprKind::String(method), .. }, url] if matches!(method.as_str(), "GET" | "HEAD") && literal_url(url))
            }
            Operation::Query(_) => args.len() == 1,
            _ => args.is_empty(),
        }
}

fn set_closed(object: &str, env: &mut State, file: &SourceFile, range: SourceRange) {
    env.objects.insert(object.to_owned(), CLOSED);
    env.close_origins
        .insert(object.to_owned(), span(file, range));
}

fn set_transaction(object: &str, state: &str, env: &mut State) {
    env.protocol_states
        .insert(object.to_owned(), BTreeSet::from([state.to_owned()]));
}

fn set_result_state(object: &str, state: &str, env: &mut State) {
    let states = env.protocol_states.entry(object.to_owned()).or_default();
    states.retain(|state| state == FILTERED_RESULT);
    states.insert(state.to_owned());
}

fn queue_uncertain_exit(object: &str, range: SourceRange, env: &mut State) {
    queue_raise(env, range, None);
    if let Some(exit) = env.pending_raises.last_mut() {
        *exit.state.objects.entry(object.to_owned()).or_default() |= UNKNOWN;
        exit.state.protocol_unknown.insert(object.to_owned());
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_operation(
    op: Operation,
    scope: &str,
    kind: Resource,
    object: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
    let body_buffered = op == Operation::ReadBody
        && env
            .protocol_states
            .get(object)
            .is_some_and(|states| states.len() == 1 && states.contains("stream-buffered"));
    let result_exhausted = matches!(kind, Resource::Result(_))
        && env.protocol_states.get(object).is_some_and(|states| {
            states.contains("result-exhausted")
                && states
                    .iter()
                    .all(|state| state == "result-exhausted" || state == FILTERED_RESULT)
        });
    if op != Operation::Close
        && !body_buffered
        && !result_exhausted
        && mask & CLOSED != 0
        && !matches!(
            kind,
            Resource::Engine | Resource::Session | Resource::AsyncSession
        )
    {
        report(
            result,
            "TP1001",
            "reviewed library operation uses a closed handle",
            span(file, range),
            env.close_origins.get(object).cloned().into_iter().collect(),
            if mask == CLOSED {
                "definite"
            } else {
                "possible"
            },
            contract_identity(env.resources.get(object)),
        );
        queue_raise(env, range, None);
        if mask == CLOSED {
            env.alive = false;
            return;
        }
    }
    match op {
        Operation::Close => {
            ownership::check_close(env, object, file, range, result, mask != OPEN);
            if env.borrowed.contains(object) || env.forbidden_borrowed_closes.contains(object) {
                report(
                    result,
                    "TP2004",
                    "cannot close a borrowed library resource",
                    span(file, range),
                    Vec::new(),
                    "definite",
                    contract_identity(env.resources.get(object)),
                );
                return;
            }
            queue_uncertain_exit(object, range, env);
            set_closed(object, env, file, range);
            if matches!(kind, Resource::Result(_)) {
                set_result_state(object, "result-closed", env);
            }
            if matches!(
                kind,
                Resource::Connection | Resource::Session | Resource::AsyncSession
            ) {
                set_transaction(object, "reset", env);
            }
        }
        Operation::Request => {
            set_transaction(object, "client-started", env);
            queue_raise(env, range, None);
            allocate(scope, Resource::Body, file, range, env, result);
        }
        Operation::ReadBody => {
            if kind == Resource::Stream {
                queue_raise(env, range, None);
                set_transaction(object, "stream-buffered", env);
            }
        }
        Operation::Connect => {
            if mask & CLOSED != 0 {
                env.objects
                    .insert(object.to_owned(), OPEN | (mask & UNKNOWN));
                env.duty_origins
                    .insert(object.to_owned(), span(file, range));
            }
            queue_raise(env, range, None);
            allocate(scope, Resource::Connection, file, range, env, result);
        }
        Operation::Query(family) => {
            if matches!(kind, Resource::Session | Resource::AsyncSession) && mask & CLOSED != 0 {
                if env
                    .ownership
                    .as_ref()
                    .is_some_and(|ownership| ownership.entering_task)
                {
                    boundary(result, file, range, "a scheduled operation reopens a reset session and its cleanup ownership is unresolved");
                    *env.objects.entry(object.to_owned()).or_default() |= UNKNOWN;
                    queue_raise(env, range, None);
                    return;
                }
                env.objects
                    .insert(object.to_owned(), OPEN | (mask & UNKNOWN));
                if !env.borrowed.contains(object) {
                    env.duty_origins
                        .insert(object.to_owned(), span(file, range));
                }
            }
            set_transaction(object, "active", env);
            queue_raise(env, range, None);
            allocate(scope, Resource::Result(family), file, range, env, result);
        }
        Operation::Consume => {
            queue_uncertain_exit(object, range, env);
            set_closed(object, env, file, range);
            set_result_state(object, "result-exhausted", env);
        }
        Operation::First => {
            queue_uncertain_exit(object, range, env);
            set_closed(object, env, file, range);
            set_result_state(object, "result-closed", env);
        }
        Operation::Filter => {
            // Filtered results share the underlying cursor. The resource retains
            // one cleanup identity, so aliases support only operations common to
            // the raw result and every view that may have been created.
            env.protocol_states
                .entry(object.to_owned())
                .or_default()
                .insert(FILTERED_RESULT.into());
            env.pending_return_targets = Some(BTreeSet::from([object.to_owned()]));
        }
        Operation::Begin => {
            queue_raise(env, range, None);
            set_transaction(object, "active", env);
            allocate(scope, Resource::Transaction, file, range, env, result);
            if let Some(transaction) = env
                .pending_return_targets
                .as_ref()
                .and_then(|targets| targets.first())
                .cloned()
            {
                set_transaction(&transaction, &format!("subject:{object}"), env);
            }
        }
        Operation::Commit | Operation::Rollback => {
            queue_uncertain_exit(object, range, env);
            let outcome = if op == Operation::Commit {
                "committed"
            } else {
                "rolled-back"
            };
            let parent = env
                .protocol_states
                .get(object)
                .and_then(|states| states.first())
                .and_then(|state| state.strip_prefix("subject:"))
                .map(str::to_owned);
            set_transaction(object, outcome, env);
            if let Some(parent) = parent {
                set_transaction(&parent, outcome, env);
            }
            if kind == Resource::Transaction {
                set_closed(object, env, file, range);
            }
        }
    }
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let Some((callee, args, kwargs)) = call_parts(expression) else {
        return false;
    };
    if let Some(public) = providers::public_name(callee, env) {
        if let Some((scope, kind)) = constructor(&public) {
            let status = ready(scope, kind.asynchronous(), env).and_then(|()| {
                if scope == "etlantic_sqlmodel" { return Err("the reviewed ETLantic helper must appear in a synchronous with statement"); }
                constructor_arguments(kind, args, kwargs, env).then_some(()).ok_or("constructor arguments do not select the reviewed HTTP transport or SQLite configuration")
            });
            queue_raise(env, expression.range, None);
            if use_effect(
                scope,
                &public,
                "fresh-owned-return",
                status,
                file,
                expression.range,
                env,
                result,
            ) {
                allocate(scope, kind, file, expression.range, env, result);
            }
            return true;
        }
        if matches!(
            public.as_str(),
            "sqlalchemy.text" | "sqlalchemy.select" | "sqlmodel.select"
        ) {
            let scope = public.split('.').next().expect("resolved SQL helper");
            let status = ready(scope, false, env);
            use_effect(
                scope,
                &public,
                "statement-construction",
                status,
                file,
                expression.range,
                env,
                result,
            );
            queue_raise(env, expression.range, None);
            return true;
        }
    }
    let AstExprKind::Attribute(receiver, method) = &callee.kind else {
        return false;
    };
    let Some((object, scope, kind)) = object_for(receiver, env) else {
        return false;
    };
    let object = object.to_owned();
    let scope = scope.to_owned();
    let op = operation(kind, method).map(|op| match op {
        Operation::Query(ResultFamily::Unknown) => Operation::Query(sqlmodel_result(args, env)),
        _ => op,
    });
    let symbol = format!("{scope}.{}.{method}", kind.name());
    let status = ready(&scope, kind.asynchronous(), env).and_then(|()| {
        if expr_name(receiver).is_some_and(|name| {
            env.unknown_places.contains(name) || env.possibly_unbound_places.contains(name)
        }) {
            return Err("reviewed library receiver identity is ambiguous");
        }
        if method == "exec" && scope != "sqlmodel" {
            return Err("exec requires a selected SQLModel session");
        }
        if matches!(kind, Resource::Result(ResultFamily::Unknown))
            && matches!(method.as_str(), "scalars" | "mappings")
        {
            return Err("cannot establish which SQL result family the statement returns");
        }
        if op == Some(Operation::Filter)
            && env
                .protocol_states
                .get(&object)
                .is_some_and(|states| states.contains(FILTERED_RESULT))
        {
            return Err("operation depends on an unresolved raw or filtered SQL result alias");
        }
        op.filter(|op| arguments_supported(*op, method, args, kwargs))
            .map(|_| ())
            .ok_or("operation or arguments are outside the reviewed library profile")
    });
    let effect = match op {
        Some(Operation::Close) => "receiver-cleanup",
        Some(Operation::Request) => "buffered-response",
        Some(Operation::Query(_)) => "database-result",
        Some(Operation::Consume | Operation::First) => "consume-result",
        Some(Operation::Begin) => "transaction-begin",
        Some(Operation::Commit) => "transaction-commit",
        Some(Operation::Rollback) => "transaction-rollback",
        _ => "receiver-operation",
    };
    if !use_effect(
        &scope,
        &symbol,
        effect,
        status,
        file,
        expression.range,
        env,
        result,
    ) {
        weaken(env, std::iter::once(object));
        queue_raise(env, expression.range, None);
        return true;
    }
    if kind.asynchronous()
        && !env
            .awaited_call
            .as_ref()
            .is_some_and(|(path, range)| path == &file.path && *range == expression.range)
    {
        register_pending_coroutine(expression, env, file, result, None);
        return true;
    }
    let op = op.expect("qualified operation");
    if kind == Resource::AsyncSession
        && matches!(
            op,
            Operation::Query(_) | Operation::Commit | Operation::Rollback
        )
        && env
            .ownership
            .as_ref()
            .is_none_or(|ownership| !ownership.entering_task)
    {
        check_live_uses(
            None,
            &object,
            &scope,
            kind,
            env,
            file,
            expression.range,
            result,
        );
    }
    apply_operation(
        op,
        &scope,
        kind,
        &object,
        env,
        file,
        expression.range,
        result,
    );
    true
}

pub(super) fn direct_await(expression: &AstExpr, env: &State) -> bool {
    let Some((callee, _, _)) = call_parts(expression) else {
        return false;
    };
    let AstExprKind::Attribute(receiver, method) = &callee.kind else {
        return false;
    };
    object_for(receiver, env).is_some_and(|(_, scope, kind)| {
        kind.asynchronous() && operation(kind, method).is_some() && ready(scope, true, env).is_ok()
    })
}

#[derive(Clone, Copy)]
pub(super) enum Context {
    Standard,
    Transaction,
    Stream,
    Etl,
}

pub(super) fn context(expression: &AstExpr, asynchronous: bool, env: &State) -> Option<Context> {
    if let Some((_, _, resource)) = object_for(expression, env) {
        return (resource.asynchronous() == asynchronous && resource_has_context(resource))
            .then_some(Context::Standard);
    }
    let (callee, _, _) = call_parts(expression)?;
    if let Some((scope, resource)) = providers::public_name(callee, env)
        .as_deref()
        .and_then(constructor)
    {
        if scope == "etlantic_sqlmodel" && !asynchronous {
            return Some(Context::Etl);
        }
        return (resource.asynchronous() == asynchronous && resource_has_context(resource))
            .then_some(Context::Standard);
    }
    if asynchronous {
        return None;
    }
    let AstExprKind::Attribute(receiver, method) = &callee.kind else {
        return None;
    };
    let (_, _, resource) = object_for(receiver, env)?;
    match (resource, method.as_str()) {
        (Resource::Client, "stream") => Some(Context::Stream),
        (Resource::Session | Resource::Connection, "begin") => Some(Context::Transaction),
        (Resource::Engine, "connect") => Some(Context::Standard),
        _ => None,
    }
}

fn resource_has_context(resource: Resource) -> bool {
    matches!(
        resource,
        Resource::Client
            | Resource::AsyncClient
            | Resource::Connection
            | Resource::Session
            | Resource::AsyncSession
    )
}

impl Context {
    pub(super) fn special(self) -> bool {
        matches!(self, Self::Stream | Self::Etl)
    }

    pub(super) fn enter(
        self,
        receiver: &str,
        env: &mut State,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        if !matches!(self, Self::Standard) {
            return;
        }
        let receiver = AstExpr {
            range,
            kind: AstExprKind::Name(receiver.to_owned()),
        };
        let Some((object, _, kind)) = object_for(&receiver, env) else {
            return;
        };
        if !matches!(kind, Resource::Client | Resource::AsyncClient) {
            return;
        }
        let object = object.to_owned();
        let definite_closed = env.objects.get(&object) == Some(&CLOSED);
        let was_started = env
            .protocol_states
            .get(&object)
            .is_some_and(|states| states.len() == 1 && states.contains("client-started"));
        if definite_closed || was_started {
            report(
                result,
                "TP1001",
                "reviewed HTTP client context requires a fresh client",
                span(file, range),
                Vec::new(),
                "definite",
                contract_identity(env.resources.get(&object)),
            );
            queue_raise(
                env,
                range,
                Some(AstExpr {
                    range,
                    kind: AstExprKind::Name("RuntimeError".into()),
                }),
            );
            env.alive = false;
        } else {
            set_transaction(&object, "client-started", env);
        }
    }

    pub(super) fn enter_special(
        self,
        expression: &AstExpr,
        env: &mut State,
        file: &SourceFile,
        result: &mut Inspection,
    ) {
        let (callee, args, kwargs) = call_parts(expression).expect("recognized context call");
        queue_raise(env, expression.range, None);
        let (scope, symbol, resource, status, client) = match self {
            Self::Stream => {
                let AstExprKind::Attribute(receiver, _) = &callee.kind else {
                    unreachable!("stream method")
                };
                let (object, scope, _) =
                    object_for(receiver, env).expect("recognized stream receiver");
                let status = ready(scope, false, env).and_then(|()| {
                    if env.objects.get(object) != Some(&OPEN)
                        || expr_name(receiver).is_some_and(|name| env.unknown_places.contains(name))
                    {
                        return Err("stream context requires a known open reviewed client");
                    }
                    arguments_supported(Operation::Request, "request", args, kwargs)
                        .then_some(())
                        .ok_or("stream context requires a literal HTTP request")
                });
                (
                    scope,
                    format!("{scope}.Client.stream"),
                    Resource::Stream,
                    status,
                    Some(object.to_owned()),
                )
            }
            Self::Etl => {
                let scope = "etlantic_sqlmodel";
                let status = ready(scope, false, env).and_then(|()| {
                    matches!((args, kwargs), ([engine], []) if object_for(engine, env).is_some_and(|(_, _, kind)| kind == Resource::Engine))
                        .then_some(()).ok_or("ETLantic session_scope requires a resolved reviewed SQLite engine")
                });
                (
                    scope,
                    "etlantic_sqlmodel.control_plane.session.session_scope".into(),
                    Resource::Session,
                    status,
                    None,
                )
            }
            _ => unreachable!("ordinary context goes through expression evaluation"),
        };
        if use_effect(
            scope,
            &symbol,
            "context-entry",
            status,
            file,
            expression.range,
            env,
            result,
        ) {
            let result_scope = if matches!(self, Self::Etl) {
                "sqlmodel"
            } else {
                scope
            }
            .to_owned();
            if let Some(client) = client {
                let already_started = env
                    .protocol_states
                    .get(&client)
                    .is_some_and(|states| states.len() == 1 && states.contains("client-started"));
                if !already_started {
                    set_transaction(&client, "client-started", env);
                    queue_raise(env, expression.range, None);
                }
            }
            allocate(&result_scope, resource, file, expression.range, env, result);
        }
    }

    pub(super) fn exit(
        self,
        receiver: &str,
        incoming: Option<&CompletionKind>,
        env: &mut State,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        let receiver = AstExpr {
            range,
            kind: AstExprKind::Name(receiver.to_owned()),
        };
        let Some((object, scope, kind)) = object_for(&receiver, env) else {
            boundary(
                result,
                file,
                range,
                "reviewed context exit cannot recover its receiver identity",
            );
            return;
        };
        let object = object.to_owned();
        let scope = scope.to_owned();
        let symbol = format!("{scope}.{}.__exit__", kind.name());
        use_effect(
            &scope,
            &symbol,
            "context-exit",
            Ok(()),
            file,
            range,
            env,
            result,
        );
        match self {
            Self::Transaction => {
                apply_operation(
                    if is_exceptional(incoming) {
                        Operation::Rollback
                    } else {
                        Operation::Commit
                    },
                    &scope,
                    kind,
                    &object,
                    env,
                    file,
                    range,
                    result,
                );
            }
            Self::Etl => {
                if is_exceptional(incoming) {
                    let exception = incoming.and_then(|kind| completion_exception_name(kind, env));
                    match exception {
                        Some(
                            "BaseException" | "KeyboardInterrupt" | "SystemExit" | "GeneratorExit"
                            | "CancelledError",
                        ) => {}
                        Some(_) => apply_operation(
                            Operation::Rollback,
                            &scope,
                            kind,
                            &object,
                            env,
                            file,
                            range,
                            result,
                        ),
                        None => {
                            boundary(result, file, range, "ETLantic rollback handler applicability is unknown for this exception");
                            env.protocol_unknown.insert(object.clone());
                        }
                    }
                } else {
                    apply_operation(
                        Operation::Commit,
                        &scope,
                        kind,
                        &object,
                        env,
                        file,
                        range,
                        result,
                    );
                    if let Some(mut failure) = env.pending_raises.pop() {
                        apply_operation(
                            Operation::Rollback,
                            &scope,
                            kind,
                            &object,
                            &mut failure.state,
                            file,
                            range,
                            result,
                        );
                        env.pending_raises
                            .extend(take_pending_raises(&mut failure.state));
                        env.pending_raises.push(failure);
                    }
                }
                let mut failures = take_pending_raises(env);
                for failure in &mut failures {
                    apply_operation(
                        Operation::Close,
                        &scope,
                        kind,
                        &object,
                        &mut failure.state,
                        file,
                        range,
                        result,
                    );
                    env.pending_raises
                        .extend(take_pending_raises(&mut failure.state));
                }
                env.pending_raises.extend(failures);
                apply_operation(
                    Operation::Close,
                    &scope,
                    kind,
                    &object,
                    env,
                    file,
                    range,
                    result,
                );
            }
            Self::Standard | Self::Stream => {
                let mut cancelled = kind.asynchronous().then(|| env.clone());
                apply_operation(
                    Operation::Close,
                    &scope,
                    kind,
                    &object,
                    env,
                    file,
                    range,
                    result,
                );
                if let Some(mut cancelled) = cancelled.take() {
                    if kind == Resource::AsyncSession && env.objects.get(&object) == Some(&CLOSED) {
                        // Cancellation of the waiting task on this success edge is
                        // propagated only after the shielded close completes.
                        set_closed(&object, &mut cancelled, file, range);
                        set_transaction(&object, "reset", &mut cancelled);
                    } else {
                        *cancelled.objects.entry(object).or_default() |= UNKNOWN;
                    }
                    cancelled.pending_raises.clear();
                    env.pending_raises.push(FlowExit {
                        state: cancelled,
                        range,
                        kind: CompletionKind::Cancelled(None),
                    });
                }
            }
        }
    }
}

fn is_exceptional(completion: Option<&CompletionKind>) -> bool {
    matches!(
        completion,
        Some(CompletionKind::Raise(_) | CompletionKind::Cancelled(_))
    )
}

fn request_capture<'a>(
    record: &CoroutineRecord,
    env: &'a State,
) -> Option<(&'a str, &'a str, Resource)> {
    let (callee, _, _) = call_parts(&record.invocation)?;
    let AstExprKind::Attribute(receiver, method) = &callee.kind else {
        return None;
    };
    let place = record.captures.get(expr_name(receiver)?)?;
    let object = place
        .targets
        .first()
        .filter(|_| !place.unknown && place.targets.len() == 1)?;
    let (object, contract) = env.resources.get_key_value(object)?;
    let (scope, kind) = library_resource(contract)?;
    matches!(
        operation(kind, method),
        Some(Operation::Request | Operation::Query(_) | Operation::Commit | Operation::Rollback)
    )
    .then_some((object, scope, kind))
}

pub(super) fn direct_task(record: &CoroutineRecord, env: &State) -> bool {
    request_capture(record, env).is_some_and(|(_, _, kind)| kind.asynchronous())
}

pub(super) fn check_scheduled_operation(
    id: &str,
    env: &State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some((object, scope, kind)) = env
        .coroutines
        .get(id)
        .and_then(|record| request_capture(record, env))
    else {
        return;
    };
    check_live_uses(Some(id), object, scope, kind, env, file, range, result);
}

#[allow(clippy::too_many_arguments)]
fn check_live_uses(
    id: Option<&str>,
    object: &str,
    scope: &str,
    kind: Resource,
    env: &State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mut affected = Vec::new();
    let mut unknown = false;
    for previous in env
        .ownership
        .iter()
        .flat_map(|ownership| ownership.groups.values())
        .flat_map(|group| &group.tasks)
    {
        let (previous, task) = previous;
        if id == Some(previous.as_str()) || task.cancelled_before_start {
            continue;
        }
        let Some(record) = env
            .coroutines
            .get(previous)
            .filter(|record| !record.consumed)
        else {
            continue;
        };
        let captures = record
            .captures
            .values()
            .any(|place| place.targets.contains(object));
        if !captures {
            continue;
        }
        if request_capture(record, env).is_some() {
            affected.push(record.origin.clone());
        } else {
            unknown = true;
        }
    }
    if unknown {
        boundary(
            result,
            file,
            range,
            "task use of the same reviewed resource is unresolved",
        );
    }
    if !affected.is_empty() && kind == Resource::AsyncSession {
        report(
            result,
            "TP2006",
            "concurrent async operations on one session require task-local sessions",
            span(file, range),
            affected,
            "possible",
            vec![format!("{scope}.AsyncSession")],
        );
    }
}

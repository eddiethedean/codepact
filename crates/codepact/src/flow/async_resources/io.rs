use super::*;

const CONNECT: &str = "external:asyncio.open_connection";
const START_SERVER: &str = "external:asyncio.start_server";
const EXEC: &str = "external:asyncio.create_subprocess_exec";
const SHELL: &str = "external:asyncio.create_subprocess_shell";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Reader,
    Writer,
    Server,
    Process { pipes: bool },
}

impl Kind {
    fn symbol(self) -> &'static str {
        match self {
            Self::Reader => "asyncio.StreamReader",
            Self::Writer => "asyncio.StreamWriter",
            Self::Server => "asyncio.Server",
            Self::Process { .. } => "asyncio.subprocess.Process",
        }
    }

    fn operation(self, method: &str) -> Option<(&'static str, bool)> {
        match (self, method) {
            (Self::Reader, "read") => Some(("read-local-bytes", true)),
            (Self::Writer, "write") => Some(("buffer-local-write", false)),
            (Self::Writer, "drain") => Some(("wait-for-flow-control", true)),
            (Self::Writer | Self::Server, "close") => Some(("request-close", false)),
            (Self::Writer | Self::Server, "wait_closed") => Some(("observe-closure", true)),
            (Self::Server, "close_clients") => Some(("request-client-close", false)),
            (Self::Server, "abort_clients") => Some(("request-client-abort", false)),
            (Self::Server, "start_serving") => Some(("start-listening", true)),
            (Self::Process { .. }, "terminate" | "kill") => Some(("request-process-stop", false)),
            (Self::Process { .. }, "wait") => Some(("observe-process-exit", true)),
            (Self::Process { .. }, "communicate") => Some(("collect-pipes-and-observe-exit", true)),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Endpoint {
    host: String,
    port: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseWaiter {
    Available,
    Cancelled,
    MaybeCancelled,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Handle {
    kind: Kind,
    endpoint: Option<Endpoint>,
    clients: BTreeSet<String>,
    close_requested: bool,
    close_waiter: CloseWaiter,
    client_close_requested: bool,
    listening: bool,
    pub(super) uncertain: bool,
}

impl Handle {
    pub(super) fn merge(&mut self, other: &Self) {
        self.uncertain |= other.uncertain
            || self.kind != other.kind
            || self.endpoint != other.endpoint
            || self.close_requested != other.close_requested
            || self.client_close_requested != other.client_close_requested
            || self.listening != other.listening
            || self.clients != other.clients;
        self.close_requested &= other.close_requested;
        if self.close_waiter != other.close_waiter {
            self.close_waiter = CloseWaiter::MaybeCancelled;
        }
        self.client_close_requested &= other.client_close_requested;
        self.listening &= other.listening;
        self.clients.extend(other.clients.iter().cloned());
    }
}

fn identify(expression: &AstExpr, env: &State) -> Option<String> {
    let (function, _, _) = call_parts(expression)?;
    ownership::external_identity(function, env)
}

fn integer(expr: &AstExpr, file: &SourceFile) -> Option<usize> {
    if !matches!(expr.kind, AstExprKind::Primitive) {
        return None;
    }
    file.source
        .get(expr.range.start..expr.range.end)?
        .parse()
        .ok()
}

fn bytes_literal(expr: &AstExpr, file: &SourceFile) -> bool {
    let AstExprKind::OtherCalls(calls, names) = &expr.kind else {
        return false;
    };
    if !calls.is_empty() || !names.is_empty() {
        return false;
    }
    let Some(text) = file.source.get(expr.range.start..expr.range.end) else {
        return false;
    };
    let Some(raw) = text.strip_prefix('b').or_else(|| text.strip_prefix('B')) else {
        return false;
    };
    let Some(quote @ (b'\'' | b'"')) = raw.bytes().next() else {
        return false;
    };
    let mut escaped = false;
    for (index, byte) in raw.bytes().enumerate().skip(1) {
        if escaped {
            escaped = false;
        } else if byte == quote {
            return index + 1 == raw.len() && index <= 4096;
        } else if byte == b'\\' {
            escaped = true;
        }
    }
    false
}

fn endpoint(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    file: &SourceFile,
) -> Option<Endpoint> {
    if !keywords.is_empty() {
        return None;
    }
    let [host, port] = arguments else {
        return None;
    };
    let AstExprKind::String(host) = &host.kind else {
        return None;
    };
    if !matches!(host.as_str(), "127.0.0.1" | "::1") {
        return None;
    }
    Some(Endpoint {
        host: host.clone(),
        port: integer(port, file)?.try_into().ok()?,
    })
}

fn process_has_pipes(arguments: &[AstExpr], keywords: &[CallKeyword], env: &State) -> Option<bool> {
    let (executable, arguments) = arguments.split_first()?;
    if !matches!(&executable.kind, AstExprKind::String(path) if path.starts_with('/') && !path.contains('\0'))
        || !arguments.iter().all(|argument| {
            matches!(&argument.kind, AstExprKind::String(value) if !value.contains('\0'))
        })
    {
        return None;
    }
    let mut configured = BTreeSet::new();
    let mut pipes = false;
    for (name, value) in keywords {
        let name = name.as_deref()?;
        if !matches!(name, "stdin" | "stdout" | "stderr") || !configured.insert(name) {
            return None;
        }
        if matches!(value.kind, AstExprKind::NoneLiteral) {
            continue;
        }
        match ownership::external_identity(value, env).as_deref() {
            Some("external:asyncio.DEVNULL" | "external:asyncio.subprocess.DEVNULL") => {}
            Some("external:asyncio.PIPE" | "external:asyncio.subprocess.PIPE")
                if name != "stdin" =>
            {
                pipes = true;
            }
            _ => return None,
        }
    }
    Some(pipes)
}

fn insert(
    kind: Kind,
    endpoint: Option<Endpoint>,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> Option<String> {
    let close = match kind {
        Kind::Reader => "read",
        Kind::Writer | Kind::Server => "wait_closed",
        Kind::Process { .. } => "wait",
    };
    let id = allocate(
        kind.symbol(),
        close,
        IO_MODEL,
        !matches!(kind, Kind::Reader),
        env,
        file,
        expression.range,
        result,
    )?;
    if let Some(handle) = env.async_resources.io.get_mut(&id) {
        handle.uncertain = true;
        *env.objects.entry(id.clone()).or_default() |= UNKNOWN;
        boundary(
            result,
            file,
            expression.range,
            "repeated asyncio I/O allocation has indistinguishable instances",
        );
    } else {
        env.async_resources.io.insert(
            id.clone(),
            Handle {
                kind,
                endpoint,
                clients: BTreeSet::new(),
                close_requested: false,
                close_waiter: CloseWaiter::Available,
                client_close_requested: false,
                listening: false,
                uncertain: false,
            },
        );
    }
    Some(id)
}

fn acquisition(
    symbol: &str,
    operation: &str,
    shape: Result<(), &'static str>,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let selection = applicable(env).and({
        if env.async_context {
            shape
        } else {
            Err("asyncio I/O requires an established running loop")
        }
    });
    if !runtime::use_model(symbol, operation, IO_MODEL, selection, file, range, result) {
        queue_raise(env, range, None);
        return false;
    }
    // Cancellation and startup error precede delivery of any local handles.
    cancellation(env, range);
    queue_raise(env, range, None);
    true
}

fn unpack_names(target: Option<&AstExpr>) -> Option<(&str, &str)> {
    let (AstExprKind::Tuple(items) | AstExprKind::Collection(items)) = &target?.kind else {
        return None;
    };
    let [first, second] = items.as_slice() else {
        return None;
    };
    let (first, second) = (expr_name(first)?, expr_name(second)?);
    (first != second).then_some((first, second))
}

pub(in crate::flow) fn unpack(
    value: &AstExpr,
    target: Option<&AstExpr>,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    let AstExprKind::Await(expression) = &value.kind else {
        return false;
    };
    if !active(env) {
        return false;
    }
    if identify(expression, env).as_deref() != Some(CONNECT) {
        if let Some((receiver, "communicate", _, _)) = called_method(expression) {
            if object_for(receiver, env).is_some_and(|id| {
                env.async_resources
                    .io
                    .get(&id)
                    .is_some_and(|handle| matches!(handle.kind, Kind::Process { .. }))
            }) {
                if let Some((stdout, stderr)) = unpack_names(target) {
                    consume_expr(value, env, file, contracts, functions, result, 0);
                    if env.alive {
                        env.bind_runtime_handle(stdout);
                        env.bind_runtime_handle(stderr);
                    }
                    return true;
                }
            }
        }
        return false;
    }
    let (_, arguments, keywords) = call_parts(expression).expect("resolved connection call");
    evaluate_call_arguments(expression, env, file, contracts, functions, result, 0);
    if !env.alive {
        return true;
    }
    let names = unpack_names(target);
    let endpoint = endpoint(arguments, keywords, file).filter(|endpoint| endpoint.port != 0);
    let shape = (names.is_some() && endpoint.is_some())
        .then_some(())
        .ok_or("asyncio.open_connection requires a literal loopback endpoint and fixed reader/writer unpacking")
        .and_then(|()| (env.async_resources.io.len() + 2 <= MAX_HANDLES)
            .then_some(()).ok_or("asyncio I/O handle budget exceeded"));
    if !acquisition(
        "asyncio.open_connection",
        "acquire-stream-pair",
        shape,
        env,
        file,
        value.range,
        result,
    ) {
        return true;
    }
    let endpoint = endpoint.expect("applicable endpoint");
    let (reader, writer) = names.expect("applicable targets");
    if let Some(id) = insert(Kind::Reader, None, expression, env, file, result) {
        bind(reader, BTreeSet::from([id]), env);
    }
    if let Some(id) = insert(Kind::Writer, None, expression, env, file, result) {
        bind(writer, BTreeSet::from([id.clone()]), env);
        for handle in env.async_resources.io.values_mut() {
            if handle.kind == Kind::Server
                && handle.endpoint.as_ref() == Some(&endpoint)
                && handle.listening
            {
                handle.clients.insert(id.clone());
            }
        }
    }
    env.pending_return_targets = None;
    true
}

pub(super) fn is_operation(expression: &AstExpr, env: &State) -> bool {
    matches!(
        identify(expression, env).as_deref(),
        Some(CONNECT | START_SERVER | EXEC | SHELL)
    ) || called_method(expression).is_some_and(|(receiver, _, _, _)| {
        expr_name(receiver)
            .and_then(|name| env.places.get(name))
            .is_some_and(|objects| {
                objects
                    .iter()
                    .any(|id| env.async_resources.io.contains_key(id))
            })
    })
}

#[allow(clippy::too_many_arguments)]
fn method_call(
    expression: &AstExpr,
    receiver: &AstExpr,
    method: &str,
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let Some(id) = object_for(receiver, env) else {
        if is_operation(expression, env) {
            boundary(
                result,
                file,
                expression.range,
                "asyncio I/O receiver identity varies across paths",
            );
            queue_raise(env, expression.range, None);
            return true;
        }
        return false;
    };
    let Some(handle) = env.async_resources.io.get(&id).cloned() else {
        return false;
    };
    let range = expression.range;
    let Some((effect, asynchronous)) = handle.kind.operation(method) else {
        boundary(
            result,
            file,
            range,
            "asyncio I/O operation is outside the reviewed profile",
        );
        queue_raise(env, range, None);
        return true;
    };
    let shape = match (handle.kind, method, arguments, keywords) {
        (Kind::Reader, "read", [size], []) => integer(size, file).is_some_and(|size| size <= 4096),
        (Kind::Writer, "write", [value], []) => bytes_literal(value, file),
        (_, _, [], []) => true,
        _ => false,
    };
    let is_awaited = awaited(expression, env, file);
    let is_open = env.objects.get(&id).copied() == Some(OPEN);
    let already_closed = env.objects.get(&id).copied() == Some(CLOSED);
    let model = applicable(env).and_then(|()| {
        if !shape {
            Err("asyncio I/O arguments are outside the reviewed local subset")
        } else if asynchronous && !is_awaited {
            Err("asyncio I/O coroutine is created without an established await")
        } else if handle.uncertain
            || env.escaped.contains(&id)
            || env.objects.get(&id).is_some_and(|mask| mask & UNKNOWN != 0)
        {
            Err("asyncio I/O handle effects differ across paths")
        } else if matches!(handle.kind, Kind::Process { pipes: true })
            && method == "wait"
            && !already_closed
        {
            Err("subprocess wait with undrained output pipes cannot establish progress")
        } else if matches!(method, "close_clients" | "abort_clients" | "wait_closed")
            && !handle.close_requested
            && !already_closed
        {
            Err("asyncio I/O shutdown requires closing the known listener or writer first")
        } else {
            Ok(())
        }
    });
    let writer_wait = handle.kind == Kind::Writer && method == "wait_closed";
    let effect = if writer_wait && handle.close_waiter == CloseWaiter::Cancelled {
        "cancelled-close-waiter"
    } else {
        effect
    };
    let symbol = format!("{}.{method}", handle.kind.symbol());
    if !runtime::use_model(&symbol, effect, IO_MODEL, model, file, range, result) {
        if !asynchronous || is_awaited {
            queue_raise(env, range, None);
        }
        return true;
    }
    match method {
        "close" => {
            ownership::check_close(env, &id, file, range, result, false);
            let handle = env
                .async_resources
                .io
                .get_mut(&id)
                .expect("resolved I/O identity");
            handle.close_requested = true;
            handle.listening = false;
        }
        "start_serving" => {
            if handle.close_requested {
                rejected(env, range, "RuntimeError");
            } else if !handle.listening {
                // Once the listener starts, a subsequent interruption
                // retains the same server handle and local close duty.
                env.async_resources
                    .io
                    .get_mut(&id)
                    .expect("resolved I/O identity")
                    .listening = true;
                cancellation(env, range);
            }
        }
        "close_clients" | "abort_clients" => {
            env.async_resources
                .io
                .get_mut(&id)
                .expect("resolved I/O identity")
                .client_close_requested = true;
        }
        "terminate" | "kill" => {
            if already_closed {
                rejected(env, range, "ProcessLookupError");
            } else {
                // A process can exit before its stop request reaches the transport.
                queue_raise(env, range, None);
                env.async_resources
                    .io
                    .get_mut(&id)
                    .expect("resolved I/O identity")
                    .close_requested = true;
            }
        }
        "wait_closed" | "wait" | "communicate" => {
            if writer_wait && handle.close_waiter != CloseWaiter::Available {
                let mut interrupted = env.clone();
                interrupted.pending_raises.clear();
                interrupted
                    .async_resources
                    .io
                    .get_mut(&id)
                    .unwrap()
                    .close_waiter = CloseWaiter::Cancelled;
                env.pending_raises.push(FlowExit {
                    state: interrupted,
                    kind: CompletionKind::Cancelled(None),
                    range,
                });
                if handle.close_waiter == CloseWaiter::Cancelled {
                    env.alive = false;
                    return true;
                }
                env.async_resources.io.get_mut(&id).unwrap().close_waiter = CloseWaiter::Available;
            }
            if !already_closed {
                if handle.kind == Kind::Server
                    && !handle.clients.is_empty()
                    && !handle.client_close_requested
                {
                    boundary(result, file, range, "server wait cannot establish closure while an accepted client remains live");
                    cancellation(env, range);
                    return true;
                }
                if writer_wait {
                    cancellation_with(env, range, |interrupted| {
                        interrupted
                            .async_resources
                            .io
                            .get_mut(&id)
                            .unwrap()
                            .close_waiter = CloseWaiter::Cancelled;
                    });
                } else {
                    cancellation(env, range);
                }
                finish(env, &id, file, range);
                if handle.kind == Kind::Writer {
                    // The protocol's connection_lost error reaches the
                    // waiter after the underlying transport has ended.
                    queue_raise(env, range, None);
                }
                if handle.kind == Kind::Server && !handle.clients.is_empty() {
                    // Connection closure and handler return are independent, even
                    // when the handler invokes the transport cleanup itself.
                    boundary(result, file, range, "transport shutdown does not establish client-handler completion or result observation");
                }
            }
        }
        "read" if arguments.first().and_then(|arg| integer(arg, file)) == Some(0) => {}
        "read" | "drain" => {
            if method == "drain" && (!is_open || handle.close_requested) {
                rejected(env, range, "ConnectionResetError");
            } else {
                cancellation(env, range);
                queue_raise(env, range, None);
            }
        }
        "write" if !is_open || handle.close_requested => {
            report(
                result,
                "TP1001",
                "stream writer is used after its shutdown began",
                span(file, range),
                env.close_origins.get(&id).cloned().into_iter().collect(),
                if already_closed || handle.close_requested {
                    "definite"
                } else {
                    "possible"
                },
                contract_identity(env.resources.get(&id)),
            );
        }
        "write" => {}
        _ => unreachable!("reviewed operation has an effect"),
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
    match identify(expression, env).as_deref() {
        Some(identity @ (CONNECT | START_SERVER | EXEC | SHELL)) => {
            let symbol = identity.trim_start_matches("external:");
            let (_, arguments, keywords) = call_parts(expression).expect("modeled constructor");
            let serve_on_start = match keywords {
                [] => Some(true),
                [(
                    Some(name),
                    AstExpr {
                        kind: AstExprKind::Boolean(value),
                        ..
                    },
                )] if name == "start_serving" => Some(*value),
                _ => None,
            };
            let (kind, endpoint) = match identity {
                START_SERVER => {
                    let endpoint = arguments.split_first().and_then(|(handler, arguments)| {
                        let definition = resolved_function(handler, env, functions)?;
                        if !definition.is_async
                            || definition.parameters.len() != 2
                            || definition.async_generator.is_some()
                        {
                            return None;
                        }
                        serve_on_start?;
                        endpoint(arguments, &[], file)
                    });
                    (Kind::Server, endpoint)
                }
                _ => (
                    Kind::Process {
                        pipes: process_has_pipes(arguments, keywords, env).unwrap_or(false),
                    },
                    None,
                ),
            };
            let shape = if !awaited(expression, env, file) {
                Err("asyncio I/O constructor must be directly awaited")
            } else if env.async_resources.io.len() >= MAX_HANDLES {
                Err("asyncio I/O handle budget exceeded")
            } else {
                match identity {
                    CONNECT => Err("asyncio.open_connection requires fixed reader/writer unpacking"),
                    SHELL => Err("shell subprocess launch is outside the reviewed profile"),
                    START_SERVER if endpoint.is_none() => Err("asyncio.start_server requires a resolved async client handler and a literal loopback endpoint"),
                    EXEC if process_has_pipes(arguments, keywords, env).is_none() => Err("asyncio.create_subprocess_exec requires an absolute literal executable, fixed string arguments, and reviewed pipe options"),
                    _ => Ok(()),
                }
            };
            if !awaited(expression, env, file) {
                runtime::use_model(
                    symbol,
                    "create-unawaited-io-coroutine",
                    IO_MODEL,
                    shape,
                    file,
                    expression.range,
                    result,
                );
            } else if acquisition(
                symbol,
                "acquire-local-handle",
                shape,
                env,
                file,
                expression.range,
                result,
            ) {
                if let Some(id) = insert(kind, endpoint, expression, env, file, result) {
                    if kind == Kind::Server && serve_on_start == Some(true) {
                        env.async_resources
                            .io
                            .get_mut(&id)
                            .expect("resolved I/O identity")
                            .listening = true;
                        runtime::use_model(
                            symbol,
                            "interrupted-after-listen",
                            IO_MODEL,
                            Err("server startup can be interrupted after listening starts and before the handle is delivered"),
                            file,
                            expression.range,
                            result,
                        );
                    }
                }
            }
            true
        }
        _ => called_method(expression).is_some_and(|(receiver, method, arguments, keywords)| {
            method_call(
                expression, receiver, method, arguments, keywords, env, file, result,
            )
        }),
    }
}

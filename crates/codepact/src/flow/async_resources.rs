//! Bounded async-generator frames and asyncio I/O shutdown operations.
use super::*;
use std::rc::Rc;

mod generator;
mod io;

pub(super) use generator::{
    context_enter, context_exit, definition, execute_for, GeneratorDefinition,
};
pub(super) use io::unpack;

pub(super) const GENERATOR_MODEL: &str = "TPM-ASYNCGEN-001";
const IO_MODEL: &str = "TPM-ASYNCIO-IO-001";
const GENERATOR_HANDLE: &str = "async_generator";
const MAX_HANDLES: usize = 8;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Resources(Rc<ResourceTables>);

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct ResourceTables {
    generators: BTreeMap<String, generator::Generator>,
    io: BTreeMap<String, io::Handle>,
}

impl Default for Resources {
    fn default() -> Self {
        thread_local! {
            static EMPTY: Rc<ResourceTables> = Rc::default();
        }
        Self(EMPTY.with(Clone::clone))
    }
}

impl std::ops::Deref for Resources {
    type Target = ResourceTables;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Resources {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Rc::make_mut(&mut self.0)
    }
}

impl Resources {
    pub(super) fn join(&mut self, other: &Self) {
        if Rc::ptr_eq(&self.0, &other.0) {
            return;
        }
        for (id, generator) in &mut self.generators {
            match other.generators.get(id) {
                Some(other) if generator == other => {}
                Some(other) => generator.merge(other),
                None => generator.make_uncertain(),
            }
        }
        for (id, generator) in &other.generators {
            self.generators.entry(id.clone()).or_insert_with(|| {
                let mut absent = generator.clone();
                absent.make_uncertain();
                absent
            });
        }
        for (id, handle) in &mut self.io {
            match other.io.get(id) {
                Some(other) => handle.merge(other),
                None => handle.uncertain = true,
            }
        }
        for (id, handle) in &other.io {
            self.io.entry(id.clone()).or_insert_with(|| {
                let mut handle = handle.clone();
                handle.uncertain = true;
                handle
            });
        }
    }

    pub(super) fn copy_from(&mut self, other: &Self, objects: &BTreeSet<String>) {
        if other.io.is_empty() {
            return;
        }
        for object in objects {
            self.copy_io_handle(other, object);
        }
    }

    pub(super) fn copy_io_handle(&mut self, other: &Self, object: &str) {
        if let Some(handle) = other.io.get(object) {
            self.io.insert(object.to_owned(), handle.clone());
        }
    }

    pub(super) fn generator_captures<'a>(
        &'a self,
        object: &'a str,
    ) -> impl Iterator<Item = (Span, bool)> + 'a {
        self.generators
            .values()
            .filter_map(move |generator| generator.capture_site(object))
    }
}

fn active(env: &State) -> bool {
    env.ownership.is_some() && env.models.foundations.is_some()
}

pub(super) fn applicable(env: &State) -> Result<(), &'static str> {
    let default_factory = env.ownership.as_ref().is_some_and(|ownership| {
        ownership.mode.assumes_default_task_factory() && !ownership.incompatible_task_factory
    });
    runtime::runtime_profile(env)?.asyncio(&env.target_python_version, default_factory)
}

fn awaited(expression: &AstExpr, env: &State, file: &SourceFile) -> bool {
    is_awaited_call(expression, file, env)
}

fn object_for(expression: &AstExpr, env: &State) -> Option<String> {
    let name = expr_name(expression)?;
    if env.unknown_places.contains(name) || env.possibly_unbound_places.contains(name) {
        return None;
    }
    let targets = env.places.get(name)?;
    targets.first().filter(|_| targets.len() == 1).cloned()
}

fn bind(name: &str, objects: BTreeSet<String>, env: &mut State) {
    env.bind_runtime_handle(name);
    env.places.insert(name.to_owned(), objects);
}

#[allow(clippy::too_many_arguments)]
fn allocate(
    symbol: &str,
    close: &str,
    model: &str,
    owns_cleanup: bool,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> Option<String> {
    runtime::allocate_modeled_resource(
        env,
        Contract {
            schema_version: crate::SCHEMA_VERSION,
            base_symbol: symbol.into(),
            display_name: symbol.into(),
            lifecycle: Some(crate::Lifecycle {
                closed_by: close.into(),
            }),
            access: if owns_cleanup { "owned" } else { "shared" }.into(),
            source: span(file, range),
            provenance: vec![crate::Provenance {
                origin: model.into(),
                trust: "reviewed-model".into(),
            }],
        },
        file,
        range,
        result,
    );
    let id = env.pending_return_targets.as_ref()?.first()?.clone();
    if !owns_cleanup {
        env.duty_origins.remove(&id);
    }
    Some(id)
}

fn finish(env: &mut State, id: &str, file: &SourceFile, range: SourceRange) {
    env.objects.insert(id.to_owned(), CLOSED);
    env.close_origins.insert(id.to_owned(), span(file, range));
}

fn cancellation(env: &mut State, range: SourceRange) {
    cancellation_with(env, range, |_| {});
}

fn cancellation_with(env: &mut State, range: SourceRange, on_interrupt: impl FnOnce(&mut State)) {
    tasks::allow_tasks_to_start(env);
    let mut interrupted = env.clone();
    interrupted.alive = true;
    interrupted.pending_raises.clear();
    on_interrupt(&mut interrupted);
    asyncio::queue_deadline_cancellations(env, range, &interrupted, false);
    env.pending_raises.push(FlowExit {
        state: interrupted,
        kind: CompletionKind::Cancelled(None),
        range,
    });
}

fn rejected(env: &mut State, range: SourceRange, exception: &str) {
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

fn resolved_function<'a>(
    function: &AstExpr,
    env: &State,
    functions: &'a FunctionMap,
) -> Option<&'a FunctionBody> {
    let name = expr_name(function)?;
    if env.shadowed_functions.contains(name) {
        return None;
    }
    let definition = functions.get(env.function_bindings.get(name)?)?;
    definition.supported_binding.then_some(definition)
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> bool {
    active(env)
        && (generator::call(expression, env, file, functions, result)
            || io::call(expression, env, file, functions, result))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn await_call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> bool {
    if !active(env) {
        return false;
    }
    let awaits_generator = call_parts(expression)
        .and_then(|(function, _, _)| resolved_function(function, env, functions))
        .is_some_and(|definition| definition.async_generator.is_some());
    if !(awaits_generator
        || generator::is_operation(expression, env)
        || io::is_operation(expression, env))
    {
        return false;
    }
    let previous = env
        .awaited_call
        .replace((file.path.clone(), expression.range));
    consume_expr(
        expression,
        env,
        file,
        contracts,
        functions,
        result,
        depth + 1,
    );
    env.awaited_call = previous;
    if awaits_generator && env.alive {
        rejected(env, expression.range, "TypeError");
    }
    true
}

pub(super) fn report_scope(env: &State, result: &mut Inspection) {
    for generator in env.async_resources.generators.values() {
        generator.report_abandonment(result);
    }
}

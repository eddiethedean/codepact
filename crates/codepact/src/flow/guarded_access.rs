use super::{
    assigned_names, async_protocols, boundary, called_method, expr_name, report, resource_method,
    span,
};
use super::{
    AstExpr, AstExprKind, FlowStatement, FunctionMap, Inspection, SourceFile, SourceRange, State,
};
use crate::foundation::AsyncGuardModel;
use std::collections::BTreeSet;

const MODEL: &str = "TPM-SYNC-001";
const RULE: &str = "TP7002";

#[derive(Clone, PartialEq, Eq)]
pub(super) struct GuardBinding {
    model: AsyncGuardModel,
    objects: BTreeSet<String>,
    exact: bool,
    lock: Option<String>,
}

fn local_binding(env: &State, model: &AsyncGuardModel) -> GuardBinding {
    let objects = env.places.get(&model.resource).cloned().unwrap_or_default();
    GuardBinding {
        model: model.clone(),
        exact: objects.len() == 1
            && !env.unknown_places.contains(&model.resource)
            && !env.possibly_unbound_places.contains(&model.resource),
        objects,
        lock: async_protocols::lock_binding(env, &model.lock).map(str::to_owned),
    }
}

pub(super) fn inherit(parent: &State, child: &mut State) {
    let bindings = parent
        .async_guard_models
        .iter()
        .map(|model| local_binding(parent, model))
        .chain(parent.inherited_async_guards.iter().cloned());
    for mut binding in bindings {
        if !child
            .places
            .values()
            .any(|objects| !objects.is_disjoint(&binding.objects))
        {
            continue;
        }
        if let Some(id) = &binding.lock {
            if matches!(
                async_protocols::held_lock_by_id(parent, id),
                async_protocols::HeldLock::Unknown
            ) {
                binding.lock = None;
            } else if let (Some(parent), Some(child)) = (&parent.ownership, &mut child.ownership) {
                child
                    .async_protocols
                    .copy_from(&parent.async_protocols, &BTreeSet::from([id.clone()]));
            }
        }
        child.inherited_async_guards.push(binding);
    }
}

pub(super) fn validate_coroutines(
    models: &[AsyncGuardModel],
    declarations: &[super::Declaration],
    file: &SourceFile,
    result: &mut Inspection,
) {
    for model in models.iter().filter(|model| model.module == file.module) {
        let matches = declarations.iter().filter(|declaration| {
            matches!(declaration, super::Declaration::Function { name, is_async: true, .. }
                if name == &model.coroutine)
        });
        if matches.count() != 1 {
            boundary(
                result,
                file,
                SourceRange { start: 0, end: 0 },
                &format!(
                    "reviewed asyncio guard requires unique coroutine {}.{}",
                    model.module, model.coroutine
                ),
            );
        }
    }
}

pub(super) fn prepare(
    env: &State,
    coroutine: &str,
    body: &[FlowStatement],
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> Vec<AsyncGuardModel> {
    let Some(foundations) = &env.models.foundations else {
        return Vec::new();
    };
    foundations.async_guards.iter().filter_map(|model| {
            if file.module != model.module || coroutine != model.coroutine {
            return None;
        }
        if !env.async_context || [&model.resource, &model.lock].into_iter().any(|name| {
            env.definitely_bound_names.contains(name) || !unique_allocation(name, body)
        }) {
            boundary(result, file, range, "the reviewed asyncio guard requires distinct, unique local allocations in the selected coroutine");
            return None;
        }
        Some(model.clone())
    }).collect()
}

fn unique_allocation(name: &str, body: &[FlowStatement]) -> bool {
    let mut bindings = body.iter().filter(|statement| {
        assigned_names(std::slice::from_ref(*statement)).contains(name)
            || matches!(statement, FlowStatement::DeleteLocal { names, .. } if names.iter().any(|target| target == name))
    });
    matches!(
        bindings.next(),
        Some(FlowStatement::Assign { name: binding, value: AstExpr { kind: AstExprKind::Call { .. }, .. }, .. })
            if binding == name
    ) && bindings.next().is_none()
}

pub(super) fn check_binding(
    name: &str,
    env: &State,
    file: &SourceFile,
    range: SourceRange,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    for model in &env.async_guard_models {
        let reason = if name == model.resource {
            let resolved = env
                .places
                .get(name)
                .filter(|objects| objects.len() == 1 && !env.unknown_places.contains(name))
                .and_then(BTreeSet::first)
                .and_then(|object| env.resources.get(object))
                .and_then(|resource| resource_method(resource, &model.method, functions));
            resolved
                .is_none()
                .then_some("the configured guarded resource method is unresolved")
        } else if name == model.lock {
            matches!(
                async_protocols::held_lock(env, name),
                async_protocols::HeldLock::Unknown
            )
            .then_some("the configured asyncio lock allocation is unresolved")
        } else {
            None
        };
        if let Some(reason) = reason {
            incomplete(result, file, range, &span(file, range), reason);
        }
    }
}

pub(super) fn check(
    expression: &AstExpr,
    env: &State,
    file: &SourceFile,
    functions: &FunctionMap,
    result: &mut Inspection,
) {
    if env.async_guard_models.is_empty() && env.inherited_async_guards.is_empty() {
        return;
    }
    let Some((receiver, method, _, _)) = called_method(expression) else {
        return;
    };
    let Some(receiver) = expr_name(receiver) else {
        return;
    };
    let origin = span(file, expression.range);
    let receiver_objects = env.places.get(receiver);
    let local = env
        .async_guard_models
        .iter()
        .filter(|model| model.method == method)
        .map(|model| (true, local_binding(env, model)));
    let inherited = env
        .inherited_async_guards
        .iter()
        .filter(|binding| binding.model.method == method)
        .cloned()
        .map(|binding| (false, binding));
    for (local, binding) in local.chain(inherited) {
        let model = &binding.model;
        if !(local && receiver == model.resource)
            && receiver_objects.is_none_or(|objects| objects.is_disjoint(&binding.objects))
        {
            continue;
        }
        let exact = (binding.exact && receiver_objects == Some(&binding.objects))
            .then(|| binding.objects.first())
            .flatten()
            .filter(|object| !env.escaped.contains(*object))
            .and_then(|object| env.resources.get(object));
        let resolved = exact.and_then(|resource| resource_method(resource, method, functions));
        if env.unknown_places.contains(receiver)
            || env.possibly_unbound_places.contains(receiver)
            || resolved.is_none()
        {
            incomplete(
                result,
                file,
                expression.range,
                &origin,
                "the protected resource method or its allocation is unresolved",
            );
            continue;
        }
        if resolved.is_some_and(|definition| definition.is_async)
            && !super::is_awaited_call(expression, file, env)
        {
            continue;
        }
        let lock = binding
            .lock
            .as_deref()
            .map_or(async_protocols::HeldLock::Unknown, |id| {
                async_protocols::held_lock_by_id(env, id)
            });
        let (guard, held) = match lock {
            async_protocols::HeldLock::Held(guard) => (guard, true),
            async_protocols::HeldLock::Unheld(guard) => (guard, false),
            async_protocols::HeldLock::Unknown => {
                incomplete(
                    result,
                    file,
                    expression.range,
                    &origin,
                    "the required asyncio lock or its held state is unresolved",
                );
                continue;
            }
        };
        let identity = vec![
            MODEL.into(),
            format!("{}.{}", model.resource, method),
            format!(
                "{}:{}",
                model.lock,
                super::protocol_guard_identity(file, &guard)
            ),
        ];
        crate::foundation::record_model_use(
            "asyncio.Lock",
            "guarded-access",
            MODEL,
            origin.clone(),
            Ok(()),
        );
        crate::review::operations::record_protocol(
            "asyncio.guarded-access",
            origin.clone(),
            identity.clone(),
        );
        crate::review::operations::record(|| crate::review::operations::CleanupSite {
            kind: RULE,
            origin: origin.clone(),
            contract: identity.clone(),
            completion: "normal".into(),
            closed: held,
        });
        if !held {
            report(
                result,
                RULE,
                "the required asyncio lock is absent at the guarded resource operation",
                origin.clone(),
                vec![origin.clone(), guard],
                "definite",
                identity,
            );
        }
    }
}

fn incomplete(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    origin: &crate::Span,
    reason: &str,
) {
    crate::foundation::record_model_use(
        "asyncio.Lock",
        "guarded-access",
        MODEL,
        origin.clone(),
        Err(reason),
    );
    boundary(result, file, range, reason);
}

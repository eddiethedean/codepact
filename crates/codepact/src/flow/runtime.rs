use super::*;
use crate::foundation::{record_model_use, RuntimeProfile, CATALOG_REVISION};

pub(super) const FILE_RESOURCE: &str = "_io.TextIOWrapper";
const FILE_MODEL: &str = "TPM-FILE-001";

pub(super) fn runtime_profile(env: &State) -> Result<&RuntimeProfile, &'static str> {
    env.models
        .foundations
        .as_ref()
        .and_then(|inputs| inputs.runtime.as_ref())
        .ok_or("an explicit analyzed runtime profile is required")
}

pub(super) fn module_runtime_mutations(
    declarations: &[Declaration],
    bindings: &BTreeMap<String, String>,
) -> (bool, bool) {
    let probe = State {
        function_bindings: bindings.clone(),
        ..State::default()
    };
    let mut custom_loop = false;
    let mut custom_wait = false;
    for declaration in declarations {
        let Declaration::Expression { value, .. } = declaration else {
            continue;
        };
        custom_loop |= tasks::sets_task_factory_on_loop(value, &probe);
        let Some((callee, _, _)) = call_parts(value) else {
            continue;
        };
        match ownership::external_identity(callee, &probe).as_deref() {
            Some("external:signal.signal") => custom_wait = true,
            Some("external:asyncio.set_event_loop" | "external:asyncio.set_event_loop_policy") => {
                custom_loop = true
            }
            _ => {}
        }
    }
    (custom_loop, custom_wait)
}

pub(super) fn use_model(
    symbol: &str,
    effect: &str,
    model: &str,
    applicability: Result<(), &str>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    record_model_use(
        symbol,
        effect,
        &format!("{model}@{CATALOG_REVISION}"),
        span(file, range),
        applicability,
    );
    if let Err(reason) = applicability {
        boundary(result, file, range, reason);
        return false;
    }
    true
}

pub(super) fn is_builtin_open(expression: &AstExpr, env: &State) -> bool {
    if env.models.foundations.is_none() {
        return false;
    }
    let Some((function, _, _)) = call_parts(expression) else {
        return false;
    };
    ownership::external_identity(function, env).as_deref() == Some("external:builtins.open")
        || (expr_name(function) == Some("open")
            && !env.shadowed_functions.contains("open")
            && !env.definitely_bound_names.contains("open")
            && !env.function_bindings.contains_key("open")
            && !env.class_bindings.contains_key("open"))
}

pub(super) fn tracked_text_file<'a>(expression: &AstExpr, env: &'a State) -> Option<&'a str> {
    let name = expr_name(expression)?;
    if env.unknown_places.contains(name) || env.possibly_unbound_places.contains(name) {
        return None;
    }
    let targets = env.places.get(name)?;
    let target = targets.first().filter(|_| targets.len() == 1)?;
    env.resources
        .get(target)
        .is_some_and(|resource| resource.base_symbol == FILE_RESOURCE)
        .then_some(target.as_str())
}

fn open_mode(arguments: &[AstExpr], keywords: &[CallKeyword]) -> Option<&'static str> {
    let mut path = arguments.first();
    let mut mode = arguments.get(1);
    let mut encoding = None;
    if arguments.len() > 2 {
        return None;
    }
    for (name, value) in keywords {
        let slot = match name.as_deref() {
            Some("file") => &mut path,
            Some("mode") => &mut mode,
            Some("encoding") => &mut encoding,
            _ => return None,
        };
        if slot.replace(value).is_some() {
            return None;
        }
    }
    if !matches!(path?.kind, AstExprKind::String(_))
        || encoding.is_some_and(|value| {
            !matches!(&value.kind, AstExprKind::NoneLiteral)
                && !matches!(&value.kind, AstExprKind::String(value) if matches!(value.as_str(), "utf-8" | "UTF-8"))
        })
    {
        return None;
    }
    match mode.map(|value| &value.kind) {
        None => Some("read"),
        Some(AstExprKind::String(value)) if value == "r" => Some("read"),
        Some(AstExprKind::String(value)) if value == "w" => Some("write"),
        _ => None,
    }
}

pub(super) fn open_resource(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if !is_builtin_open(expression, env) {
        return false;
    }
    let (_, arguments, keywords) = call_parts(expression).expect("resolved open call");
    let mode = open_mode(arguments, keywords);
    let applies = runtime_profile(env)
        .and_then(|profile| profile.applicability(&env.target_python_version))
        .and_then(|()| {
            mode.map(|_| ()).ok_or("the file model requires an ordinary literal path, default or literal text mode, and no custom opener")
        });
    queue_raise(env, expression.range, None);
    if use_model(
        "builtins.open",
        "fresh-owned-return",
        FILE_MODEL,
        applies,
        file,
        expression.range,
        result,
    ) {
        let contract = Contract {
            schema_version: crate::SCHEMA_VERSION,
            base_symbol: FILE_RESOURCE.into(),
            display_name: FILE_RESOURCE.into(),
            lifecycle: Some(crate::Lifecycle {
                closed_by: "close".into(),
            }),
            access: "owned".into(),
            source: span(file, expression.range),
            provenance: vec![crate::Provenance {
                origin: format!("{FILE_MODEL}:{}", mode.expect("applicable open mode")),
                trust: "reviewed-model".into(),
            }],
        };
        allocate_modeled_resource(env, contract, file, expression.range, result);
    }
    true
}

pub(super) fn allocate_modeled_resource(
    env: &mut State,
    contract: Contract,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let object = format!(
        "{}:{}:{}:{}",
        file.path,
        range.start,
        contract.base_symbol,
        env.allocation_context.join("/")
    );
    let previous = env.objects.get(&object).copied();
    if previous.is_none() && env.objects.len() >= MAX_FLOW_OBJECTS {
        boundary(
            result,
            file,
            range,
            "tracked resource object budget exceeded; modeled return is unknown",
        );
        return;
    }
    let mask = match previous {
        Some(mask) if env.loop_depth > 0 && mask & (OPEN | UNKNOWN) != 0 => {
            boundary(
                result,
                file,
                range,
                "repeated allocation keeps an earlier cleanup duty unresolved",
            );
            mask | OPEN | UNKNOWN
        }
        _ => OPEN,
    };
    env.objects.insert(object.clone(), mask);
    env.resources.insert(object.clone(), contract);
    env.duty_origins.insert(object.clone(), span(file, range));
    env.pending_return_targets = Some(BTreeSet::from([object]));
}

pub(super) fn text_file_io(
    resource: Option<&Contract>,
    method: &str,
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
) -> bool {
    let Some(resource) = resource.filter(|value| value.base_symbol == FILE_RESOURCE) else {
        return false;
    };
    if !keywords.is_empty() {
        return false;
    }
    match (method, arguments) {
        (
            "read",
            []
            | [AstExpr {
                kind: AstExprKind::Primitive,
                ..
            }],
        ) => resource
            .provenance
            .iter()
            .any(|entry| entry.origin == "TPM-FILE-001:read"),
        (
            "write",
            [AstExpr {
                kind: AstExprKind::String(_) | AstExprKind::Name(_),
                ..
            }],
        ) => resource
            .provenance
            .iter()
            .any(|entry| entry.origin == "TPM-FILE-001:write"),
        _ => false,
    }
}

pub(super) fn file_close_use(file: &SourceFile, range: SourceRange) {
    record_model_use(
        "_io.TextIOWrapper.close",
        "receiver-cleanup",
        &format!("{FILE_MODEL}@{CATALOG_REVISION}"),
        span(file, range),
        Ok(()),
    );
}

use super::*;

pub(super) fn function_is_must_use(
    decorators: &[AstExpr],
    returns: Option<&AstExpr>,
    imports: &BTreeMap<String, String>,
) -> bool {
    let decorated = decorators
        .iter()
        .any(|decorator| canonical(decorator, imports).as_deref() == Some("codepact::must_use"));
    decorated || returns.is_some_and(|annotation| annotation_is_must_use(annotation, imports))
}

pub(super) fn supported_method_must_use(
    decorators: &[AstExpr],
    returns: Option<&AstExpr>,
    imports: &BTreeMap<String, String>,
) -> Result<bool, ()> {
    if !function_is_must_use(decorators, returns, imports) {
        return Ok(false);
    }
    let mut marker_index = None;
    let mut wrapper_index = None;
    for (index, decorator) in decorators.iter().enumerate() {
        match canonical(decorator, imports).as_deref() {
            Some("codepact::must_use") => marker_index = Some(index),
            Some("builtins::staticmethod" | "builtins::classmethod") => {
                if wrapper_index.replace(index).is_some() {
                    return Err(());
                }
            }
            _ => return Err(()),
        }
    }
    if let (Some(marker), Some(wrapper)) = (marker_index, wrapper_index) {
        if wrapper > marker {
            return Err(());
        }
    }
    Ok(true)
}

pub(super) fn annotation_is_must_use(
    annotation: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    match &annotation.kind {
        AstExprKind::Subscript(container, arguments) => {
            match canonical(container, imports).as_deref() {
                Some("codepact::MustUse") => true,
                Some("typing::Annotated") => arguments.iter().skip(1).any(|metadata| {
                    let expression = match &metadata.kind {
                        AstExprKind::Call { function, .. } => function.as_ref(),
                        _ => metadata,
                    };
                    canonical(expression, imports).as_deref()
                        == Some("codepact::contracts::MustUseResult")
                }),
                _ => false,
            }
        }
        _ => false,
    }
}

pub(super) fn contains_must_use_marker(
    expression: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    if annotation_is_must_use(expression, imports)
        || canonical(expression, imports).is_some_and(|identity| {
            matches!(
                identity.as_str(),
                "codepact::MustUse" | "codepact::contracts::MustUseResult"
            )
        })
    {
        return true;
    }
    match &expression.kind {
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            contains_must_use_marker(value, imports)
        }
        AstExprKind::Subscript(container, arguments) => {
            contains_must_use_marker(container, imports)
                || arguments
                    .iter()
                    .any(|argument| contains_must_use_marker(argument, imports))
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| contains_must_use_marker(argument, imports)),
        AstExprKind::OtherCalls(calls, _) => calls.iter().any(|call| {
            canonical_path(call, imports).is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "codepact::MustUse" | "codepact::contracts::MustUseResult"
                )
            })
        }),
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            contains_must_use_marker(function, imports)
                || arguments
                    .iter()
                    .any(|argument| contains_must_use_marker(argument, imports))
                || keywords
                    .iter()
                    .any(|(_, value)| contains_must_use_marker(value, imports))
        }
        _ => false,
    }
}

pub(super) fn report_misplaced_must_use_annotation(
    result: &mut Inspection,
    file: &SourceFile,
    annotation: &AstExpr,
    imports: &BTreeMap<String, String>,
) {
    if contains_must_use_marker(annotation, imports) {
        declaration_error(
            result,
            file,
            annotation.range,
            "TP4002",
            "must-use metadata is supported only on function return annotations",
            Vec::new(),
        );
    }
}

pub(super) fn is_misplaced_must_use_decorator(
    decorator: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    let identity = match &decorator.kind {
        AstExprKind::Call { function, .. } => canonical(function, imports),
        _ => canonical(decorator, imports),
    };
    identity.is_some_and(|identity| {
        matches!(
            identity.as_str(),
            "codepact::MustUse" | "codepact::contracts::MustUseResult"
        )
    })
}

pub(super) fn check_must_use_body(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    state: &FileState,
    class_scope: Option<(&str, Option<&str>)>,
    parameters: &[String],
) {
    let mut locals = parameters.iter().cloned().collect::<BTreeSet<_>>();
    collect_must_use_locals(statements, &mut locals);
    let mut marked = state.must_use_callables.clone();
    let mut imports = state.imports.clone();
    let mut instances = state.instances.clone();
    for name in locals {
        marked.remove(&name);
        imports.remove(&name);
        instances.remove(&name);
    }
    walk_must_use_body(
        result,
        file,
        statements,
        &mut marked,
        &mut instances,
        &imports,
        &state.known_classes,
        class_scope,
        0,
    );
}

fn collect_must_use_locals(
    statements: &[crate::frontend::FlowStatement],
    names: &mut BTreeSet<String>,
) {
    use crate::frontend::FlowStatement as Flow;
    for statement in statements {
        match statement {
            Flow::Assign { name, .. }
            | Flow::EscapingAssign { name, .. }
            | Flow::BindFunction { name, .. } => {
                names.insert(name.clone());
            }
            Flow::AnnotatedDeclaration {
                name: Some(name), ..
            } => {
                names.insert(name.clone());
            }
            Flow::If {
                body, alternate, ..
            }
            | Flow::Loop {
                body, alternate, ..
            } => {
                collect_must_use_locals(body, names);
                collect_must_use_locals(alternate, names);
            }
            Flow::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                collect_must_use_locals(body, names);
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        names.insert(name.clone());
                    }
                    collect_must_use_locals(&handler.body, names);
                }
                collect_must_use_locals(alternate, names);
                collect_must_use_locals(finalbody, names);
            }
            Flow::With { items, body, .. } => {
                for item in items {
                    if let Some(name) = &item.target {
                        names.insert(name.clone());
                    }
                }
                collect_must_use_locals(body, names);
            }
            _ => {}
        }
    }
}

fn resolve_marked_callable(
    name: &str,
    marked: &BTreeMap<String, Span>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> Option<(String, Span)> {
    if let Some(declaration) = marked.get(name) {
        return Some((name.to_owned(), declaration.clone()));
    }
    if let Some((class, Some(receiver))) = class_scope {
        if let Some(method) = name.strip_prefix(&format!("{receiver}.")) {
            let resolved = format!("{class}.{method}");
            if let Some(declaration) = marked.get(&resolved) {
                return Some((resolved, declaration.clone()));
            }
        }
    }
    let (receiver, method) = name.split_once('.')?;
    let class = instances.get(receiver)?;
    let resolved = format!("{class}.{method}");
    marked
        .get(&resolved)
        .cloned()
        .map(|declaration| (resolved, declaration))
}

fn expression_has_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, Span>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    match &expression.kind {
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            expression_path(function).is_some_and(|name| {
                resolve_marked_callable(&name, marked, instances, class_scope).is_some()
            }) || arguments.iter().any(|argument| {
                expression_has_marked_call(argument, marked, instances, class_scope)
            }) || keywords
                .iter()
                .any(|(_, value)| expression_has_marked_call(value, marked, instances, class_scope))
        }
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            expression_has_marked_call(value, marked, instances, class_scope)
        }
        AstExprKind::Subscript(value, arguments) => {
            expression_has_marked_call(value, marked, instances, class_scope)
                || arguments.iter().any(|argument| {
                    expression_has_marked_call(argument, marked, instances, class_scope)
                })
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| expression_has_marked_call(argument, marked, instances, class_scope)),
        AstExprKind::OtherCalls(calls, _) => calls
            .iter()
            .any(|name| resolve_marked_callable(name, marked, instances, class_scope).is_some()),
        _ => false,
    }
}

/// Calls passed directly as an argument are considered immediate handling.
/// A marked call wrapped in another expression, await, container, or computed
/// callee remains an unqualified boundary.
fn has_unsupported_nested_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, Span>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    match &expression.kind {
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            expression_has_marked_call(function, marked, instances, class_scope)
                || arguments
                    .iter()
                    .chain(keywords.iter().map(|(_, value)| value))
                    .any(|argument| {
                        if direct_marked_call(argument, marked, instances, class_scope) {
                            has_unsupported_nested_marked_call(
                                argument,
                                marked,
                                instances,
                                class_scope,
                            )
                        } else {
                            expression_has_marked_call(argument, marked, instances, class_scope)
                        }
                    })
        }
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            expression_has_marked_call(value, marked, instances, class_scope)
        }
        AstExprKind::Subscript(value, arguments) => {
            expression_has_marked_call(value, marked, instances, class_scope)
                || arguments.iter().any(|argument| {
                    expression_has_marked_call(argument, marked, instances, class_scope)
                })
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| expression_has_marked_call(argument, marked, instances, class_scope)),
        AstExprKind::OtherCalls(..)
        | AstExprKind::Other
        | AstExprKind::Starred(_)
        | AstExprKind::DepthLimit => {
            expression_has_marked_call(expression, marked, instances, class_scope)
        }
        _ => false,
    }
}

fn direct_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, Span>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    let AstExprKind::Call { function, .. } = &expression.kind else {
        return false;
    };
    expression_path(function).is_some_and(|name| {
        resolve_marked_callable(&name, marked, instances, class_scope).is_some()
    })
}

fn merge_equal_bindings<K: Ord + Clone, V: PartialEq + Clone>(
    left: &mut BTreeMap<K, V>,
    right: &BTreeMap<K, V>,
) {
    left.retain(|name, value| right.get(name).is_some_and(|other| other == value));
}

fn must_use_sequence_terminates(
    statements: &[crate::frontend::FlowStatement],
    loop_depth: usize,
) -> bool {
    statements
        .iter()
        .any(|statement| must_use_statement_terminates(statement, loop_depth))
}

fn must_use_statement_terminates(
    statement: &crate::frontend::FlowStatement,
    loop_depth: usize,
) -> bool {
    use crate::frontend::FlowStatement as Flow;
    match statement {
        Flow::Return { .. } | Flow::Raise { .. } => true,
        Flow::Break { .. } | Flow::Continue { .. } => loop_depth > 0,
        Flow::If {
            body, alternate, ..
        } => {
            !alternate.is_empty()
                && must_use_sequence_terminates(body, loop_depth)
                && must_use_sequence_terminates(alternate, loop_depth)
        }
        Flow::Try {
            body,
            handlers,
            alternate,
            finalbody,
            ..
        } => {
            must_use_sequence_terminates(finalbody, loop_depth)
                || (must_use_sequence_terminates(body, loop_depth)
                    && (alternate.is_empty()
                        || must_use_sequence_terminates(alternate, loop_depth))
                    && handlers
                        .iter()
                        .all(|handler| must_use_sequence_terminates(&handler.body, loop_depth)))
        }
        Flow::With { body, .. } => must_use_sequence_terminates(body, loop_depth),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_must_use_body(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    marked: &mut BTreeMap<String, Span>,
    instances: &mut BTreeMap<String, String>,
    imports: &BTreeMap<String, String>,
    known_classes: &BTreeSet<String>,
    class_scope: Option<(&str, Option<&str>)>,
    loop_depth: usize,
) {
    use crate::frontend::FlowStatement as Flow;
    for statement in statements {
        match statement {
            Flow::Expression { value, .. } => {
                let call = match &value.kind {
                    AstExprKind::Call {
                        function,
                        arguments,
                        keywords,
                    } => Some((function.as_ref(), arguments.as_slice(), keywords.as_slice())),
                    _ => None,
                };
                let discard = call.is_some_and(|(function, _, _)| {
                    canonical(function, imports).as_deref() == Some("codepact::discard")
                });
                if discard {
                    if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                        unsupported(
                            result,
                            file,
                            value.range,
                            "must-use result is nested in an unsupported explicit discard expression",
                        );
                    }
                    continue;
                }
                let resolved = call.and_then(|(function, _, _)| {
                    expression_path(function).and_then(|name| {
                        resolve_marked_callable(&name, marked, instances, class_scope)
                    })
                });
                if let Some((resolved, declaration)) = resolved {
                    let primary = crate::source_span(&file.path, &file.source, value.range);
                    result.diagnostics.push(Diagnostic {
                        semantic_identity: vec![resolved.clone()],
                        code: "TP3001".into(),
                        severity: "warning".into(),
                        message: format!("must-use result from {resolved} is discarded"),
                        primary,
                        related: vec![declaration],
                        certainty: "definite".into(),
                    });
                }
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an expression whose immediate handling is unsupported",
                    );
                }
            }
            Flow::Assign {
                name,
                annotation,
                value,
                ..
            } => {
                if let Some(annotation) = annotation {
                    report_misplaced_must_use_annotation(result, file, annotation, imports);
                }
                if let Some(path) = expression_path(value) {
                    if let Some((_, declaration)) =
                        resolve_marked_callable(&path, marked, instances, class_scope)
                    {
                        marked.insert(name.clone(), declaration);
                    } else {
                        marked.remove(name);
                    }
                    if let Some(class) = instances.get(&path).cloned() {
                        instances.insert(name.clone(), class);
                    } else {
                        instances.remove(name);
                    }
                } else {
                    marked.remove(name);
                    if let AstExprKind::Call { function, .. } = &value.kind {
                        let class =
                            expression_path(function).filter(|class| known_classes.contains(class));
                        if let Some(class) = class {
                            instances.insert(name.clone(), class);
                        } else {
                            instances.remove(name);
                        }
                    } else {
                        instances.remove(name);
                    }
                }
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an assignment expression whose immediate handling is unsupported",
                    );
                }
            }
            Flow::EscapingAssign {
                name,
                annotation,
                value,
                ..
            } => {
                if let Some(annotation) = annotation {
                    report_misplaced_must_use_annotation(result, file, annotation, imports);
                }
                if expression_has_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use callable escapes through an unsupported assignment",
                    );
                }
                marked.remove(name);
                instances.remove(name);
            }
            Flow::AnnotatedDeclaration {
                name, annotation, ..
            } => {
                report_misplaced_must_use_annotation(result, file, annotation, imports);
                if let Some(name) = name {
                    marked.remove(name);
                    instances.remove(name);
                }
            }
            Flow::Return {
                value: Some(value), ..
            } => {
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an unsupported returned expression",
                    );
                }
            }
            Flow::Raise {
                exception, cause, ..
            } => {
                for expression in exception.iter().chain(cause.iter()) {
                    if expression_has_marked_call(expression, marked, instances, class_scope) {
                        unsupported(
                            result,
                            file,
                            expression.range,
                            "must-use result is evaluated in an unsupported raise expression",
                        );
                    }
                }
            }
            Flow::If {
                body, alternate, ..
            } => {
                let mut left_marked = marked.clone();
                let mut left_instances = instances.clone();
                let mut right_marked = marked.clone();
                let mut right_instances = instances.clone();
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut left_marked,
                    &mut left_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                walk_must_use_body(
                    result,
                    file,
                    alternate,
                    &mut right_marked,
                    &mut right_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                merge_equal_bindings(&mut left_marked, &right_marked);
                merge_equal_bindings(&mut left_instances, &right_instances);
                *marked = left_marked;
                *instances = left_instances;
            }
            Flow::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                let mut outcomes = Vec::new();
                let mut body_marked = marked.clone();
                let mut body_instances = instances.clone();
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut body_marked,
                    &mut body_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                outcomes.push((body_marked, body_instances));
                for handler in handlers {
                    let mut handler_marked = marked.clone();
                    let mut handler_instances = instances.clone();
                    walk_must_use_body(
                        result,
                        file,
                        &handler.body,
                        &mut handler_marked,
                        &mut handler_instances,
                        imports,
                        known_classes,
                        class_scope,
                        loop_depth,
                    );
                    outcomes.push((handler_marked, handler_instances));
                }
                let (mut merged_marked, mut merged_instances) = outcomes.remove(0);
                for (candidate_marked, candidate_instances) in outcomes {
                    merge_equal_bindings(&mut merged_marked, &candidate_marked);
                    merge_equal_bindings(&mut merged_instances, &candidate_instances);
                }
                if !alternate.is_empty() {
                    walk_must_use_body(
                        result,
                        file,
                        alternate,
                        &mut merged_marked,
                        &mut merged_instances,
                        imports,
                        known_classes,
                        class_scope,
                        loop_depth,
                    );
                }
                walk_must_use_body(
                    result,
                    file,
                    finalbody,
                    &mut merged_marked,
                    &mut merged_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                *marked = merged_marked;
                *instances = merged_instances;
            }
            Flow::Loop {
                target,
                body,
                alternate,
                ..
            } => {
                let mut loop_marked = marked.clone();
                let mut loop_instances = instances.clone();
                if let Some(name) = target {
                    loop_marked.remove(name);
                    loop_instances.remove(name);
                }
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut loop_marked,
                    &mut loop_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth + 1,
                );
                merge_equal_bindings(marked, &loop_marked);
                merge_equal_bindings(instances, &loop_instances);
                walk_must_use_body(
                    result,
                    file,
                    alternate,
                    marked,
                    instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
            }
            Flow::With { items, body, .. } => {
                for item in items {
                    if let Some(name) = &item.target {
                        marked.remove(name);
                        instances.remove(name);
                    }
                    if item.unsupported_target {
                        marked.clear();
                        instances.clear();
                        unsupported(
                            result,
                            file,
                            item.range,
                            "must-use binding through a destructuring context-manager target is unsupported",
                        );
                    }
                }
                walk_must_use_body(
                    result,
                    file,
                    body,
                    marked,
                    instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
            }
            Flow::BindFunction { name, .. } => {
                marked.remove(name);
                instances.remove(name);
            }
            _ => {}
        }
        if must_use_statement_terminates(statement, loop_depth) {
            break;
        }
    }
}

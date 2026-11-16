use super::{
    annotations::{canonical, expression_path},
    must_use::{function_is_must_use, resolve_marked_callable},
    ExportedView, FileState, MustUseCallable,
};
use crate::{frontend::*, Diagnostic, Inspection, SourceFile, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn report_review_hints(
    result: &mut Inspection,
    file: &SourceFile,
    declarations: &[Declaration],
    state: &FileState,
) {
    report_closing_resource_hints(result, file, declarations, state);
    report_must_use_forwarding_hints(result, file, declarations, state);
}

#[derive(Clone)]
struct LocalCloseClass {
    name_range: SourceRange,
    close_range: SourceRange,
}

fn report_closing_resource_hints(
    result: &mut Inspection,
    file: &SourceFile,
    declarations: &[Declaration],
    state: &FileState,
) {
    let mut candidates = BTreeMap::<String, LocalCloseClass>::new();
    for declaration in declarations {
        let Declaration::Class {
            name,
            methods,
            unsupported,
            range,
            ..
        } = declaration
        else {
            continue;
        };
        if *unsupported || !state.known_classes.contains(name) {
            continue;
        }
        let Some(ExportedView {
            closed_by: None, ..
        }) = state.exports.get(name)
        else {
            continue;
        };
        let Some(close_method) = methods.iter().find(|method| {
            method.name == "close"
                && !method.is_async
                && !method.is_generator
                && method.decorators.is_empty()
                && !flow_has_unsupported(&method.body)
        }) else {
            continue;
        };
        candidates.insert(
            name.clone(),
            LocalCloseClass {
                name_range: declaration_name_range(file, *range, "class", name),
                close_range: close_method.range,
            },
        );
    }
    if candidates.is_empty() {
        return;
    }

    let mut uses = BTreeMap::<String, Vec<Span>>::new();
    let mut visit = |expression: &AstExpr| {
        let AstExprKind::Call {
            function,
            arguments,
            keywords,
        } = &expression.kind
        else {
            return;
        };
        let identity = canonical(function, &state.imports);
        if !matches!(
            identity.as_deref(),
            Some("external:contextlib.closing" | "external:contextlib::closing")
        ) || !keywords.is_empty()
            || arguments.len() != 1
        {
            return;
        }
        let AstExprKind::Call {
            function: constructor,
            ..
        } = &arguments[0].kind
        else {
            return;
        };
        let Some(class) = expression_path(constructor).filter(|name| !name.contains('.')) else {
            return;
        };
        if candidates.contains_key(&class) {
            uses.entry(class).or_default().push(crate::source_span(
                &file.path,
                &file.source,
                expression.range,
            ));
        }
    };
    for declaration in declarations {
        match declaration {
            Declaration::Function { body, .. } => visit_flow(body, &mut visit),
            Declaration::Class { methods, .. } => {
                for method in methods {
                    visit_flow(&method.body, &mut visit);
                }
            }
            Declaration::Assignment {
                value: Some(value), ..
            }
            | Declaration::Expression { value, .. } => visit_expression(value, &mut visit),
            _ => {}
        }
    }

    for (class, mut call_sites) in uses {
        let Some(class_decl) = candidates.get(&class) else {
            continue;
        };
        call_sites.sort_by(span_order);
        call_sites.dedup();
        let mut related = vec![crate::source_span(
            &file.path,
            &file.source,
            class_decl.close_range,
        )];
        related.extend(call_sites);
        related.sort_by(span_order);
        related.dedup();
        result.diagnostics.push(Diagnostic {
            semantic_identity: vec![format!("review-hint:resource-closing:{}.{}", file.module, class)],
            code: "TP6001".into(),
            severity: "info".into(),
            message: format!(
                "consider @resource(closed_by=\"close\") for {class}; contextlib.closing calls this method"
            ),
            primary: crate::source_span(&file.path, &file.source, class_decl.name_range),
            related,
            certainty: "definite".into(),
        });
    }
}

fn report_must_use_forwarding_hints(
    result: &mut Inspection,
    file: &SourceFile,
    declarations: &[Declaration],
    state: &FileState,
) {
    for declaration in declarations {
        match declaration {
            Declaration::Function {
                name,
                parameters,
                returns,
                decorators,
                body,
                is_async,
                is_generator,
                range,
                ..
            } => report_forwarding_hint(
                result,
                file,
                name,
                *range,
                body,
                parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect(),
                decorators,
                returns.as_ref(),
                *is_async,
                *is_generator,
                &state.imports,
                &state.must_use_callables,
                None,
            ),
            Declaration::Class { name, methods, .. } => {
                for method in methods {
                    let is_static = method.decorators.iter().any(|decorator| {
                        canonical(decorator, &state.imports).as_deref()
                            == Some("builtins::staticmethod")
                    });
                    report_forwarding_hint(
                        result,
                        file,
                        &format!("{name}.{}", method.name),
                        method.range,
                        &method.body,
                        method.parameters.clone(),
                        &method.decorators,
                        method.returns.as_ref(),
                        method.is_async,
                        method.is_generator,
                        &state.imports,
                        &state.must_use_callables,
                        Some((
                            name,
                            (!is_static)
                                .then(|| method.parameters.first())
                                .flatten()
                                .map(String::as_str),
                        )),
                    );
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn report_forwarding_hint(
    result: &mut Inspection,
    file: &SourceFile,
    wrapper: &str,
    range: SourceRange,
    body: &[FlowStatement],
    parameters: Vec<String>,
    decorators: &[AstExpr],
    returns: Option<&AstExpr>,
    is_async: bool,
    is_generator: bool,
    imports: &BTreeMap<String, String>,
    module_marked: &BTreeMap<String, MustUseCallable>,
    class_scope: Option<(&str, Option<&str>)>,
) {
    if is_async
        || is_generator
        || !decorators.is_empty()
        || function_is_must_use(decorators, returns, imports)
    {
        return;
    }
    let mut locals = parameters.into_iter().collect::<BTreeSet<_>>();
    collect_locals(body, &mut locals);
    let mut marked = module_marked.clone();
    let mut local_imports = imports.clone();
    for name in locals {
        marked.remove(&name);
        local_imports.remove(&name);
    }

    let mut returns = Vec::<Option<&AstExpr>>::new();
    collect_returns(body, &mut returns);
    if returns.is_empty() {
        return;
    }
    let mut call_sites = Vec::<(String, Span, Span)>::new();
    for value in returns {
        let Some(AstExpr {
            range: call_range,
            kind: AstExprKind::Call { function, .. },
        }) = value
        else {
            return;
        };
        let Some(target) = expression_path(function) else {
            return;
        };
        let Some((resolved, declaration)) =
            resolve_marked_callable(&target, &marked, &BTreeMap::new(), class_scope)
        else {
            return;
        };
        call_sites.push((
            resolved,
            crate::source_span(&file.path, &file.source, *call_range),
            declaration.span,
        ));
    }
    let Some((target, _, _)) = call_sites.first() else {
        return;
    };
    if call_sites
        .iter()
        .any(|(candidate, _, _)| candidate != target)
    {
        return;
    }
    let target = target.clone();
    let mut related = call_sites
        .into_iter()
        .flat_map(|(_, call, declaration)| [call, declaration])
        .collect::<Vec<_>>();
    related.sort_by(span_order);
    related.dedup();
    result.diagnostics.push(Diagnostic {
        semantic_identity: vec![format!(
            "review-hint:must-use-forwarder:{}:{wrapper}",
            file.module
        )],
        code: "TP6002".into(),
        severity: "info".into(),
        message: format!("consider @must_use for {wrapper}; it directly returns {target}"),
        primary: crate::source_span(
            &file.path,
            &file.source,
            declaration_name_range(
                file,
                range,
                "def",
                wrapper.rsplit('.').next().unwrap_or(wrapper),
            ),
        ),
        related,
        certainty: "definite".into(),
    });
}

fn collect_returns<'a>(statements: &'a [FlowStatement], output: &mut Vec<Option<&'a AstExpr>>) {
    for statement in statements {
        match statement {
            FlowStatement::Return { value, .. } => output.push(value.as_ref()),
            FlowStatement::If {
                body, alternate, ..
            }
            | FlowStatement::Loop {
                body, alternate, ..
            } => {
                collect_returns(body, output);
                collect_returns(alternate, output);
            }
            FlowStatement::Match { cases, .. } => {
                for case in cases {
                    collect_returns(&case.body, output);
                }
            }
            FlowStatement::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                collect_returns(body, output);
                for handler in handlers {
                    collect_returns(&handler.body, output);
                }
                collect_returns(alternate, output);
                collect_returns(finalbody, output);
            }
            FlowStatement::With { body, .. } => collect_returns(body, output),
            _ => {}
        }
    }
}

fn collect_locals(statements: &[FlowStatement], names: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            FlowStatement::Assign { name, .. }
            | FlowStatement::EscapingAssign { name, .. }
            | FlowStatement::BindFunction { name, .. } => {
                names.insert(name.clone());
            }
            FlowStatement::AnnotatedDeclaration {
                name: Some(name), ..
            } => {
                names.insert(name.clone());
            }
            FlowStatement::If {
                body, alternate, ..
            }
            | FlowStatement::Loop {
                body, alternate, ..
            } => {
                collect_locals(body, names);
                collect_locals(alternate, names);
            }
            FlowStatement::Match { cases, .. } => {
                for case in cases {
                    case.pattern.binding_names(names);
                    collect_locals(&case.body, names);
                }
            }
            FlowStatement::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                collect_locals(body, names);
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        names.insert(name.clone());
                    }
                    collect_locals(&handler.body, names);
                }
                collect_locals(alternate, names);
                collect_locals(finalbody, names);
            }
            FlowStatement::With { items, body, .. } => {
                for item in items {
                    if let Some(name) = &item.target {
                        names.insert(name.clone());
                    }
                }
                collect_locals(body, names);
            }
            FlowStatement::DeleteLocal { names: deleted, .. } => {
                names.extend(deleted.iter().cloned());
            }
            _ => {}
        }
    }
}

fn visit_flow(statements: &[FlowStatement], visitor: &mut impl FnMut(&AstExpr)) {
    for statement in statements {
        match statement {
            FlowStatement::Assign {
                annotation, value, ..
            }
            | FlowStatement::EscapingAssign {
                annotation, value, ..
            } => {
                if let Some(annotation) = annotation {
                    visit_expression(annotation, visitor);
                }
                visit_expression(value, visitor);
            }
            FlowStatement::AnnotatedDeclaration { annotation, .. } => {
                visit_expression(annotation, visitor)
            }
            FlowStatement::Expression { value, .. } => visit_expression(value, visitor),
            FlowStatement::If {
                test,
                body,
                alternate,
                ..
            } => {
                visit_expression(test, visitor);
                visit_flow(body, visitor);
                visit_flow(alternate, visitor);
            }
            FlowStatement::Match { subject, cases, .. } => {
                visit_expression(subject, visitor);
                for case in cases {
                    if let Some(guard) = &case.guard {
                        visit_expression(guard, visitor);
                    }
                    visit_flow(&case.body, visitor);
                }
            }
            FlowStatement::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                visit_flow(body, visitor);
                for handler in handlers {
                    if let Some(exception) = &handler.exception {
                        visit_expression(exception, visitor);
                    }
                    visit_flow(&handler.body, visitor);
                }
                visit_flow(alternate, visitor);
                visit_flow(finalbody, visitor);
            }
            FlowStatement::Loop {
                test,
                iterator,
                body,
                alternate,
                ..
            } => {
                if let Some(test) = test {
                    visit_expression(test, visitor);
                }
                if let Some(iterator) = iterator {
                    visit_expression(iterator, visitor);
                }
                visit_flow(body, visitor);
                visit_flow(alternate, visitor);
            }
            FlowStatement::With { items, body, .. } => {
                for item in items {
                    visit_expression(&item.context, visitor);
                }
                visit_flow(body, visitor);
            }
            FlowStatement::Return {
                value: Some(value), ..
            } => visit_expression(value, visitor),
            FlowStatement::Raise {
                exception, cause, ..
            } => {
                if let Some(exception) = exception {
                    visit_expression(exception, visitor);
                }
                if let Some(cause) = cause {
                    visit_expression(cause, visitor);
                }
            }
            FlowStatement::Store { value, target, .. } => {
                visit_expression(value, visitor);
                if let Some(target) = target {
                    visit_expression(target, visitor);
                }
            }
            _ => {}
        }
    }
}

fn visit_expression(expression: &AstExpr, visitor: &mut impl FnMut(&AstExpr)) {
    visitor(expression);
    match &expression.kind {
        AstExprKind::Attribute(value, _)
        | AstExprKind::Await(value)
        | AstExprKind::Starred(value) => visit_expression(value, visitor),
        AstExprKind::Subscript(value, arguments) => {
            visit_expression(value, visitor);
            for argument in arguments {
                visit_expression(argument, visitor);
            }
        }
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            visit_expression(function, visitor);
            for argument in arguments {
                visit_expression(argument, visitor);
            }
            for (_, value) in keywords {
                visit_expression(value, visitor);
            }
        }
        AstExprKind::Tuple(values) | AstExprKind::Collection(values) => {
            for value in values {
                visit_expression(value, visitor);
            }
        }
        AstExprKind::Yield(Some(value)) => visit_expression(value, visitor),
        AstExprKind::Name(_)
        | AstExprKind::Yield(None)
        | AstExprKind::Closure(_)
        | AstExprKind::OtherCalls(_, _)
        | AstExprKind::Boolean(_)
        | AstExprKind::String(_)
        | AstExprKind::Primitive
        | AstExprKind::NoneLiteral
        | AstExprKind::Other
        | AstExprKind::DepthLimit => {}
    }
}

fn flow_has_unsupported(statements: &[FlowStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        FlowStatement::Unsupported { .. } => true,
        FlowStatement::If {
            body, alternate, ..
        }
        | FlowStatement::Loop {
            body, alternate, ..
        } => flow_has_unsupported(body) || flow_has_unsupported(alternate),
        FlowStatement::Match { cases, .. } => {
            cases.iter().any(|case| flow_has_unsupported(&case.body))
        }
        FlowStatement::Try {
            body,
            handlers,
            alternate,
            finalbody,
            ..
        } => {
            flow_has_unsupported(body)
                || handlers
                    .iter()
                    .any(|handler| flow_has_unsupported(&handler.body))
                || flow_has_unsupported(alternate)
                || flow_has_unsupported(finalbody)
        }
        FlowStatement::With { body, .. } => flow_has_unsupported(body),
        _ => false,
    })
}

fn declaration_name_range(
    file: &SourceFile,
    range: SourceRange,
    keyword: &str,
    name: &str,
) -> SourceRange {
    let Some(source) = file.source.get(range.start..range.end) else {
        return range;
    };
    let declaration = format!("{keyword} {name}");
    source
        .find(&declaration)
        .map(|offset| {
            let start = range.start + offset + keyword.len() + 1;
            SourceRange {
                start,
                end: start + name.len(),
            }
        })
        .unwrap_or(range)
}

fn span_order(left: &Span, right: &Span) -> std::cmp::Ordering {
    (
        &left.file,
        left.line,
        left.column,
        left.end_line,
        left.end_column,
    )
        .cmp(&(
            &right.file,
            right.line,
            right.column,
            right.end_line,
            right.end_column,
        ))
}

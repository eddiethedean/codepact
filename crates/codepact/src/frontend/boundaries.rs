use super::*;

fn function_header_requires_boundary(
    function: &ruff_python_ast::StmtFunctionDef,
    safe_metadata_calls: &std::collections::BTreeSet<String>,
    safe_annotation_subscripts: &std::collections::BTreeSet<String>,
    annotated_subscripts: &std::collections::BTreeSet<String>,
    must_use_names: &std::collections::BTreeSet<String>,
) -> bool {
    (!function.decorator_list.is_empty()
        && !decorators_are_only_must_use(&function.decorator_list, must_use_names))
        || parameters_have_effectful_defaults(&function.parameters)
        || function.parameters.iter_source_order().any(|parameter| {
            parameter.annotation().is_some_and(|expr| {
                expression_has_unknown_call(expr, safe_metadata_calls)
                    || expression_has_unsafe_annotation_evaluation(
                        expr,
                        safe_metadata_calls,
                        safe_annotation_subscripts,
                        annotated_subscripts,
                    )
            })
        })
        || function.returns.as_deref().is_some_and(|expr| {
            expression_has_unknown_call(expr, safe_metadata_calls)
                || expression_has_unsafe_annotation_evaluation(
                    expr,
                    safe_metadata_calls,
                    safe_annotation_subscripts,
                    annotated_subscripts,
                )
        })
}

fn class_header_requires_boundary(
    class: &ruff_python_ast::StmtClassDef,
    resource_names: &std::collections::BTreeSet<String>,
) -> bool {
    class.decorator_list.iter().any(|decorator| {
        let Expr::Call(call) = &decorator.expression else {
            return true;
        };
        let Some(name) = dotted_name(&call.func) else {
            return true;
        };
        !resource_names.contains(&name)
            || !call.arguments.args.is_empty()
            || call.arguments.keywords.len() != 1
            || call.arguments.keywords[0]
                .arg
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                != Some("closed_by")
            || !matches!(call.arguments.keywords[0].value, Expr::StringLiteral(_))
    }) || class
        .arguments
        .as_ref()
        .is_some_and(|arguments| !arguments.args.is_empty() || !arguments.keywords.is_empty())
}

fn has_known_resource_decorator(
    class: &ruff_python_ast::StmtClassDef,
    resource_names: &std::collections::BTreeSet<String>,
) -> bool {
    class.decorator_list.iter().any(|decorator| {
        let Expr::Call(call) = &decorator.expression else {
            return false;
        };
        let Some(name) = dotted_name(&call.func) else {
            return false;
        };
        resource_names.contains(&name)
            && call.arguments.args.is_empty()
            && call.arguments.keywords.len() == 1
            && call.arguments.keywords[0]
                .arg
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                == Some("closed_by")
            && matches!(call.arguments.keywords[0].value, Expr::StringLiteral(_))
    })
}

fn expression_has_unknown_call(
    expression: &Expr,
    safe_calls: &std::collections::BTreeSet<String>,
) -> bool {
    struct CallVisitor<'a> {
        found: bool,
        exceeded: bool,
        depth: usize,
        nodes: usize,
        safe_calls: &'a std::collections::BTreeSet<String>,
    }
    impl<'a> Visitor<'a> for CallVisitor<'a> {
        fn visit_expr(&mut self, expression: &'a Expr) {
            if self.found || self.exceeded {
                return;
            }
            if self.depth >= ExpressionBudget::MAX_DEPTH
                || self.nodes >= ExpressionBudget::MAX_NODES
            {
                self.exceeded = true;
                return;
            }
            self.nodes += 1;
            if let Expr::Call(call) = expression {
                let name = dotted_name(&call.func);
                if !name.is_some_and(|name| self.safe_calls.contains(&name)) {
                    self.found = true;
                }
            }
            if !self.found {
                self.depth += 1;
                walk_expr(self, expression);
                self.depth -= 1;
            }
        }
    }
    let mut visitor = CallVisitor {
        found: false,
        exceeded: false,
        depth: 0,
        nodes: 0,
        safe_calls,
    };
    visitor.visit_expr(expression);
    visitor.found || visitor.exceeded
}

fn expression_has_unsafe_annotation_evaluation(
    expression: &Expr,
    safe_calls: &std::collections::BTreeSet<String>,
    safe_subscripts: &std::collections::BTreeSet<String>,
    annotated_subscripts: &std::collections::BTreeSet<String>,
) -> bool {
    fn safe_metadata_value(
        expression: &Expr,
        safe_calls: &std::collections::BTreeSet<String>,
    ) -> bool {
        let mut pending = vec![(expression, 0usize)];
        let mut nodes = 0usize;
        while let Some((expression, depth)) = pending.pop() {
            if depth >= ExpressionBudget::MAX_DEPTH || nodes >= ExpressionBudget::MAX_NODES {
                return false;
            }
            nodes += 1;
            match expression {
                Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_) => {}
                Expr::Tuple(tuple) => {
                    pending.extend(tuple.elts.iter().map(|value| (value, depth + 1)))
                }
                Expr::Call(call)
                    if dotted_name(&call.func).is_some_and(|name| safe_calls.contains(&name)) =>
                {
                    pending.extend(call.arguments.args.iter().map(|value| (value, depth + 1)));
                    pending.extend(
                        call.arguments
                            .keywords
                            .iter()
                            .map(|keyword| (&keyword.value, depth + 1)),
                    );
                }
                _ => return false,
            }
        }
        true
    }

    struct SubscriptVisitor<'a> {
        found: bool,
        exceeded: bool,
        depth: usize,
        nodes: usize,
        safe_calls: &'a std::collections::BTreeSet<String>,
        safe_subscripts: &'a std::collections::BTreeSet<String>,
        annotated_subscripts: &'a std::collections::BTreeSet<String>,
    }
    impl<'a> Visitor<'a> for SubscriptVisitor<'a> {
        fn visit_expr(&mut self, expression: &'a Expr) {
            if self.found || self.exceeded {
                return;
            }
            if self.depth >= ExpressionBudget::MAX_DEPTH
                || self.nodes >= ExpressionBudget::MAX_NODES
            {
                self.exceeded = true;
                return;
            }
            self.nodes += 1;
            let safe_path = |name: String| {
                self.safe_subscripts.contains(&name)
                    || self.safe_subscripts.iter().any(|prefix| {
                        prefix
                            .strip_suffix(".*")
                            .is_some_and(|prefix| name.starts_with(&format!("{prefix}.")))
                    })
            };
            let recognized = match expression {
                Expr::Name(_)
                | Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_) => true,
                Expr::Attribute(_) => dotted_name(expression).is_some_and(safe_path),
                Expr::Subscript(subscript) => {
                    let name = dotted_name(&subscript.value);
                    let is_safe_type = name.clone().is_some_and(safe_path);
                    let is_annotated = name
                        .as_ref()
                        .is_some_and(|name| self.annotated_subscripts.contains(name));
                    let metadata_is_safe = if is_annotated {
                        let arguments = match &*subscript.slice {
                            Expr::Tuple(tuple) => tuple.elts.as_slice(),
                            expression => std::slice::from_ref(expression),
                        };
                        arguments
                            .iter()
                            .skip(1)
                            .all(|metadata| safe_metadata_value(metadata, self.safe_calls))
                    } else {
                        true
                    };
                    is_safe_type && metadata_is_safe
                }
                Expr::Call(call) => {
                    dotted_name(&call.func).is_some_and(|name| self.safe_calls.contains(&name))
                }
                Expr::Tuple(_) | Expr::List(_) => true,
                _ => false,
            };
            if !recognized {
                self.found = true;
            }
            if !self.found {
                self.depth += 1;
                walk_expr(self, expression);
                self.depth -= 1;
            }
        }
    }
    let mut visitor = SubscriptVisitor {
        found: false,
        exceeded: false,
        depth: 0,
        nodes: 0,
        safe_calls,
        safe_subscripts,
        annotated_subscripts,
    };
    visitor.visit_expr(expression);
    visitor.found || visitor.exceeded
}

pub(super) fn dotted_name(expression: &Expr) -> Option<String> {
    let mut components = Vec::new();
    let mut current = expression;
    loop {
        match current {
            Expr::Name(name) => {
                components.push(name.id.to_string());
                break;
            }
            Expr::Attribute(attribute) => {
                components.push(attribute.attr.to_string());
                current = &attribute.value;
            }
            _ => return None,
        }
    }
    components.reverse();
    Some(components.join("."))
}

pub(super) fn clear_imported_names(names: &mut std::collections::BTreeSet<String>, local: &str) {
    names.retain(|name| {
        name != local
            && !name
                .strip_prefix(local)
                .is_some_and(|suffix| suffix.starts_with('.'))
    });
}

pub(super) fn assignment_roots(target: &Expr, names: &mut std::collections::BTreeSet<String>) {
    let mut pending = vec![target];
    while let Some(target) = pending.pop() {
        match target {
            Expr::Name(name) => {
                names.insert(name.id.to_string());
            }
            Expr::Attribute(attribute) => pending.push(&attribute.value),
            Expr::Subscript(subscript) => pending.push(&subscript.value),
            Expr::Tuple(tuple) => pending.extend(tuple.elts.iter()),
            Expr::List(list) => pending.extend(list.elts.iter()),
            _ => {}
        }
    }
}

pub(super) fn contains_yield(statements: &[Stmt]) -> bool {
    #[derive(Default)]
    struct YieldVisitor {
        found: bool,
    }
    impl<'a> Visitor<'a> for YieldVisitor {
        fn visit_stmt(&mut self, statement: &'a Stmt) {
            if !matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                walk_stmt(self, statement);
            }
        }
        fn visit_expr(&mut self, expression: &'a Expr) {
            match expression {
                Expr::Yield(_) | Expr::YieldFrom(_) => self.found = true,
                Expr::Lambda(_) => {}
                _ => walk_expr(self, expression),
            }
        }
    }
    let mut visitor = YieldVisitor::default();
    for statement in statements {
        visitor.visit_stmt(statement);
    }
    visitor.found
}

pub(super) fn definition_header_boundaries(statements: &[Stmt]) -> Vec<bool> {
    let metadata = [
        "Resource",
        "SharedBorrow",
        "OwnedAccess",
        "ExclusiveBorrow",
        "MustUseResult",
    ];
    let mut metadata_names = std::collections::BTreeSet::new();
    let mut resource_names = std::collections::BTreeSet::new();
    let mut safe_annotation_subscripts = std::collections::BTreeSet::from([
        "list".to_owned(),
        "dict".to_owned(),
        "tuple".to_owned(),
        "set".to_owned(),
        "frozenset".to_owned(),
    ]);
    let mut annotated_subscripts = std::collections::BTreeSet::new();
    let mut must_use_names = std::collections::BTreeSet::new();
    let mut known_resource_classes = std::collections::BTreeSet::new();
    let mut boundaries = Vec::with_capacity(statements.len());
    for statement in statements {
        let boundary = match statement {
            Stmt::FunctionDef(function) => function_header_requires_boundary(
                function,
                &metadata_names,
                &safe_annotation_subscripts,
                &annotated_subscripts,
                &must_use_names,
            ),
            Stmt::ClassDef(class) => {
                class_body_requires_boundary(
                    class,
                    &metadata_names,
                    &safe_annotation_subscripts,
                    &annotated_subscripts,
                    &must_use_names,
                ) || class_header_requires_boundary(class, &resource_names)
            }
            _ => false,
        };
        boundaries.push(boundary);
        if boundary {
            metadata_names.clear();
            resource_names.clear();
            safe_annotation_subscripts.clear();
            annotated_subscripts.clear();
            known_resource_classes.clear();
            must_use_names.clear();
        }
        match statement {
            Stmt::ImportFrom(import) => {
                let module = import.module.as_ref().map(ToString::to_string);
                if import.level != 0
                    || import
                        .names
                        .iter()
                        .any(|alias| alias.name.to_string() == "*")
                {
                    metadata_names.clear();
                    resource_names.clear();
                    safe_annotation_subscripts.clear();
                    annotated_subscripts.clear();
                    must_use_names.clear();
                    known_resource_classes.clear();
                    must_use_names.clear();
                }
                for alias in &import.names {
                    let source = alias.name.to_string();
                    let local = alias
                        .asname
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| source.clone());
                    clear_imported_names(&mut metadata_names, &local);
                    clear_imported_names(&mut resource_names, &local);
                    clear_imported_names(&mut safe_annotation_subscripts, &local);
                    clear_imported_names(&mut annotated_subscripts, &local);
                    clear_imported_names(&mut must_use_names, &local);
                    if import.level != 0 {
                        continue;
                    }
                    match module.as_deref() {
                        Some("codepact.contracts") if metadata.contains(&source.as_str()) => {
                            metadata_names.insert(local.clone());
                        }
                        Some("codepact") if source == "Resource" => {
                            metadata_names.insert(local.clone());
                        }
                        Some("codepact") if source == "contracts" => {
                            for name in metadata {
                                metadata_names.insert(format!("{local}.{name}"));
                            }
                        }
                        _ => {}
                    }
                    if matches!(
                        module.as_deref(),
                        Some(
                            "typing" | "typing_extensions" | "codepact.rust" | "codepact.contracts"
                        )
                    ) {
                        safe_annotation_subscripts.insert(local.clone());
                    } else if module.as_deref() == Some("codepact")
                        && matches!(source.as_str(), "rust" | "contracts")
                    {
                        safe_annotation_subscripts.insert(format!("{local}.*"));
                    }
                    if matches!(module.as_deref(), Some("typing" | "typing_extensions"))
                        && source == "Annotated"
                    {
                        annotated_subscripts.insert(local.clone());
                    }
                    if module.as_deref() == Some("codepact") && source == "must_use" {
                        must_use_names.insert(local.clone());
                    }
                    if import.level == 0
                        && module.as_deref() == Some("codepact")
                        && source == "resource"
                    {
                        resource_names.insert(local);
                    } else if import.level == 0 && source != "*" {
                        known_resource_classes.insert(local);
                    }
                }
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let source = alias.name.to_string();
                    let local = alias
                        .asname
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| source.split('.').next().unwrap_or_default().to_owned());
                    clear_imported_names(&mut metadata_names, &local);
                    clear_imported_names(&mut resource_names, &local);
                    clear_imported_names(&mut safe_annotation_subscripts, &local);
                    clear_imported_names(&mut annotated_subscripts, &local);
                    clear_imported_names(&mut must_use_names, &local);
                    if source == "codepact.contracts" {
                        let prefix = alias
                            .asname
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "codepact.contracts".to_owned());
                        for name in metadata {
                            metadata_names.insert(format!("{prefix}.{name}"));
                        }
                    } else if source == "codepact" {
                        metadata_names.insert(format!("{local}.Resource"));
                        resource_names.insert(format!("{local}.resource"));
                    }
                    known_resource_classes.remove(&local);
                    if matches!(
                        source.as_str(),
                        "typing"
                            | "typing_extensions"
                            | "codepact.rust"
                            | "codepact.contracts"
                            | "codepact"
                    ) {
                        safe_annotation_subscripts.insert(format!("{local}.*"));
                    }
                    if matches!(source.as_str(), "typing" | "typing_extensions") {
                        annotated_subscripts.insert(format!("{local}.Annotated"));
                    }
                    if source == "codepact" {
                        must_use_names.insert(format!("{local}.must_use"));
                    }
                }
            }
            Stmt::FunctionDef(function) => {
                clear_imported_names(&mut metadata_names, function.name.as_ref());
                clear_imported_names(&mut resource_names, function.name.as_ref());
                clear_imported_names(&mut safe_annotation_subscripts, function.name.as_ref());
                clear_imported_names(&mut annotated_subscripts, function.name.as_ref());
                clear_imported_names(&mut must_use_names, function.name.as_ref());
                known_resource_classes.remove(function.name.as_ref());
            }
            Stmt::ClassDef(class) => {
                clear_imported_names(&mut metadata_names, class.name.as_ref());
                clear_imported_names(&mut resource_names, class.name.as_ref());
                clear_imported_names(&mut safe_annotation_subscripts, class.name.as_ref());
                clear_imported_names(&mut annotated_subscripts, class.name.as_ref());
                clear_imported_names(&mut must_use_names, class.name.as_ref());
                known_resource_classes.remove(class.name.as_ref());
                if !boundary && has_known_resource_decorator(class, &resource_names) {
                    known_resource_classes.insert(class.name.to_string());
                }
            }
            Stmt::Assign(assign) => {
                let mut rebound = std::collections::BTreeSet::new();
                for target in &assign.targets {
                    assignment_roots(target, &mut rebound);
                }
                let resource_class_alias = matches!(
                    &*assign.value,
                    Expr::Name(source) if known_resource_classes.contains(source.id.as_str())
                ) || (matches!(&*assign.value, Expr::Subscript(_))
                    && !expression_has_unknown_call(&assign.value, &metadata_names)
                    && !expression_has_unsafe_annotation_evaluation(
                        &assign.value,
                        &metadata_names,
                        &safe_annotation_subscripts,
                        &annotated_subscripts,
                    ));
                let mut safe_calls = metadata_names.clone();
                safe_calls.extend(known_resource_classes.iter().cloned());
                for name in rebound {
                    clear_imported_names(&mut metadata_names, &name);
                    clear_imported_names(&mut resource_names, &name);
                    clear_imported_names(&mut safe_annotation_subscripts, &name);
                    clear_imported_names(&mut annotated_subscripts, &name);
                    clear_imported_names(&mut must_use_names, &name);
                    known_resource_classes.remove(&name);
                }
                if resource_class_alias {
                    let mut names = std::collections::BTreeSet::new();
                    for target in &assign.targets {
                        assignment_roots(target, &mut names);
                    }
                    known_resource_classes.extend(names);
                }
                if expression_has_unknown_call(&assign.value, &safe_calls) {
                    metadata_names.clear();
                    resource_names.clear();
                    safe_annotation_subscripts.clear();
                    annotated_subscripts.clear();
                    must_use_names.clear();
                    known_resource_classes.clear();
                }
            }
            Stmt::AnnAssign(assign) => {
                let mut rebound = std::collections::BTreeSet::new();
                assignment_roots(&assign.target, &mut rebound);
                let resource_class_alias = assign.value.as_deref().and_then(|value| match value {
                    Expr::Name(source) if known_resource_classes.contains(source.id.as_str()) => {
                        Some(source.id.to_string())
                    }
                    Expr::Subscript(_)
                        if !expression_has_unknown_call(value, &metadata_names)
                            && !expression_has_unsafe_annotation_evaluation(
                                value,
                                &metadata_names,
                                &safe_annotation_subscripts,
                                &annotated_subscripts,
                            ) =>
                    {
                        Some(String::new())
                    }
                    _ => None,
                });
                let mut safe_calls = metadata_names.clone();
                safe_calls.extend(known_resource_classes.iter().cloned());
                for name in rebound {
                    clear_imported_names(&mut metadata_names, &name);
                    clear_imported_names(&mut resource_names, &name);
                    clear_imported_names(&mut safe_annotation_subscripts, &name);
                    clear_imported_names(&mut annotated_subscripts, &name);
                    clear_imported_names(&mut must_use_names, &name);
                    known_resource_classes.remove(&name);
                }
                if resource_class_alias.is_some() {
                    let mut names = std::collections::BTreeSet::new();
                    assignment_roots(&assign.target, &mut names);
                    known_resource_classes.extend(names);
                }
                if assign.value.as_deref().is_none_or(|value| {
                    expression_has_unknown_call(value, &safe_calls)
                        || expression_has_unknown_call(&assign.annotation, &metadata_names)
                }) {
                    metadata_names.clear();
                    resource_names.clear();
                    safe_annotation_subscripts.clear();
                    annotated_subscripts.clear();
                    must_use_names.clear();
                    known_resource_classes.clear();
                }
            }
            Stmt::Delete(delete) => {
                let mut rebound = std::collections::BTreeSet::new();
                for target in &delete.targets {
                    assignment_roots(target, &mut rebound);
                }
                for name in rebound {
                    clear_imported_names(&mut metadata_names, &name);
                    clear_imported_names(&mut resource_names, &name);
                    clear_imported_names(&mut safe_annotation_subscripts, &name);
                    clear_imported_names(&mut annotated_subscripts, &name);
                    clear_imported_names(&mut must_use_names, &name);
                    known_resource_classes.remove(&name);
                }
            }
            Stmt::TypeAlias(_) => {
                metadata_names.clear();
                resource_names.clear();
                safe_annotation_subscripts.clear();
                annotated_subscripts.clear();
                must_use_names.clear();
                known_resource_classes.clear();
            }
            Stmt::Pass(_) => {}
            Stmt::Expr(expression)
                if matches!(expression.value.as_ref(), Expr::StringLiteral(_)) => {}
            Stmt::Expr(expression) => {
                let mut safe_calls = metadata_names.clone();
                safe_calls.extend(known_resource_classes.iter().cloned());
                if expression_has_unknown_call(&expression.value, &safe_calls) {
                    metadata_names.clear();
                    resource_names.clear();
                    safe_annotation_subscripts.clear();
                    annotated_subscripts.clear();
                    must_use_names.clear();
                    known_resource_classes.clear();
                }
            }
            _ => {
                metadata_names.clear();
                resource_names.clear();
                safe_annotation_subscripts.clear();
                annotated_subscripts.clear();
                must_use_names.clear();
                known_resource_classes.clear();
            }
        }
    }
    boundaries
}

fn default_expression_is_simple(expression: &Expr) -> bool {
    match expression {
        Expr::Name(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_) => true,
        Expr::Tuple(tuple) => tuple.elts.iter().all(default_expression_is_simple),
        _ => false,
    }
}

fn parameters_have_effectful_defaults(parameters: &ruff_python_ast::Parameters) -> bool {
    parameters.iter_source_order().any(|parameter| {
        matches!(parameter, AnyParameterRef::NonVariadic(parameter)
            if parameter.default.as_ref().is_some_and(|value| !default_expression_is_simple(value)))
    })
}

fn class_body_requires_boundary(
    class: &ruff_python_ast::StmtClassDef,
    safe_metadata_calls: &std::collections::BTreeSet<String>,
    safe_annotation_subscripts: &std::collections::BTreeSet<String>,
    annotated_subscripts: &std::collections::BTreeSet<String>,
    must_use_names: &std::collections::BTreeSet<String>,
) -> bool {
    class
        .body
        .iter()
        .enumerate()
        .any(|(index, statement)| match statement {
            Stmt::FunctionDef(function) => {
                parameters_have_effectful_defaults(&function.parameters)
                    || (!function.decorator_list.is_empty()
                        && !decorators_are_only_must_use(&function.decorator_list, must_use_names))
                    || function.parameters.iter_source_order().any(|parameter| {
                        parameter.annotation().is_some_and(|expr| {
                            expression_has_unknown_call(expr, safe_metadata_calls)
                                || expression_has_unsafe_annotation_evaluation(
                                    expr,
                                    safe_metadata_calls,
                                    safe_annotation_subscripts,
                                    annotated_subscripts,
                                )
                        })
                    })
                    || function.returns.as_deref().is_some_and(|expr| {
                        expression_has_unknown_call(expr, safe_metadata_calls)
                            || expression_has_unsafe_annotation_evaluation(
                                expr,
                                safe_metadata_calls,
                                safe_annotation_subscripts,
                                annotated_subscripts,
                            )
                    })
            }
            Stmt::Pass(_) => false,
            Stmt::Expr(expression)
                if index == 0 && matches!(expression.value.as_ref(), Expr::StringLiteral(_)) =>
            {
                false
            }
            _ => true,
        })
}

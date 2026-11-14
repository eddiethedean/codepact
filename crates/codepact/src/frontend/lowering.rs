use super::*;

pub(super) fn expression(node: &Expr, budget: &mut ExpressionBudget, depth: usize) -> AstExpr {
    let range = source_range(node);
    if budget.exceeded
        || depth >= ExpressionBudget::MAX_DEPTH
        || budget.nodes >= ExpressionBudget::MAX_NODES
    {
        budget.exceeded = true;
        return AstExpr {
            range,
            kind: AstExprKind::DepthLimit,
        };
    }
    budget.nodes += 1;
    let kind = match node {
        Expr::Name(name) => AstExprKind::Name(name.id.to_string()),
        Expr::Attribute(attribute) => AstExprKind::Attribute(
            Box::new(expression(&attribute.value, budget, depth + 1)),
            attribute.attr.to_string(),
        ),
        Expr::Subscript(subscript) => {
            let items = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple
                    .elts
                    .iter()
                    .map(|item| expression(item, budget, depth + 1))
                    .collect(),
                other => vec![expression(other, budget, depth + 1)],
            };
            AstExprKind::Subscript(
                Box::new(expression(&subscript.value, budget, depth + 1)),
                items,
            )
        }
        Expr::Call(call) => AstExprKind::Call {
            function: Box::new(expression(&call.func, budget, depth + 1)),
            arguments: call
                .arguments
                .args
                .iter()
                .map(|argument| expression(argument, budget, depth + 1))
                .collect(),
            keywords: call
                .arguments
                .keywords
                .iter()
                .map(|keyword| {
                    (
                        keyword.arg.as_ref().map(ToString::to_string),
                        expression(&keyword.value, budget, depth + 1),
                    )
                })
                .collect(),
        },
        Expr::Await(await_) => {
            AstExprKind::Await(Box::new(expression(&await_.value, budget, depth + 1)))
        }
        Expr::Tuple(tuple) => AstExprKind::Tuple(
            tuple
                .elts
                .iter()
                .map(|item| expression(item, budget, depth + 1))
                .collect(),
        ),
        Expr::List(list) => AstExprKind::Collection(
            list.elts
                .iter()
                .map(|item| expression(item, budget, depth + 1))
                .collect(),
        ),
        Expr::Set(set) => AstExprKind::Collection(
            set.elts
                .iter()
                .map(|item| expression(item, budget, depth + 1))
                .collect(),
        ),
        Expr::Dict(dict) => AstExprKind::Collection(
            dict.items
                .iter()
                .flat_map(|item| item.key.iter().chain(std::iter::once(&item.value)))
                .map(|item| expression(item, budget, depth + 1))
                .collect(),
        ),
        Expr::Starred(starred) => {
            AstExprKind::Starred(Box::new(expression(&starred.value, budget, depth + 1)))
        }
        Expr::Lambda(lambda) => {
            let mut references = ReferencedNames::default();
            references.visit_expr(&lambda.body);
            for parameter in lambda
                .parameters
                .iter()
                .flat_map(|parameters| parameters.iter())
            {
                references.names.remove(parameter.name().as_str());
            }
            AstExprKind::Closure(references.names)
        }
        Expr::StringLiteral(string) => AstExprKind::String(string.value.to_str().to_owned()),
        Expr::BooleanLiteral(boolean) => AstExprKind::Boolean(boolean.value),
        Expr::NumberLiteral(_) => AstExprKind::Primitive,
        Expr::NoneLiteral(_) => AstExprKind::NoneLiteral,
        _ => {
            let mut collector = NestedCallCollector {
                budget,
                calls: Vec::new(),
                depth: depth + 1,
            };
            collector.visit_expr(node);
            if collector.budget.exceeded {
                return AstExpr {
                    range,
                    kind: AstExprKind::DepthLimit,
                };
            }
            collector.calls.sort();
            collector.calls.dedup();
            let mut references = ReferencedNames::default();
            references.visit_expr(node);
            AstExprKind::OtherCalls(collector.calls, references.names)
        }
    };
    if budget.exceeded {
        return AstExpr {
            range,
            kind: AstExprKind::DepthLimit,
        };
    }
    AstExpr { range, kind }
}

pub(super) fn imports(names: &[ruff_python_ast::Alias]) -> Vec<ImportName> {
    names
        .iter()
        .map(|name| ImportName {
            name: name.name.to_string(),
            alias: name.asname.as_ref().map(ToString::to_string),
            range: source_range(name),
        })
        .collect()
}

pub(super) fn decorators(
    list: &[ruff_python_ast::Decorator],
    budget: &mut ExpressionBudget,
) -> Vec<AstExpr> {
    list.iter()
        .map(|decorator| {
            let mut expression = expression(&decorator.expression, budget, 0);
            expression.range = source_range(decorator);
            expression
        })
        .collect()
}

pub(super) fn decorators_are_only_must_use(
    list: &[ruff_python_ast::Decorator],
    must_use_names: &std::collections::BTreeSet<String>,
) -> bool {
    !list.is_empty()
        && list.iter().all(|decorator| {
            dotted_name(&decorator.expression).is_some_and(|name| must_use_names.contains(&name))
        })
}

pub(super) fn update_must_use_names(
    names: &mut std::collections::BTreeSet<String>,
    statement: &Stmt,
) {
    match statement {
        Stmt::ImportFrom(import) => {
            let module = import.module.as_ref().map(ToString::to_string);
            for alias in &import.names {
                let source = alias.name.to_string();
                let local = alias
                    .asname
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| source.clone());
                clear_imported_names(names, &local);
                if import.level == 0
                    && module.as_deref() == Some("codepact")
                    && source == "must_use"
                {
                    names.insert(local);
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
                clear_imported_names(names, &local);
                if source == "codepact" {
                    names.insert(format!("{local}.must_use"));
                }
            }
        }
        Stmt::FunctionDef(function) => clear_imported_names(names, function.name.as_ref()),
        Stmt::ClassDef(class) => clear_imported_names(names, class.name.as_ref()),
        Stmt::Assign(assign) => {
            let mut rebound = std::collections::BTreeSet::new();
            for target in &assign.targets {
                assignment_roots(target, &mut rebound);
            }
            for name in rebound {
                clear_imported_names(names, &name);
            }
        }
        Stmt::AnnAssign(assign) => {
            let mut rebound = std::collections::BTreeSet::new();
            assignment_roots(&assign.target, &mut rebound);
            for name in rebound {
                clear_imported_names(names, &name);
            }
        }
        Stmt::Delete(delete) => {
            let mut rebound = std::collections::BTreeSet::new();
            for target in &delete.targets {
                assignment_roots(target, &mut rebound);
            }
            for name in rebound {
                clear_imported_names(names, &name);
            }
        }
        Stmt::Pass(_) => {}
        Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::StringLiteral(_)) => {}
        _ => names.clear(),
    }
}

#[derive(Default)]
struct ReferencedNames {
    names: BTreeSet<String>,
    locals: BTreeSet<String>,
    globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    depth: usize,
}

impl<'a> Visitor<'a> for ReferencedNames {
    fn visit_expr(&mut self, node: &'a Expr) {
        if self.depth >= 128 {
            return;
        }
        if let Expr::Name(name) = node {
            if name.ctx == ruff_python_ast::ExprContext::Load {
                self.names.insert(name.id.to_string());
            } else {
                self.locals.insert(name.id.to_string());
            }
            return;
        }
        self.depth += 1;
        walk_expr(self, node);
        self.depth -= 1;
    }

    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.depth >= 128 {
            return;
        }
        match statement {
            Stmt::Global(declaration) => self
                .globals
                .extend(declaration.names.iter().map(ToString::to_string)),
            Stmt::Nonlocal(declaration) => self
                .nonlocals
                .extend(declaration.names.iter().map(ToString::to_string)),
            Stmt::FunctionDef(definition) => {
                self.locals.insert(definition.name.to_string());
                // Nested definitions may reference surrounding bindings.
                // Do not count their own stores as bindings of this scope.
                let mut nested = Self::default();
                for statement in &definition.body {
                    if self.depth + 1 < 128 {
                        nested.depth = self.depth + 1;
                        nested.visit_stmt(statement);
                    }
                }
                nested.finish();
                self.names.extend(nested.names);
                return;
            }
            _ => {}
        }
        self.depth += 1;
        walk_stmt(self, statement);
        self.depth -= 1;
    }
}

impl ReferencedNames {
    fn finish(&mut self) {
        self.names.retain(|name| {
            !self.globals.contains(name)
                && (!self.locals.contains(name) || self.nonlocals.contains(name))
        });
    }
}

pub(super) fn flow_statements(
    statements: &[Stmt],
    budget: &mut ExpressionBudget,
) -> Vec<FlowStatement> {
    let mut visitor = ScopeEscapeVisitor::default();
    for statement in statements {
        visitor.visit_stmt(statement);
    }
    flow_statements_at(statements, budget, &visitor.names, 0)
}

fn flow_statements_at(
    statements: &[Stmt],
    budget: &mut ExpressionBudget,
    scope_escapes: &BTreeSet<String>,
    depth: usize,
) -> Vec<FlowStatement> {
    statements
        .iter()
        .map(|statement| {
            if depth >= 128
                && matches!(
                    statement,
                    Stmt::If(_) | Stmt::Try(_) | Stmt::For(_) | Stmt::While(_) | Stmt::With(_)
                )
            {
                return FlowStatement::Unsupported {
                    range: source_range(statement),
                };
            }
            match statement {
                Stmt::Assign(assign) if assign.targets.len() == 1 => match &assign.targets[0] {
                    Expr::Name(name) => {
                        let name = name.id.to_string();
                        let value = expression(&assign.value, budget, 0);
                        let range = source_range(assign);
                        if scope_escapes.contains(&name) {
                            FlowStatement::EscapingAssign {
                                name,
                                annotation: None,
                                value,
                                range,
                            }
                        } else {
                            FlowStatement::Assign {
                                name,
                                annotation: None,
                                value,
                                range,
                            }
                        }
                    }
                    _ => FlowStatement::Store {
                        value: expression(&assign.value, budget, 0),
                        target: assign
                            .targets
                            .first()
                            .map(|target| expression(target, budget, 0)),
                        range: source_range(statement),
                    },
                },
                Stmt::Assign(assign) => FlowStatement::Store {
                    value: expression(&assign.value, budget, 0),
                    target: None,
                    range: source_range(statement),
                },
                Stmt::AnnAssign(assign) => match assign.target.as_ref() {
                    Expr::Name(target_name) => {
                        let name = target_name.id.to_string();
                        let annotation = expression(&assign.annotation, budget, 0);
                        let range = source_range(statement);
                        match assign.value.as_deref() {
                            Some(value) => {
                                let value = expression(value, budget, 0);
                                if scope_escapes.contains(&name) {
                                    FlowStatement::EscapingAssign {
                                        name,
                                        annotation: Some(annotation),
                                        value,
                                        range,
                                    }
                                } else {
                                    FlowStatement::Assign {
                                        name,
                                        annotation: Some(annotation),
                                        value,
                                        range,
                                    }
                                }
                            }
                            None => FlowStatement::AnnotatedDeclaration {
                                name: Some(name),
                                annotation,
                                range,
                            },
                        }
                    }
                    _ => FlowStatement::Store {
                        value: assign
                            .value
                            .as_ref()
                            .map(|value| expression(value, budget, 0))
                            .unwrap_or(AstExpr {
                                range: source_range(statement),
                                kind: AstExprKind::NoneLiteral,
                            }),
                        target: Some(expression(&assign.target, budget, 0)),
                        range: source_range(statement),
                    },
                },
                Stmt::FunctionDef(function) => {
                    let mut references = ReferencedNames::default();
                    for statement in &function.body {
                        references.visit_stmt(statement);
                    }
                    for parameter in function.parameters.iter() {
                        references.locals.insert(parameter.name().to_string());
                    }
                    references.finish();
                    FlowStatement::ClosureDefinition {
                        name: function.name.to_string(),
                        captures: references.names,
                        range: source_range(statement),
                    }
                }
                Stmt::Delete(delete)
                    if delete
                        .targets
                        .iter()
                        .all(|target| matches!(target, Expr::Name(_))) =>
                {
                    FlowStatement::DeleteLocal {
                        names: delete
                            .targets
                            .iter()
                            .filter_map(|target| match target {
                                Expr::Name(name) => Some(name.id.to_string()),
                                _ => None,
                            })
                            .collect(),
                        range: source_range(statement),
                    }
                }
                Stmt::Expr(expr) => FlowStatement::Expression {
                    value: expression(&expr.value, budget, 0),
                    range: source_range(statement),
                },
                Stmt::If(branch) => {
                    if depth >= 128 {
                        return FlowStatement::Unsupported {
                            range: source_range(statement),
                        };
                    }
                    let mut alternate = Vec::new();
                    if let Some(clause) = branch.elif_else_clauses.last() {
                        alternate =
                            flow_statements_at(&clause.body, budget, scope_escapes, depth + 1);
                        if branch
                            .elif_else_clauses
                            .iter()
                            .any(|clause| clause.test.is_some())
                        {
                            alternate.insert(
                                0,
                                FlowStatement::Unsupported {
                                    range: SourceRange {
                                        start: clause.range.start().to_usize(),
                                        end: clause.range.end().to_usize(),
                                    },
                                },
                            );
                        }
                    }
                    FlowStatement::If {
                        test: expression(&branch.test, budget, 0),
                        body: flow_statements_at(&branch.body, budget, scope_escapes, depth + 1),
                        alternate,
                        range: source_range(branch),
                    }
                }
                Stmt::Try(attempt) if attempt.is_star => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
                Stmt::Try(attempt) => FlowStatement::Try {
                    body: flow_statements_at(&attempt.body, budget, scope_escapes, depth + 1),
                    handlers: attempt
                        .handlers
                        .iter()
                        .map(|handler| match handler {
                            ruff_python_ast::ExceptHandler::ExceptHandler(handler) => FlowHandler {
                                exception: handler
                                    .type_
                                    .as_deref()
                                    .map(|expr| expression(expr, budget, 0)),
                                name: handler.name.as_ref().map(ToString::to_string),
                                body: flow_statements_at(
                                    &handler.body,
                                    budget,
                                    scope_escapes,
                                    depth + 1,
                                ),
                                range: source_range(handler),
                            },
                        })
                        .collect(),
                    alternate: flow_statements_at(
                        &attempt.orelse,
                        budget,
                        scope_escapes,
                        depth + 1,
                    ),
                    finalbody: flow_statements_at(
                        &attempt.finalbody,
                        budget,
                        scope_escapes,
                        depth + 1,
                    ),
                    range: source_range(attempt),
                },
                Stmt::For(loop_) if loop_.is_async => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
                Stmt::For(loop_) if !matches!(loop_.target.as_ref(), Expr::Name(_)) => {
                    FlowStatement::Unsupported {
                        range: source_range(statement),
                    }
                }
                Stmt::For(loop_) => FlowStatement::Loop {
                    test: None,
                    iterator: Some(expression(&loop_.iter, budget, 0)),
                    target: match loop_.target.as_ref() {
                        Expr::Name(name) => Some(name.id.to_string()),
                        _ => None,
                    },
                    body: flow_statements_at(&loop_.body, budget, scope_escapes, depth + 1),
                    alternate: flow_statements_at(&loop_.orelse, budget, scope_escapes, depth + 1),
                    range: source_range(loop_),
                },
                Stmt::While(loop_) => FlowStatement::Loop {
                    test: Some(expression(&loop_.test, budget, 0)),
                    iterator: None,
                    target: None,
                    body: flow_statements_at(&loop_.body, budget, scope_escapes, depth + 1),
                    alternate: flow_statements_at(&loop_.orelse, budget, scope_escapes, depth + 1),
                    range: source_range(loop_),
                },
                Stmt::With(with_) => FlowStatement::With {
                    items: with_
                        .items
                        .iter()
                        .map(|item| FlowWithItem {
                            context: expression(&item.context_expr, budget, 0),
                            unsupported_target: item
                                .optional_vars
                                .as_deref()
                                .is_some_and(|target| !matches!(target, Expr::Name(_))),
                            target: item
                                .optional_vars
                                .as_deref()
                                .and_then(|target| match target {
                                    Expr::Name(name) => Some(name.id.to_string()),
                                    _ => None,
                                }),
                            range: source_range(item),
                        })
                        .collect(),
                    body: flow_statements_at(&with_.body, budget, scope_escapes, depth + 1),
                    asynchronous: with_.is_async,
                    range: source_range(with_),
                },
                Stmt::Break(node) => FlowStatement::Break {
                    range: source_range(node),
                },
                Stmt::Continue(node) => FlowStatement::Continue {
                    range: source_range(node),
                },
                Stmt::Return(ret) => FlowStatement::Return {
                    value: ret
                        .value
                        .as_deref()
                        .map(|value| expression(value, budget, 0)),
                    range: source_range(statement),
                },
                Stmt::Raise(raise) => FlowStatement::Raise {
                    exception: raise
                        .exc
                        .as_deref()
                        .map(|value| expression(value, budget, 0)),
                    cause: raise
                        .cause
                        .as_deref()
                        .map(|value| expression(value, budget, 0)),
                    range: source_range(statement),
                },
                Stmt::Pass(_) => FlowStatement::Pass {
                    range: source_range(statement),
                },
                Stmt::Global(_) => FlowStatement::Pass {
                    range: source_range(statement),
                },
                _ => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
            }
        })
        .collect()
}

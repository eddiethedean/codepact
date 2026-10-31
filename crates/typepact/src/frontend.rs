use ruff_python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    AnyParameterRef, Expr, PythonVersion, Stmt,
};
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::Ranged;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AstExpr {
    pub range: SourceRange,
    pub kind: AstExprKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AstExprKind {
    Name(String),
    Attribute(Box<AstExpr>, String),
    Subscript(Box<AstExpr>, Vec<AstExpr>),
    Call {
        function: Box<AstExpr>,
        arguments: Vec<AstExpr>,
        keywords: Vec<(Option<String>, AstExpr)>,
    },
    Tuple(Vec<AstExpr>),
    String(String),
    NoneLiteral,
    Other,
    DepthLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportName {
    pub name: String,
    pub alias: Option<String>,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub name: String,
    pub annotation: Option<AstExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FlowStatement {
    Pass {
        range: SourceRange,
    },
    Assign {
        name: String,
        value: AstExpr,
        range: SourceRange,
    },
    Expression {
        value: AstExpr,
        range: SourceRange,
    },
    If {
        test: AstExpr,
        body: Vec<FlowStatement>,
        alternate: Vec<FlowStatement>,
        range: SourceRange,
    },
    Return {
        value: Option<AstExpr>,
        range: SourceRange,
    },
    Unsupported {
        range: SourceRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MethodBody {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: Vec<FlowStatement>,
    pub is_async: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Declaration {
    ImportFrom {
        module: String,
        level: u32,
        names: Vec<ImportName>,
        range: SourceRange,
    },
    Import {
        names: Vec<ImportName>,
        range: SourceRange,
    },
    Class {
        name: String,
        decorators: Vec<AstExpr>,
        methods: Vec<MethodBody>,
        unsupported: bool,
        range: SourceRange,
    },
    Function {
        name: String,
        parameters: Vec<Parameter>,
        returns: Option<AstExpr>,
        decorators: Vec<AstExpr>,
        is_async: bool,
        unsupported: bool,
        body: Vec<FlowStatement>,
        range: SourceRange,
    },
    Assignment {
        name: Option<String>,
        annotation: Option<AstExpr>,
        value: Option<AstExpr>,
        range: SourceRange,
    },
    Delete {
        names: Vec<Option<String>>,
        range: SourceRange,
    },
    TypeAlias {
        name: Option<String>,
        range: SourceRange,
    },
    Unsupported {
        message: &'static str,
        range: SourceRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrontendOutput {
    pub declarations: Vec<Declaration>,
    pub absolute_imports: Vec<String>,
    pub module_body: Vec<FlowStatement>,
}

fn source_range(node: &impl Ranged) -> SourceRange {
    let range = node.range();
    SourceRange {
        start: range.start().to_usize(),
        end: range.end().to_usize(),
    }
}

struct ExpressionBudget {
    nodes: usize,
    exceeded: bool,
}

impl ExpressionBudget {
    const MAX_DEPTH: usize = 128;
    const MAX_NODES: usize = 50_000;
}

struct NamedExpressionVisitor {
    depth: usize,
    nodes: usize,
    found: bool,
    exceeded: bool,
}

impl NamedExpressionVisitor {
    fn new() -> Self {
        Self {
            depth: 0,
            nodes: 0,
            found: false,
            exceeded: false,
        }
    }

    fn unsupported_reason(&self) -> Option<&'static str> {
        if self.found {
            Some("module-level named expressions in declaration headers are unsupported")
        } else if self.exceeded {
            Some("AST expression exceeds the 128-level or 50,000-node work limit")
        } else {
            None
        }
    }
}

impl<'a> Visitor<'a> for NamedExpressionVisitor {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if self.found || self.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH || self.nodes >= ExpressionBudget::MAX_NODES {
            self.exceeded = true;
            return;
        }
        self.nodes += 1;
        if matches!(expression, Expr::Named(_)) {
            self.found = true;
            return;
        }
        self.depth += 1;
        if let Expr::Lambda(lambda) = expression {
            // Creating a lambda evaluates its defaults, but its body has its own
            // scope and runs only when called.
            if let Some(parameters) = &lambda.parameters {
                self.visit_parameters(parameters);
            }
        } else {
            walk_expr(self, expression);
        }
        self.depth -= 1;
    }
}

fn eager_named_expression_issue(statement: &Stmt) -> Option<&'static str> {
    let mut visitor = NamedExpressionVisitor::new();
    match statement {
        Stmt::ClassDef(class) => {
            for decorator in &class.decorator_list {
                visitor.visit_decorator(decorator);
            }
            if let Some(arguments) = &class.arguments {
                visitor.visit_arguments(arguments);
            }
        }
        Stmt::FunctionDef(function) => {
            for decorator in &function.decorator_list {
                visitor.visit_decorator(decorator);
            }
            visitor.visit_parameters(&function.parameters);
            if let Some(returns) = &function.returns {
                visitor.visit_expr(returns);
            }
        }
        Stmt::Assign(assign) => visitor.visit_expr(&assign.value),
        Stmt::AnnAssign(assign) => {
            visitor.visit_expr(&assign.annotation);
            if let Some(value) = &assign.value {
                visitor.visit_expr(value);
            }
        }
        _ => {}
    }
    visitor.unsupported_reason()
}

// Class suites run eagerly, while function bodies have a separate, deferred
// scope. A global declaration anywhere in an eager suite can redirect writes
// into the module namespace, which this declaration-only frontend cannot model.
#[derive(Default)]
struct ClassGlobalVisitor {
    depth: usize,
    nodes: usize,
    found: bool,
    exceeded: bool,
}

impl<'a> Visitor<'a> for ClassGlobalVisitor {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.found || self.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH || self.nodes >= ExpressionBudget::MAX_NODES {
            self.exceeded = true;
            return;
        }
        self.nodes += 1;
        match statement {
            Stmt::Global(_) => self.found = true,
            Stmt::FunctionDef(_) => {}
            _ => {
                self.depth += 1;
                walk_stmt(self, statement);
                self.depth -= 1;
            }
        }
    }

    fn visit_expr(&mut self, _expression: &'a Expr) {}
}

fn class_body_issue(statement: &Stmt) -> Option<&'static str> {
    let Stmt::ClassDef(class) = statement else {
        return None;
    };
    let mut visitor = ClassGlobalVisitor::default();
    for statement in &class.body {
        visitor.visit_stmt(statement);
    }
    if visitor.found {
        Some("class-body global declarations can rebind module names and are unsupported")
    } else if visitor.exceeded {
        Some("class body exceeds the 128-level or 50,000-statement work limit")
    } else {
        None
    }
}

fn expression(node: &Expr, budget: &mut ExpressionBudget, depth: usize) -> AstExpr {
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
        Expr::Tuple(tuple) => AstExprKind::Tuple(
            tuple
                .elts
                .iter()
                .map(|item| expression(item, budget, depth + 1))
                .collect(),
        ),
        Expr::StringLiteral(string) => AstExprKind::String(string.value.to_str().to_owned()),
        Expr::NoneLiteral(_) => AstExprKind::NoneLiteral,
        _ => AstExprKind::Other,
    };
    AstExpr { range, kind }
}

fn imports(names: &[ruff_python_ast::Alias]) -> Vec<ImportName> {
    names
        .iter()
        .map(|name| ImportName {
            name: name.name.to_string(),
            alias: name.asname.as_ref().map(ToString::to_string),
            range: source_range(name),
        })
        .collect()
}

fn decorators(list: &[ruff_python_ast::Decorator], budget: &mut ExpressionBudget) -> Vec<AstExpr> {
    list.iter()
        .map(|decorator| {
            let mut expression = expression(&decorator.expression, budget, 0);
            expression.range = source_range(decorator);
            expression
        })
        .collect()
}

fn flow_statements(statements: &[Stmt], budget: &mut ExpressionBudget) -> Vec<FlowStatement> {
    statements
        .iter()
        .map(|statement| match statement {
            Stmt::Assign(assign) if assign.targets.len() == 1 => match &assign.targets[0] {
                Expr::Name(name) => FlowStatement::Assign {
                    name: name.id.to_string(),
                    value: expression(&assign.value, budget, 0),
                    range: source_range(assign),
                },
                _ => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
            },
            Stmt::AnnAssign(assign) => match assign.target.as_ref() {
                Expr::Name(name) => assign.value.as_deref().map_or(
                    FlowStatement::Unsupported {
                        range: source_range(statement),
                    },
                    |value| FlowStatement::Assign {
                        name: name.id.to_string(),
                        value: expression(value, budget, 0),
                        range: source_range(statement),
                    },
                ),
                _ => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
            },
            Stmt::Expr(expr) => FlowStatement::Expression {
                value: expression(&expr.value, budget, 0),
                range: source_range(statement),
            },
            Stmt::If(branch) => {
                let mut alternate = Vec::new();
                if let Some(clause) = branch.elif_else_clauses.last() {
                    alternate = flow_statements(&clause.body, budget);
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
                    body: flow_statements(&branch.body, budget),
                    alternate,
                    range: source_range(branch),
                }
            }
            Stmt::Return(ret) => FlowStatement::Return {
                value: ret
                    .value
                    .as_deref()
                    .map(|value| expression(value, budget, 0)),
                range: source_range(statement),
            },
            Stmt::Pass(_) => FlowStatement::Pass {
                range: source_range(statement),
            },
            _ => FlowStatement::Unsupported {
                range: source_range(statement),
            },
        })
        .collect()
}

pub(crate) fn parse_frontend(
    source: &str,
    python_version: &str,
) -> Result<FrontendOutput, (String, usize)> {
    let target_version = match python_version {
        "3.11" => PythonVersion::PY311,
        "3.12" => PythonVersion::PY312,
        "3.13" => PythonVersion::PY313,
        _ => PythonVersion::PY311,
    };
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(target_version),
    );
    if let Some(error) = parsed.errors().first() {
        return Err((error.to_string(), error.location.start().to_usize()));
    }
    if let Some(error) = parsed.unsupported_syntax_errors().first() {
        return Err((error.to_string(), error.range.start().to_usize()));
    }
    let module = parsed
        .try_into_module()
        .expect("module parser returns a module")
        .into_syntax();
    let absolute_imports = module
        .body
        .iter()
        .take(crate::DEFAULT_MAX_AST_DECLARATIONS)
        .filter_map(|statement| match statement {
            Stmt::ImportFrom(import) if import.level == 0 => {
                import.module.as_ref().map(ToString::to_string)
            }
            _ => None,
        })
        .collect();
    let mut declarations = Vec::new();
    for (index, statement) in module.body.iter().enumerate() {
        if index >= crate::DEFAULT_MAX_AST_DECLARATIONS {
            declarations.push(Declaration::Unsupported {
                message: "module declaration count exceeds the 20,000 statement work limit",
                range: source_range(statement),
            });
            break;
        }
        if let Some(message) =
            class_body_issue(statement).or_else(|| eager_named_expression_issue(statement))
        {
            declarations.push(Declaration::Unsupported {
                message,
                range: source_range(statement),
            });
            continue;
        }
        let mut expression_budget = ExpressionBudget {
            nodes: 0,
            exceeded: false,
        };
        match statement {
            Stmt::ImportFrom(import) => declarations.push(Declaration::ImportFrom {
                module: import
                    .module
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                level: import.level,
                names: imports(&import.names),
                range: source_range(import),
            }),
            Stmt::Import(import) => declarations.push(Declaration::Import {
                names: imports(&import.names),
                range: source_range(import),
            }),
            Stmt::ClassDef(class) => declarations.push(Declaration::Class {
                name: class.name.to_string(),
                decorators: decorators(&class.decorator_list, &mut expression_budget),
                methods: class
                    .body
                    .iter()
                    .filter_map(|statement| {
                        let Stmt::FunctionDef(method) = statement else {
                            return None;
                        };
                        let parameters = method
                            .parameters
                            .iter_source_order()
                            .map(|parameter| match parameter {
                                AnyParameterRef::NonVariadic(parameter) => {
                                    parameter.parameter.name.to_string()
                                }
                                AnyParameterRef::Variadic(parameter) => parameter.name.to_string(),
                            })
                            .collect();
                        Some(MethodBody {
                            name: method.name.to_string(),
                            parameters,
                            body: flow_statements(&method.body, &mut expression_budget),
                            is_async: method.is_async,
                        })
                    })
                    .collect(),
                unsupported: class
                    .type_params
                    .as_ref()
                    .is_some_and(|params| !params.is_empty())
                    || class
                        .arguments
                        .as_ref()
                        .is_some_and(|args| !args.args.is_empty() || !args.keywords.is_empty())
                    || class
                        .body
                        .iter()
                        .any(|stmt| matches!(stmt, Stmt::ClassDef(_))),
                range: source_range(class),
            }),
            Stmt::FunctionDef(function) => {
                let parameters = function
                    .parameters
                    .iter_source_order()
                    .map(|parameter| {
                        let parameter = match parameter {
                            AnyParameterRef::NonVariadic(parameter) => &parameter.parameter,
                            AnyParameterRef::Variadic(parameter) => parameter,
                        };
                        Parameter {
                            name: parameter.name.to_string(),
                            annotation: parameter.annotation().map(|annotation| {
                                expression(annotation, &mut expression_budget, 0)
                            }),
                        }
                    })
                    .collect();
                declarations.push(Declaration::Function {
                    name: function.name.to_string(),
                    parameters,
                    returns: function
                        .returns
                        .as_deref()
                        .map(|annotation| expression(annotation, &mut expression_budget, 0)),
                    decorators: decorators(&function.decorator_list, &mut expression_budget),
                    is_async: function.is_async,
                    unsupported: function
                        .type_params
                        .as_ref()
                        .is_some_and(|params| !params.is_empty())
                        || function
                            .body
                            .iter()
                            .any(|stmt| matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_))),
                    body: flow_statements(&function.body, &mut expression_budget),
                    range: source_range(function),
                });
            }
            Stmt::Assign(assign) => {
                let name = if assign.targets.len() == 1 {
                    match &assign.targets[0] {
                        Expr::Name(name) => Some(name.id.to_string()),
                        _ => None,
                    }
                } else {
                    None
                };
                if name.is_some() {
                    declarations.push(Declaration::Assignment {
                        name,
                        annotation: None,
                        value: Some(expression(&assign.value, &mut expression_budget, 0)),
                        range: source_range(assign),
                    });
                } else {
                    declarations.push(Declaration::Unsupported {
                        message: "destructuring and chained assignments are unsupported",
                        range: source_range(assign),
                    });
                }
            }
            Stmt::AnnAssign(assign) => {
                let name = match assign.target.as_ref() {
                    Expr::Name(name) => Some(name.id.to_string()),
                    _ => None,
                };
                if name.is_some() {
                    declarations.push(Declaration::Assignment {
                        name,
                        annotation: Some(expression(&assign.annotation, &mut expression_budget, 0)),
                        value: assign
                            .value
                            .as_deref()
                            .map(|value| expression(value, &mut expression_budget, 0)),
                        range: source_range(assign),
                    });
                } else {
                    declarations.push(Declaration::Unsupported {
                        message:
                            "annotated attribute and destructuring assignments are unsupported",
                        range: source_range(assign),
                    });
                }
            }
            Stmt::Delete(delete) => declarations.push(Declaration::Delete {
                names: delete
                    .targets
                    .iter()
                    .map(|target| match target {
                        Expr::Name(name) => Some(name.id.to_string()),
                        _ => None,
                    })
                    .collect(),
                range: source_range(delete),
            }),
            Stmt::Pass(_) => {}
            Stmt::Expr(expression)
                if index == 0 && matches!(expression.value.as_ref(), Expr::StringLiteral(_)) => {}
            Stmt::TypeAlias(alias) => {
                let name = match alias.name.as_ref() {
                    Expr::Name(name) => Some(name.id.to_string()),
                    _ => None,
                };
                declarations.push(Declaration::TypeAlias {
                    name,
                    range: source_range(alias),
                });
            }
            _ => declarations.push(Declaration::Unsupported {
                message: "module-level control flow and executable statements are unsupported",
                range: source_range(statement),
            }),
        }
        if expression_budget.exceeded {
            declarations.push(Declaration::Unsupported {
                message:
                    "AST expression exceeds the 128-level or 50,000-node per-statement work limit",
                range: source_range(statement),
            });
        }
    }
    Ok(FrontendOutput {
        declarations,
        absolute_imports,
        module_body: flow_statements(
            &module.body,
            &mut ExpressionBudget {
                nodes: 0,
                exceeded: false,
            },
        )
        .into_iter()
        .zip(module.body.iter())
        .filter_map(|(flow, statement)| match statement {
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Pass(_) => None,
            _ => Some(flow),
        })
        .collect(),
    })
}

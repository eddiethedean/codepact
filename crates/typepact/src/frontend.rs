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
    Await(Box<AstExpr>),
    Tuple(Vec<AstExpr>),
    Boolean(bool),
    String(String),
    Primitive,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CallSignature {
    pub positional_only: Vec<String>,
    pub positional_or_keyword: Vec<String>,
    pub keyword_only: Vec<String>,
    pub optional: Vec<String>,
    pub unknown_defaults: Vec<String>,
    pub vararg: Option<String>,
    pub kwarg: Option<String>,
}

impl CallSignature {
    fn from_parameters(parameters: &ruff_python_ast::Parameters) -> Self {
        Self {
            positional_only: parameters
                .posonlyargs
                .iter()
                .map(|p| p.name().to_string())
                .collect(),
            positional_or_keyword: parameters
                .args
                .iter()
                .map(|p| p.name().to_string())
                .collect(),
            keyword_only: parameters
                .kwonlyargs
                .iter()
                .map(|p| p.name().to_string())
                .collect(),
            optional: parameters
                .iter_non_variadic_params()
                .filter(|p| p.default.is_some())
                .map(|p| p.name().to_string())
                .collect(),
            unknown_defaults: parameters
                .iter_non_variadic_params()
                .filter(|p| {
                    p.default.as_ref().is_some_and(|default| {
                        !matches!(
                            default.as_ref(),
                            Expr::BooleanLiteral(_)
                                | Expr::NumberLiteral(_)
                                | Expr::StringLiteral(_)
                                | Expr::NoneLiteral(_)
                        )
                    })
                })
                .map(|p| p.name().to_string())
                .collect(),
            vararg: parameters.vararg.as_ref().map(|p| p.name.to_string()),
            kwarg: parameters.kwarg.as_ref().map(|p| p.name.to_string()),
        }
    }
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
    Try {
        body: Vec<FlowStatement>,
        handlers: Vec<FlowHandler>,
        alternate: Vec<FlowStatement>,
        finalbody: Vec<FlowStatement>,
        range: SourceRange,
    },
    Loop {
        test: Option<AstExpr>,
        iterator: Option<AstExpr>,
        target: Option<String>,
        body: Vec<FlowStatement>,
        alternate: Vec<FlowStatement>,
        range: SourceRange,
    },
    With {
        items: Vec<FlowWithItem>,
        body: Vec<FlowStatement>,
        asynchronous: bool,
        range: SourceRange,
    },
    Break {
        range: SourceRange,
    },
    Continue {
        range: SourceRange,
    },
    Return {
        value: Option<AstExpr>,
        range: SourceRange,
    },
    Raise {
        exception: Option<AstExpr>,
        range: SourceRange,
    },
    BindFunction {
        name: String,
        binding: Option<String>,
        class_decorators: Vec<SourceRange>,
        range: SourceRange,
    },
    Unsupported {
        range: SourceRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlowHandler {
    pub exception: Option<AstExpr>,
    pub name: Option<String>,
    pub body: Vec<FlowStatement>,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlowWithItem {
    pub context: AstExpr,
    pub target: Option<String>,
    pub unsupported_target: bool,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MethodBody {
    pub name: String,
    pub parameters: Vec<String>,
    pub signature: CallSignature,
    pub has_parameter_annotations: bool,
    pub range: SourceRange,
    pub body: Vec<FlowStatement>,
    pub is_async: bool,
    pub decorated: bool,
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
        signature: CallSignature,
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
        Expr::StringLiteral(string) => AstExprKind::String(string.value.to_str().to_owned()),
        Expr::BooleanLiteral(boolean) => AstExprKind::Boolean(boolean.value),
        Expr::NumberLiteral(_) => AstExprKind::Primitive,
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
    flow_statements_at(statements, budget, 0)
}

fn flow_statements_at(
    statements: &[Stmt],
    budget: &mut ExpressionBudget,
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
                    if depth >= 128 {
                        return FlowStatement::Unsupported {
                            range: source_range(statement),
                        };
                    }
                    let mut alternate = Vec::new();
                    if let Some(clause) = branch.elif_else_clauses.last() {
                        alternate = flow_statements_at(&clause.body, budget, depth + 1);
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
                        body: flow_statements_at(&branch.body, budget, depth + 1),
                        alternate,
                        range: source_range(branch),
                    }
                }
                Stmt::Try(attempt) if attempt.is_star => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
                Stmt::Try(attempt) => FlowStatement::Try {
                    body: flow_statements_at(&attempt.body, budget, depth + 1),
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
                                body: flow_statements_at(&handler.body, budget, depth + 1),
                                range: source_range(handler),
                            },
                        })
                        .collect(),
                    alternate: flow_statements_at(&attempt.orelse, budget, depth + 1),
                    finalbody: flow_statements_at(&attempt.finalbody, budget, depth + 1),
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
                    body: flow_statements_at(&loop_.body, budget, depth + 1),
                    alternate: flow_statements_at(&loop_.orelse, budget, depth + 1),
                    range: source_range(loop_),
                },
                Stmt::While(loop_) => FlowStatement::Loop {
                    test: Some(expression(&loop_.test, budget, 0)),
                    iterator: None,
                    target: None,
                    body: flow_statements_at(&loop_.body, budget, depth + 1),
                    alternate: flow_statements_at(&loop_.orelse, budget, depth + 1),
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
                    body: flow_statements_at(&with_.body, budget, depth + 1),
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
                    range: source_range(statement),
                },
                Stmt::Pass(_) => FlowStatement::Pass {
                    range: source_range(statement),
                },
                _ => FlowStatement::Unsupported {
                    range: source_range(statement),
                },
            }
        })
        .collect()
}

pub(crate) fn parse_frontend(
    source: &str,
    module_name: &str,
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
        .flat_map(|statement| match statement {
            Stmt::ImportFrom(import) if import.level == 0 => import
                .module
                .as_ref()
                .map(ToString::to_string)
                .into_iter()
                .collect::<Vec<_>>(),
            Stmt::Import(import) => import
                .names
                .iter()
                .map(|name| name.name.to_string())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
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
                            signature: CallSignature::from_parameters(&method.parameters),
                            has_parameter_annotations: method
                                .parameters
                                .iter_source_order()
                                .any(|parameter| parameter.annotation().is_some()),
                            range: source_range(method),
                            body: flow_statements(&method.body, &mut expression_budget),
                            is_async: method.is_async,
                            decorated: !method.decorator_list.is_empty(),
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
                    signature: CallSignature::from_parameters(&function.parameters),
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
    let header_boundaries = definition_header_boundaries(&module.body);
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
        .enumerate()
        .flat_map(|(index, (flow, statement))| match statement {
            Stmt::FunctionDef(function) => {
                let mut statements = Vec::new();
                if header_boundaries[index] {
                    statements.push(FlowStatement::Unsupported {
                        range: source_range(statement),
                    });
                }
                statements.push(FlowStatement::BindFunction {
                    name: function.name.to_string(),
                    binding: Some(format!(
                        "{}::{}:{}",
                        module_name,
                        function.name,
                        source_range(statement).start
                    )),
                    class_decorators: Vec::new(),
                    range: source_range(statement),
                });
                statements
            }
            Stmt::ClassDef(class) => {
                let mut statements = Vec::new();
                if header_boundaries[index] {
                    statements.push(FlowStatement::Unsupported {
                        range: source_range(statement),
                    });
                }
                statements.push(FlowStatement::BindFunction {
                    name: class.name.to_string(),
                    binding: Some("class".to_owned()),
                    class_decorators: class.decorator_list.iter().map(source_range).collect(),
                    range: source_range(statement),
                });
                statements
            }
            Stmt::ImportFrom(import) => {
                let mut statements = Vec::new();
                if import.level != 0
                    || import
                        .names
                        .iter()
                        .any(|alias| alias.name.to_string() == "*")
                {
                    statements.push(FlowStatement::Unsupported {
                        range: source_range(statement),
                    });
                }
                statements.extend(import.names.iter().map(|alias| {
                    FlowStatement::BindFunction {
                        name: alias
                            .asname
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| {
                                alias.name.split('.').next().unwrap_or_default().to_owned()
                            }),
                        binding: import.module.as_ref().map(|source_module| {
                            let local = alias
                                .asname
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| alias.name.to_string());
                            if source_module.as_str() == "contextlib"
                                && matches!(alias.name.as_str(), "closing" | "aclosing")
                            {
                                format!("external:contextlib.{}", alias.name)
                            } else {
                                format!(
                                    "import:{}:{}:{}",
                                    module_name,
                                    source_range(statement).start,
                                    local
                                )
                            }
                        }),
                        range: source_range(statement),
                        class_decorators: Vec::new(),
                    }
                }));
                statements
            }
            Stmt::Import(import) => import
                .names
                .iter()
                .map(|alias| FlowStatement::BindFunction {
                    name: alias
                        .asname
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| {
                            alias.name.split('.').next().unwrap_or_default().to_owned()
                        }),
                    binding: Some(format!("external:{}", alias.name)),
                    class_decorators: Vec::new(),
                    range: source_range(statement),
                })
                .collect(),
            Stmt::Pass(_) => Vec::new(),
            _ => vec![flow],
        })
        .collect(),
    })
}

fn function_header_requires_boundary(
    function: &ruff_python_ast::StmtFunctionDef,
    safe_metadata_calls: &std::collections::BTreeSet<String>,
    safe_annotation_subscripts: &std::collections::BTreeSet<String>,
    annotated_subscripts: &std::collections::BTreeSet<String>,
) -> bool {
    !function.decorator_list.is_empty()
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

fn dotted_name(expression: &Expr) -> Option<String> {
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

fn clear_imported_names(names: &mut std::collections::BTreeSet<String>, local: &str) {
    names.retain(|name| {
        name != local
            && !name
                .strip_prefix(local)
                .is_some_and(|suffix| suffix.starts_with('.'))
    });
}

fn assignment_roots(target: &Expr, names: &mut std::collections::BTreeSet<String>) {
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

fn definition_header_boundaries(statements: &[Stmt]) -> Vec<bool> {
    let metadata = ["Resource", "SharedBorrow", "OwnedAccess", "ExclusiveBorrow"];
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
    let mut known_resource_classes = std::collections::BTreeSet::new();
    let mut boundaries = Vec::with_capacity(statements.len());
    for statement in statements {
        let boundary = match statement {
            Stmt::FunctionDef(function) => function_header_requires_boundary(
                function,
                &metadata_names,
                &safe_annotation_subscripts,
                &annotated_subscripts,
            ),
            Stmt::ClassDef(class) => {
                class_body_requires_boundary(
                    class,
                    &metadata_names,
                    &safe_annotation_subscripts,
                    &annotated_subscripts,
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
                    known_resource_classes.clear();
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
                    if import.level != 0 {
                        continue;
                    }
                    match module.as_deref() {
                        Some("typepact.contracts") if metadata.contains(&source.as_str()) => {
                            metadata_names.insert(local.clone());
                        }
                        Some("typepact") if source == "Resource" => {
                            metadata_names.insert(local.clone());
                        }
                        Some("typepact") if source == "contracts" => {
                            for name in metadata {
                                metadata_names.insert(format!("{local}.{name}"));
                            }
                        }
                        _ => {}
                    }
                    if matches!(
                        module.as_deref(),
                        Some(
                            "typing" | "typing_extensions" | "typepact.rust" | "typepact.contracts"
                        )
                    ) {
                        safe_annotation_subscripts.insert(local.clone());
                    } else if module.as_deref() == Some("typepact")
                        && matches!(source.as_str(), "rust" | "contracts")
                    {
                        safe_annotation_subscripts.insert(format!("{local}.*"));
                    }
                    if matches!(module.as_deref(), Some("typing" | "typing_extensions"))
                        && source == "Annotated"
                    {
                        annotated_subscripts.insert(local.clone());
                    }
                    if import.level == 0
                        && module.as_deref() == Some("typepact")
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
                    if source == "typepact.contracts" {
                        let prefix = alias
                            .asname
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "typepact.contracts".to_owned());
                        for name in metadata {
                            metadata_names.insert(format!("{prefix}.{name}"));
                        }
                    } else if source == "typepact" {
                        metadata_names.insert(format!("{local}.Resource"));
                        resource_names.insert(format!("{local}.resource"));
                    }
                    known_resource_classes.remove(&local);
                    if matches!(
                        source.as_str(),
                        "typing"
                            | "typing_extensions"
                            | "typepact.rust"
                            | "typepact.contracts"
                            | "typepact"
                    ) {
                        safe_annotation_subscripts.insert(format!("{local}.*"));
                    }
                    if matches!(source.as_str(), "typing" | "typing_extensions") {
                        annotated_subscripts.insert(format!("{local}.Annotated"));
                    }
                }
            }
            Stmt::FunctionDef(function) => {
                clear_imported_names(&mut metadata_names, function.name.as_ref());
                clear_imported_names(&mut resource_names, function.name.as_ref());
                clear_imported_names(&mut safe_annotation_subscripts, function.name.as_ref());
                clear_imported_names(&mut annotated_subscripts, function.name.as_ref());
                known_resource_classes.remove(function.name.as_ref());
            }
            Stmt::ClassDef(class) => {
                clear_imported_names(&mut metadata_names, class.name.as_ref());
                clear_imported_names(&mut resource_names, class.name.as_ref());
                clear_imported_names(&mut safe_annotation_subscripts, class.name.as_ref());
                clear_imported_names(&mut annotated_subscripts, class.name.as_ref());
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
                    known_resource_classes.remove(&name);
                }
            }
            Stmt::TypeAlias(_) => {
                metadata_names.clear();
                resource_names.clear();
                safe_annotation_subscripts.clear();
                annotated_subscripts.clear();
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
                    known_resource_classes.clear();
                }
            }
            _ => {
                metadata_names.clear();
                resource_names.clear();
                safe_annotation_subscripts.clear();
                annotated_subscripts.clear();
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
) -> bool {
    class
        .body
        .iter()
        .enumerate()
        .any(|(index, statement)| match statement {
            Stmt::FunctionDef(function) => {
                parameters_have_effectful_defaults(&function.parameters)
                    || !function.decorator_list.is_empty()
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

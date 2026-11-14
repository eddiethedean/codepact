use ruff_python_ast::{
    token::TokenKind,
    visitor::{walk_expr, walk_pattern, walk_stmt, Visitor},
    AnyParameterRef, Expr, Pattern, PythonVersion, Stmt,
};
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

mod boundaries;
mod lowering;
mod model;
pub(crate) use model::*;

use boundaries::{
    assignment_roots, clear_imported_names, contains_yield, definition_header_boundaries,
    dotted_name,
};
use lowering::{
    decorators, decorators_are_only_must_use, expression, flow_statements, imports,
    update_must_use_names,
};

pub(crate) const MODULE_STAR_IMPORT_UNCERTAINTY: &str =
    "module-level wildcard import is unsupported";

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

fn source_range(node: &impl Ranged) -> SourceRange {
    let range = node.range();
    SourceRange {
        start: range.start().to_usize(),
        end: range.end().to_usize(),
    }
}

pub(super) struct ExpressionBudget {
    pub(super) nodes: usize,
    pub(super) exceeded: bool,
}

struct NestedCallCollector<'b> {
    budget: &'b mut ExpressionBudget,
    calls: Vec<String>,
    depth: usize,
}

impl<'a> Visitor<'a> for NestedCallCollector<'_> {
    fn visit_expr(&mut self, node: &'a Expr) {
        if self.budget.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH
            || self.budget.nodes >= ExpressionBudget::MAX_NODES
        {
            self.budget.exceeded = true;
            return;
        }
        self.budget.nodes += 1;
        self.depth += 1;
        if let Expr::Call(call) = node {
            if let Some(path) = expression_identity(&call.func) {
                self.calls.push(path);
            }
        }
        walk_expr(self, node);
        self.depth -= 1;
    }
}

fn expression_identity(node: &Expr) -> Option<String> {
    match node {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            expression_identity(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}

impl ExpressionBudget {
    pub(super) const MAX_DEPTH: usize = 128;
    pub(super) const MAX_NODES: usize = 50_000;
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

#[derive(Default)]
struct ModuleBindingUncertaintyVisitor {
    depth: usize,
    nodes: usize,
    wildcard_import: bool,
    exception_bindings: BTreeSet<String>,
    exceeded: bool,
}

impl ModuleBindingUncertaintyVisitor {
    fn add_binding_name(&mut self, name: &str) {
        if is_exception_binding_name(name) {
            self.exception_bindings.insert(name.to_owned());
        }
    }

    fn add_all_exception_bindings(&mut self) {
        self.exception_bindings
            .extend(all_exception_binding_names());
    }

    fn add_target(&mut self, target: &Expr) {
        let mut pending = vec![target];
        let mut nodes = 0;
        while let Some(target) = pending.pop() {
            if nodes >= ExpressionBudget::MAX_NODES {
                self.exceeded = true;
                return;
            }
            nodes += 1;
            match target {
                Expr::Name(name) => self.add_binding_name(name.id.as_str()),
                Expr::Tuple(tuple) => pending.extend(tuple.elts.iter()),
                Expr::List(list) => pending.extend(list.elts.iter()),
                Expr::Starred(starred) => pending.push(&starred.value),
                _ => {}
            }
        }
    }

    fn finish(self) -> Vec<String> {
        if self.exceeded {
            all_exception_binding_names()
        } else {
            self.exception_bindings.into_iter().collect()
        }
    }
}

impl<'a> Visitor<'a> for ModuleBindingUncertaintyVisitor {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.wildcard_import || self.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH || self.nodes >= ExpressionBudget::MAX_NODES {
            self.exceeded = true;
            return;
        }
        self.nodes += 1;
        match statement {
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        self.wildcard_import = true;
                        return;
                    }
                    let name = alias
                        .asname
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| alias.name.to_string());
                    self.add_binding_name(&name);
                }
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let name = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_owned(),
                        ToString::to_string,
                    );
                    self.add_binding_name(&name);
                }
            }
            Stmt::FunctionDef(function) => {
                self.add_binding_name(function.name.as_str());
            }
            Stmt::ClassDef(class) => {
                self.add_binding_name(class.name.as_str());
                if class_body_issue(statement).is_some() {
                    self.add_all_exception_bindings();
                }
            }
            Stmt::TypeAlias(alias) => {
                if let Expr::Name(name) = alias.name.as_ref() {
                    self.add_binding_name(name.id.as_str());
                }
                self.walk_statement(statement);
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.add_target(target);
                }
                self.walk_statement(statement);
            }
            Stmt::AnnAssign(assign) => {
                self.add_target(&assign.target);
                self.walk_statement(statement);
            }
            Stmt::AugAssign(assign) => {
                self.add_target(&assign.target);
                self.walk_statement(statement);
            }
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    self.add_target(target);
                }
            }
            Stmt::For(loop_) => {
                self.add_target(&loop_.target);
                self.walk_statement(statement);
            }
            Stmt::With(with_) => {
                for item in &with_.items {
                    if let Some(target) = &item.optional_vars {
                        self.add_target(target);
                    }
                }
                self.walk_statement(statement);
            }
            _ => {
                self.walk_statement(statement);
            }
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        if self.wildcard_import || self.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH || self.nodes >= ExpressionBudget::MAX_NODES {
            self.exceeded = true;
            return;
        }
        self.nodes += 1;
        if let Expr::Named(named) = expression {
            self.add_target(&named.target);
        }
        if let Expr::Lambda(lambda) = expression {
            if let Some(parameters) = &lambda.parameters {
                self.visit_parameters(parameters);
            }
            return;
        }
        self.depth += 1;
        walk_expr(self, expression);
        self.depth -= 1;
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if self.wildcard_import || self.exceeded {
            return;
        }
        if self.depth >= ExpressionBudget::MAX_DEPTH || self.nodes >= ExpressionBudget::MAX_NODES {
            self.exceeded = true;
            return;
        }
        self.nodes += 1;
        match pattern {
            Pattern::MatchAs(pattern) => {
                if let Some(name) = &pattern.name {
                    self.add_binding_name(name.as_str());
                }
            }
            Pattern::MatchStar(pattern) => {
                if let Some(name) = &pattern.name {
                    self.add_binding_name(name.as_str());
                }
            }
            Pattern::MatchMapping(pattern) => {
                if let Some(name) = &pattern.rest {
                    self.add_binding_name(name.as_str());
                }
            }
            _ => {}
        }
        self.depth += 1;
        walk_pattern(self, pattern);
        self.depth -= 1;
    }
}

impl ModuleBindingUncertaintyVisitor {
    fn walk_statement(&mut self, statement: &Stmt) {
        self.depth += 1;
        walk_stmt(self, statement);
        self.depth -= 1;
    }
}

fn module_binding_uncertainty(statement: &Stmt) -> (bool, Vec<String>) {
    let mut visitor = ModuleBindingUncertaintyVisitor::default();
    visitor.visit_stmt(statement);
    let wildcard_import = visitor.wildcard_import;
    (wildcard_import, visitor.finish())
}

const BUILTIN_EXCEPTION_NAMES: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionError",
    "ConnectionAbortedError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "EncodingWarning",
    "Exception",
    "ExceptionGroup",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
];

pub(crate) fn is_known_builtin_exception_name(name: &str) -> bool {
    BUILTIN_EXCEPTION_NAMES.contains(&name)
}

fn is_exception_binding_name(name: &str) -> bool {
    is_known_builtin_exception_name(name) || name == "CancelledError"
}

fn all_exception_binding_names() -> Vec<String> {
    BUILTIN_EXCEPTION_NAMES
        .iter()
        .copied()
        .chain(std::iter::once("CancelledError"))
        .map(str::to_owned)
        .collect()
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

#[derive(Default)]
struct ScopeEscapeVisitor {
    names: BTreeSet<String>,
}

impl<'a> Visitor<'a> for ScopeEscapeVisitor {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Global(declaration) => {
                self.names
                    .extend(declaration.names.iter().map(ToString::to_string));
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
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
    let mut must_use_names = std::collections::BTreeSet::new();
    for (index, statement) in module.body.iter().enumerate() {
        if index >= crate::DEFAULT_MAX_AST_DECLARATIONS {
            declarations.push(Declaration::Unsupported {
                message: "module declaration count exceeds the 20,000 statement work limit",
                unknown_exception_bindings: all_exception_binding_names(),
                range: source_range(statement),
            });
            break;
        }
        let unsupported_issue =
            class_body_issue(statement).or_else(|| eager_named_expression_issue(statement));
        if let Some(message) = unsupported_issue {
            declarations.push(Declaration::Unsupported {
                message,
                unknown_exception_bindings: all_exception_binding_names(),
                range: source_range(statement),
            });
            must_use_names.clear();
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
                class_annotations: class
                    .body
                    .iter()
                    .filter_map(|statement| match statement {
                        Stmt::AnnAssign(assignment) => Some(expression(
                            &assignment.annotation,
                            &mut expression_budget,
                            0,
                        )),
                        _ => None,
                    })
                    .collect(),
                has_unresolved_base_classes: class
                    .bases()
                    .iter()
                    .any(|base| !matches!(base, Expr::Name(name) if name.id.as_str() == "object")),
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
                            parameter_annotations: method
                                .parameters
                                .iter_source_order()
                                .filter_map(|parameter| {
                                    let parameter = match parameter {
                                        AnyParameterRef::NonVariadic(parameter) => {
                                            &parameter.parameter
                                        }
                                        AnyParameterRef::Variadic(parameter) => parameter,
                                    };
                                    parameter.annotation().map(|annotation| {
                                        (
                                            parameter.name.to_string(),
                                            expression(annotation, &mut expression_budget, 0),
                                        )
                                    })
                                })
                                .collect(),
                            signature: CallSignature::from_parameters(&method.parameters),
                            returns: method
                                .returns
                                .as_deref()
                                .map(|value| expression(value, &mut expression_budget, 0)),
                            decorators: decorators(&method.decorator_list, &mut expression_budget),
                            must_use_decorator: decorators_are_only_must_use(
                                &method.decorator_list,
                                &must_use_names,
                            ),
                            has_parameter_annotations: method
                                .parameters
                                .iter_source_order()
                                .any(|parameter| parameter.annotation().is_some()),
                            range: source_range(method),
                            body: flow_statements(&method.body, &mut expression_budget),
                            is_async: method.is_async,
                            is_generator: contains_yield(&method.body),
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
                    must_use_decorator: decorators_are_only_must_use(
                        &function.decorator_list,
                        &must_use_names,
                    ),
                    is_async: function.is_async,
                    is_generator: contains_yield(&function.body),
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
                        unknown_exception_bindings: module_binding_uncertainty(statement).1,
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
                        unknown_exception_bindings: module_binding_uncertainty(statement).1,
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
            Stmt::Expr(expr) => declarations.push(Declaration::Expression {
                value: expression(&expr.value, &mut expression_budget, 0),
                range: source_range(expr),
            }),
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
            _ => {
                let (wildcard_import, unknown_exception_bindings) =
                    module_binding_uncertainty(statement);
                declarations.push(Declaration::Unsupported {
                    message: if wildcard_import {
                        MODULE_STAR_IMPORT_UNCERTAINTY
                    } else {
                        "module-level control flow and executable statements are unsupported"
                    },
                    unknown_exception_bindings,
                    range: source_range(statement),
                });
            }
        }
        if expression_budget.exceeded {
            declarations.push(Declaration::Unsupported {
                message:
                    "AST expression exceeds the 128-level or 50,000-node per-statement work limit",
                unknown_exception_bindings: all_exception_binding_names(),
                range: source_range(statement),
            });
        }
        update_must_use_names(&mut must_use_names, statement);
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

/// Return a source view with CodePact policy comments removed. These comments
/// affect report disposition, not semantic analysis, so keeping them out of
/// Salsa semantic inputs lets policy-only edits reuse parsed and flow facts.
/// Lexer token ranges keep `#` inside strings untouched. Newlines and code
/// columns are preserved; absolute parser offsets are private to this view.
pub(crate) fn semantic_source(source: &str) -> String {
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY313),
    );
    let mut semantic = String::with_capacity(source.len());
    let mut offset = 0usize;
    for token in parsed.tokens() {
        if token.kind() != TokenKind::Comment {
            continue;
        }
        let start = token.start().to_usize();
        let end = token.end().to_usize();
        let comment = &source[start..end];
        if !comment
            .strip_prefix('#')
            .unwrap_or(comment)
            .trim_start()
            .starts_with("codepact:")
        {
            continue;
        }
        let prefix = &source[offset..start];
        let line_start = prefix.rfind(['\n', '\r']).map_or(0, |index| index + 1);
        let code_end = line_start + prefix[line_start..].trim_end_matches([' ', '\t']).len();
        semantic.push_str(&prefix[..code_end]);
        semantic.push_str(
            &source[start..end]
                .chars()
                .filter(|character| matches!(character, '\n' | '\r'))
                .collect::<String>(),
        );
        offset = end;
    }
    semantic.push_str(&source[offset..]);
    semantic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_source_removes_only_lexer_identified_codepact_comments() {
        let source = concat!(
            "literal = '# codepact: ignore[TP3001] -- text'\n",
            "formatted = f\"{'# codepact: ignore[TP3001] -- text'}\"\n",
            "fetch()  # codepact: ignore[TP3001] -- café\r\n",
            "# codepact: malformed policy\r\n",
            "# ordinary comment stays\r\n",
        );
        let semantic = semantic_source(source);

        assert_eq!(
            semantic.bytes().filter(|byte| *byte == b'\n').count(),
            source.bytes().filter(|byte| *byte == b'\n').count()
        );
        assert_eq!(
            semantic,
            concat!(
                "literal = '# codepact: ignore[TP3001] -- text'\n",
                "formatted = f\"{'# codepact: ignore[TP3001] -- text'}\"\n",
                "fetch()\r\n",
                "\r\n",
                "# ordinary comment stays\r\n",
            )
        );
        assert!(semantic.contains("literal = '# codepact: ignore[TP3001] -- text'"));
        assert!(semantic.contains("# codepact: ignore[TP3001] -- text"));
    }

    fn snapshot(statements: &[FlowStatement], depth: usize, lines: &mut Vec<String>) {
        let indent = "  ".repeat(depth);
        for statement in statements {
            match statement {
                FlowStatement::If { range, body, .. } => {
                    lines.push(format!("{indent}if [{}..{}]", range.start, range.end));
                    lines.push(format!("{indent}  then:"));
                    snapshot(body, depth + 2, lines);
                }
                FlowStatement::Try {
                    range,
                    body,
                    finalbody,
                    ..
                } => {
                    lines.push(format!("{indent}try [{}..{}]", range.start, range.end));
                    lines.push(format!("{indent}  body:"));
                    snapshot(body, depth + 2, lines);
                    if !finalbody.is_empty() {
                        lines.push(format!("{indent}  finally (pending completion preserved):"));
                        snapshot(finalbody, depth + 2, lines);
                    }
                }
                FlowStatement::Loop {
                    range, test, body, ..
                } => {
                    lines.push(format!(
                        "{indent}{} [{}..{}]",
                        if test.is_some() {
                            "while (backedge)"
                        } else {
                            "for (backedge)"
                        },
                        range.start,
                        range.end
                    ));
                    lines.push(format!("{indent}  body:"));
                    snapshot(body, depth + 2, lines);
                }
                FlowStatement::With {
                    range,
                    body,
                    asynchronous,
                    ..
                } => {
                    lines.push(format!(
                        "{indent}{} [{}..{}] (cleanup frame)",
                        if *asynchronous { "async with" } else { "with" },
                        range.start,
                        range.end
                    ));
                    lines.push(format!("{indent}  body:"));
                    snapshot(body, depth + 2, lines);
                }
                FlowStatement::Return { range, .. } => {
                    lines.push(format!("{indent}return [{}..{}]", range.start, range.end));
                }
                FlowStatement::Raise { range, .. } => {
                    lines.push(format!("{indent}raise [{}..{}]", range.start, range.end));
                }
                FlowStatement::Break { range } => {
                    lines.push(format!(
                        "{indent}break -> loop exit [{}..{}]",
                        range.start, range.end
                    ));
                }
                FlowStatement::Continue { range } => {
                    lines.push(format!(
                        "{indent}continue -> loop backedge [{}..{}]",
                        range.start, range.end
                    ));
                }
                FlowStatement::Expression { value, range }
                    if matches!(value.kind, AstExprKind::Await(_)) =>
                {
                    lines.push(format!(
                        "{indent}await -> suspension [{}..{}]",
                        range.start, range.end
                    ));
                }
                _ => {}
            }
        }
    }

    #[test]
    fn cleanup_control_flow_frontend_snapshot_keeps_exits_backedge_and_unwind_frame() {
        let source = "async def transfer(flag, manager):\n    try:\n        while flag:\n            if flag:\n                return manager\n            if not flag:\n                raise RuntimeError()\n            if flag:\n                break\n            continue\n    finally:\n        async with manager:\n            await manager.close()\n";
        let parsed = parse_frontend(source, "fixture", "3.11").expect("fixture parses");
        let body = parsed
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Function {
                    name,
                    body,
                    is_async,
                    ..
                } if name == "transfer" => {
                    assert!(*is_async);
                    Some(body)
                }
                _ => None,
            })
            .expect("transfer function is present");
        let mut lines = Vec::new();
        snapshot(body, 0, &mut lines);
        assert_eq!(
            lines.join("\n"),
            include_str!("../snapshots/phase03-control-flow.txt").trim_end()
        );
    }
}

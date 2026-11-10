use ruff_python_ast::{
    token::TokenKind,
    visitor::{walk_expr, walk_pattern, walk_stmt, Visitor},
    AnyParameterRef, Expr, Pattern, PythonVersion, Stmt,
};
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(crate) const MODULE_STAR_IMPORT_UNCERTAINTY: &str =
    "module-level wildcard import is unsupported";

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
    /// An expression outside the directly modeled subset, with its nested
    /// call targets retained for rules that need to report an honest boundary.
    OtherCalls(Vec<String>),
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
        annotation: Option<AstExpr>,
        value: AstExpr,
        range: SourceRange,
    },
    EscapingAssign {
        name: String,
        annotation: Option<AstExpr>,
        value: AstExpr,
        range: SourceRange,
    },
    AnnotatedDeclaration {
        name: Option<String>,
        annotation: AstExpr,
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
        cause: Option<AstExpr>,
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
    pub parameter_annotations: Vec<(String, AstExpr)>,
    pub signature: CallSignature,
    pub returns: Option<AstExpr>,
    pub decorators: Vec<AstExpr>,
    pub must_use_decorator: bool,
    pub has_parameter_annotations: bool,
    pub range: SourceRange,
    pub body: Vec<FlowStatement>,
    pub is_async: bool,
    pub is_generator: bool,
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
        class_annotations: Vec<AstExpr>,
        has_unresolved_base_classes: bool,
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
        must_use_decorator: bool,
        is_async: bool,
        is_generator: bool,
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
    Expression {
        value: AstExpr,
        range: SourceRange,
    },
    Unsupported {
        message: &'static str,
        unknown_exception_bindings: Vec<String>,
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
            AstExprKind::OtherCalls(collector.calls)
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

fn decorators_are_only_must_use(
    list: &[ruff_python_ast::Decorator],
    must_use_names: &std::collections::BTreeSet<String>,
) -> bool {
    !list.is_empty()
        && list.iter().all(|decorator| {
            dotted_name(&decorator.expression).is_some_and(|name| must_use_names.contains(&name))
        })
}

fn update_must_use_names(names: &mut std::collections::BTreeSet<String>, statement: &Stmt) {
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

fn flow_statements(statements: &[Stmt], budget: &mut ExpressionBudget) -> Vec<FlowStatement> {
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
                    _ => FlowStatement::Unsupported {
                        range: source_range(statement),
                    },
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

fn contains_yield(statements: &[Stmt]) -> bool {
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

fn definition_header_boundaries(statements: &[Stmt]) -> Vec<bool> {
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

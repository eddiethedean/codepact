use std::collections::BTreeSet;

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
    Yield(Option<Box<AstExpr>>),
    Tuple(Vec<AstExpr>),
    Collection(Vec<AstExpr>),
    Starred(Box<AstExpr>),
    Closure(BTreeSet<String>),
    /// An expression outside the directly modeled subset, with its nested
    /// call targets retained for rules that need to report an honest boundary.
    OtherCalls(Vec<String>, BTreeSet<String>),
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
    Match {
        subject: AstExpr,
        cases: Vec<FlowMatchCase>,
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
    Store {
        value: AstExpr,
        target: Option<AstExpr>,
        range: SourceRange,
    },
    ClosureDefinition {
        name: String,
        captures: BTreeSet<String>,
        range: SourceRange,
    },
    DeleteLocal {
        names: Vec<String>,
        range: SourceRange,
    },
    Unsupported {
        range: SourceRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlowMatchCase {
    pub pattern: AstPattern,
    pub guard: Option<AstExpr>,
    pub body: Vec<FlowStatement>,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AstPattern {
    Class {
        class: AstExpr,
        positional: Vec<AstPattern>,
        keywords: Vec<(String, AstPattern)>,
        range: SourceRange,
    },
    As {
        pattern: Option<Box<AstPattern>>,
        name: Option<String>,
        range: SourceRange,
    },
    Or {
        patterns: Vec<AstPattern>,
        range: SourceRange,
    },
    Capture {
        name: Option<String>,
        range: SourceRange,
    },
    Unsupported {
        bindings: Vec<String>,
        range: SourceRange,
    },
}

impl AstPattern {
    pub(crate) fn binding_names(&self, names: &mut std::collections::BTreeSet<String>) {
        match self {
            Self::Class {
                positional,
                keywords,
                ..
            } => {
                for child in positional {
                    child.binding_names(names);
                }
                for (_, child) in keywords {
                    child.binding_names(names);
                }
            }
            Self::As { pattern, name, .. } => {
                names.extend(name.iter().cloned());
                if let Some(child) = pattern {
                    child.binding_names(names);
                }
            }
            Self::Capture { name, .. } => names.extend(name.iter().cloned()),
            Self::Or { patterns, .. } => {
                for child in patterns {
                    child.binding_names(names);
                }
            }
            Self::Unsupported { bindings, .. } => names.extend(bindings.iter().cloned()),
        }
    }

    pub(crate) fn range(&self) -> SourceRange {
        match self {
            Self::Class { range, .. }
            | Self::As { range, .. }
            | Self::Or { range, .. }
            | Self::Capture { range, .. }
            | Self::Unsupported { range, .. } => *range,
        }
    }
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
        bound_methods: BTreeSet<String>,
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
    ModuleFlow {
        statements: Vec<FlowStatement>,
        unknown_exception_bindings: Vec<String>,
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

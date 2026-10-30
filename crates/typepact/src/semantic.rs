use crate::{
    db::ParseSummary,
    frontend::{AstExpr, AstExprKind, Declaration, ImportName, SourceRange},
    Access, Contract, Coverage, Diagnostic, Inspection, Lifecycle, Provenance, SourceFile, Span,
    DEFAULT_MAX_DECLARATIONS, SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ClassDecl {
    closed_by: Option<String>,
    closed_by_span: Option<Span>,
}

#[derive(Clone, Debug)]
struct ParsedAnnotation {
    base: Option<String>,
    access: Access,
    origin: &'static str,
    has_resource: bool,
    closed_by: Option<String>,
    access_span: Option<Span>,
    closed_by_span: Option<Span>,
    conflict: Option<&'static str>,
    conflict_spans: Vec<Span>,
    unsupported_reason: Option<&'static str>,
}

impl ParsedAnnotation {
    fn plain(base: Option<String>) -> Self {
        Self {
            base,
            access: Access::Unspecified,
            origin: "python-annotation",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: None,
        }
    }
}

struct SemanticProject {
    inspection: Inspection,
    module_exports: BTreeMap<String, BTreeMap<String, ExportedView>>,
    symbols: BTreeMap<String, ClassDecl>,
}

fn inspect_inner(
    files: &[SourceFile],
    python_version: &str,
    prepared: &BTreeMap<String, ParseSummary>,
    module_context: Option<&ModuleContext>,
    mut check_cancelled: impl FnMut(),
) -> SemanticProject {
    let mut result = Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: Vec::new(),
        coverage: Coverage {
            status: "complete-for-supported-subset".into(),
            reasons: Vec::new(),
            source_bytes: files.iter().fold(0usize, |total, file| {
                total.saturating_add(file.source.len())
            }),
            declarations: 0,
        },
    };
    if module_context.is_none() && !matches!(python_version, "3.11" | "3.12" | "3.13") {
        result.coverage.status = "incomplete".into();
        result.coverage.reasons.push(format!(
            "unsupported Python target version {python_version}; supported targets are 3.11–3.13"
        ));
    }

    let ordered = order_project_files(
        files,
        |path| {
            prepared
                .get(path)
                .map(|summary| summary.absolute_imports.as_slice())
        },
        &mut result,
    );
    let mut module_counts = BTreeMap::<String, usize>::new();
    for file in files {
        *module_counts.entry(file.module.clone()).or_default() += 1;
    }
    let supplied_modules: BTreeSet<_> = module_counts
        .iter()
        .filter_map(|(module, count)| (*count == 1).then_some(module.clone()))
        .collect();
    let ambiguous_modules: BTreeSet<_> = module_counts
        .into_iter()
        .filter_map(|(module, count)| (count > 1).then_some(module))
        .collect();

    let mut supplied_modules = supplied_modules;
    let mut ambiguous_modules = ambiguous_modules;
    if let Some(context) = module_context {
        supplied_modules.extend(context.supplied_modules.iter().cloned());
        ambiguous_modules.extend(context.ambiguous_modules.iter().cloned());
    }
    let mut symbols = module_context
        .into_iter()
        .flat_map(|context| context.dependency_exports.values())
        .flat_map(|exports| {
            exports
                .classes
                .iter()
                .map(|(symbol, decl)| (symbol.clone(), decl.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut bindings = BTreeMap::<String, String>::new();
    let mut module_exports = module_context
        .into_iter()
        .flat_map(|context| context.dependency_exports.iter())
        .map(|(module, exports)| (module.clone(), exports.names.clone()))
        .collect::<BTreeMap<_, _>>();
    for index in ordered {
        let file = &files[index];
        check_cancelled();
        if ambiguous_modules.contains(&file.module) {
            continue;
        }
        let Some(summary) = prepared.get(&file.path) else {
            unsupported(
                &mut result,
                file,
                SourceRange { start: 0, end: 0 },
                "Ruff AST extraction returned no declaration stream",
            );
            continue;
        };
        if !summary.ok {
            syntax_error(&mut result, file, summary);
            continue;
        }
        let Some(declarations) = summary.declarations.as_ref() else {
            unsupported(
                &mut result,
                file,
                SourceRange { start: 0, end: 0 },
                "Ruff AST extraction returned no declaration stream",
            );
            continue;
        };
        let mut state = FileState::new(&file.module);
        state.future_annotations = declarations.iter().any(|declaration| {
            matches!(
                declaration,
                Declaration::ImportFrom {
                    module,
                    level: 0,
                    names,
                    ..
                } if module == "__future__" && names.iter().any(|name| name.name == "annotations")
            )
        });
        let mut stop = false;
        for (index, declaration) in declarations.iter().enumerate() {
            if index % 64 == 0 {
                check_cancelled();
            }
            if result.coverage.declarations >= DEFAULT_MAX_DECLARATIONS {
                unsupported(
                    &mut result,
                    file,
                    declaration.range(),
                    "contract limit reached; remaining source was not inspected",
                );
                stop = true;
                break;
            }
            match declaration {
                Declaration::ImportFrom {
                    module,
                    level,
                    names,
                    range,
                } => {
                    if *level == 0
                        && module == "__future__"
                        && names.iter().any(|name| name.name == "annotations")
                    {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "future annotations are not resolved in 0.1",
                        );
                    } else {
                        process_import_from(
                            &mut result,
                            file,
                            module,
                            *level,
                            names,
                            *range,
                            &supplied_modules,
                            &ambiguous_modules,
                            &symbols,
                            &module_exports,
                            &mut bindings,
                            &mut state,
                        );
                    }
                }
                Declaration::Import { names, range } => {
                    for item in names {
                        let local = item
                            .alias
                            .as_deref()
                            .unwrap_or_else(|| item.name.split('.').next().unwrap_or(&item.name));
                        state.clear_name(local, &file.module, &mut bindings);
                    }
                    unsupported(
                        &mut result,
                        file,
                        *range,
                        "module imports are not resolved in 0.1",
                    );
                }
                Declaration::Delete { names, range } => {
                    for name in names {
                        if let Some(name) = name {
                            state.clear_name(name, &file.module, &mut bindings);
                        } else {
                            unsupported(
                                &mut result,
                                file,
                                *range,
                                "delete targets other than simple names are unsupported",
                            );
                        }
                    }
                }
                Declaration::Class {
                    name,
                    decorators,
                    unsupported: has_unsupported_syntax,
                    range,
                    ..
                } => {
                    if *has_unsupported_syntax {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "Ruff AST marked syntax outside the supported declaration subset (class bases, keywords, and type parameters are not resolved)",
                        );
                    }
                    lower_class(
                        &mut result,
                        file,
                        name,
                        decorators,
                        &mut state,
                        &mut symbols,
                        &mut bindings,
                    );
                }
                Declaration::Assignment {
                    name, value, range, ..
                } => {
                    if let Some(name) = name {
                        lower_assignment(
                            &mut result,
                            file,
                            name,
                            value.as_ref(),
                            &mut state,
                            &symbols,
                            &mut bindings,
                        );
                    } else {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "assignment target is outside the supported module-level subset",
                        );
                    }
                }
                Declaration::TypeAlias { name, range } => {
                    if let Some(name) = name {
                        state.clear_name(name, &file.module, &mut bindings);
                    }
                    unsupported(
                        &mut result,
                        file,
                        *range,
                        "Python type-alias statements are not resolved in 0.1",
                    );
                    state.clear_module(&file.module, &mut bindings);
                }
                Declaration::Function {
                    name,
                    parameters,
                    returns,
                    decorators,
                    unsupported: has_unsupported_syntax,
                    range,
                    ..
                } => {
                    if *has_unsupported_syntax {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "nested or generic function declarations are unsupported",
                        );
                    }
                    if !decorators.is_empty() {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "decorated functions are unresolved in 0.1",
                        );
                        state.clear_module(&file.module, &mut bindings);
                    } else {
                        if !state.future_annotations {
                            for parameter in parameters {
                                if let Some(annotation) = &parameter.annotation {
                                    lower_function_annotation(
                                        &mut result,
                                        file,
                                        annotation,
                                        &format!("{name}.{}", parameter.name),
                                        &state,
                                        &symbols,
                                        &bindings,
                                    );
                                }
                            }
                            if let Some(annotation) = returns {
                                lower_function_annotation(
                                    &mut result,
                                    file,
                                    annotation,
                                    &format!("{name}.return"),
                                    &state,
                                    &symbols,
                                    &bindings,
                                );
                            }
                        }
                        // Python evaluates eager annotations before installing the new function binding.
                        state.clear_name(name, &file.module, &mut bindings);
                    }
                }
                Declaration::Unsupported { message, range } => {
                    unsupported(&mut result, file, *range, message);
                    state.clear_module(&file.module, &mut bindings);
                }
            }
        }
        if !stop {
            module_exports.insert(file.module.clone(), state.exports.clone());
        } else {
            break;
        }
    }

    result.contracts.sort_by(|left, right| {
        (
            &left.source.file,
            left.source.line,
            left.source.column,
            &left.base_symbol,
        )
            .cmp(&(
                &right.source.file,
                right.source.line,
                right.source.column,
                &right.base_symbol,
            ))
    });
    result.diagnostics.sort_by(|left, right| {
        (
            &left.primary.file,
            left.primary.line,
            left.primary.column,
            &left.code,
        )
            .cmp(&(
                &right.primary.file,
                right.primary.line,
                right.primary.column,
                &right.code,
            ))
    });
    result.coverage.reasons.sort();
    result.coverage.reasons.dedup();
    SemanticProject {
        inspection: result,
        module_exports,
        symbols,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ExportedView {
    symbol: String,
    access: Access,
    closed_by: Option<String>,
    access_span: Option<Span>,
    closed_by_span: Option<Span>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SemanticExports {
    classes: BTreeMap<String, ClassDecl>,
    names: BTreeMap<String, ExportedView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ModuleAnalysis {
    pub inspection: Inspection,
    pub exports: SemanticExports,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ModuleContext {
    dependency_exports: BTreeMap<String, SemanticExports>,
    supplied_modules: BTreeSet<String>,
    ambiguous_modules: BTreeSet<String>,
}

pub(crate) struct ProjectPlan {
    pub inspection: Inspection,
    pub order: Vec<usize>,
    module_counts: BTreeMap<String, usize>,
}

pub(crate) fn plan_project(
    files: &[SourceFile],
    python_version: &str,
    prepared: &BTreeMap<String, &ParseSummary>,
) -> ProjectPlan {
    let mut inspection = Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: Vec::new(),
        coverage: Coverage {
            status: "complete-for-supported-subset".into(),
            reasons: Vec::new(),
            source_bytes: files.iter().fold(0usize, |total, file| {
                total.saturating_add(file.source.len())
            }),
            declarations: 0,
        },
    };
    if !matches!(python_version, "3.11" | "3.12" | "3.13") {
        inspection.coverage.status = "incomplete".into();
        inspection.coverage.reasons.push(format!(
            "unsupported Python target version {python_version}; supported targets are 3.11–3.13"
        ));
    }
    let mut module_counts = BTreeMap::<String, usize>::new();
    for file in files {
        *module_counts.entry(file.module.clone()).or_default() += 1;
    }
    let order = order_project_files(
        files,
        |path| {
            prepared
                .get(path)
                .map(|summary| summary.absolute_imports.as_slice())
        },
        &mut inspection,
    );
    ProjectPlan {
        inspection,
        order,
        module_counts,
    }
}

pub(crate) fn context_for_module(
    file: &SourceFile,
    summary: &ParseSummary,
    plan: &ProjectPlan,
    computed_exports: &BTreeMap<String, SemanticExports>,
) -> ModuleContext {
    let mut context = ModuleContext::default();
    let retain_origins = summary_uses_contract_wrapper(summary);
    for module in &summary.absolute_imports {
        match plan.module_counts.get(module).copied() {
            Some(1) => {
                context.supplied_modules.insert(module.clone());
                if let Some(exports) = computed_exports.get(module) {
                    let mut exports = exports.clone();
                    if !retain_origins {
                        for view in exports.names.values_mut() {
                            view.access_span = None;
                            view.closed_by_span = None;
                        }
                    }
                    context.dependency_exports.insert(module.clone(), exports);
                }
            }
            Some(_) => {
                context.ambiguous_modules.insert(module.clone());
            }
            None => {}
        }
    }
    match plan.module_counts.get(&file.module).copied() {
        Some(1) => {
            context.supplied_modules.insert(file.module.clone());
        }
        Some(_) => {
            context.ambiguous_modules.insert(file.module.clone());
        }
        None => {}
    }
    context
}

fn summary_uses_contract_wrapper(summary: &ParseSummary) -> bool {
    fn expression_uses_wrapper(expression: &AstExpr, imports: &BTreeMap<String, String>) -> bool {
        match &expression.kind {
            AstExprKind::Subscript(container, arguments) => {
                canonical(container, imports).is_some_and(|identity| {
                    matches!(
                        identity.as_str(),
                        "typing::Annotated"
                            | "typepact::Borrowed"
                            | "typepact::Owned"
                            | "typepact::BorrowedMut"
                    )
                }) || expression_uses_wrapper(container, imports)
                    || arguments
                        .iter()
                        .any(|argument| expression_uses_wrapper(argument, imports))
            }
            AstExprKind::Attribute(value, _) => expression_uses_wrapper(value, imports),
            AstExprKind::Call {
                function,
                arguments,
                keywords,
            } => {
                expression_uses_wrapper(function, imports)
                    || arguments
                        .iter()
                        .any(|argument| expression_uses_wrapper(argument, imports))
                    || keywords
                        .iter()
                        .any(|(_, value)| expression_uses_wrapper(value, imports))
            }
            AstExprKind::Tuple(values) => values
                .iter()
                .any(|value| expression_uses_wrapper(value, imports)),
            _ => false,
        }
    }

    let Some(declarations) = summary.declarations.as_ref() else {
        return false;
    };
    let mut state = FileState::default();
    let mut bindings = BTreeMap::new();
    for declaration in declarations {
        let uses_wrapper = match declaration {
            Declaration::Class { decorators, .. } => decorators
                .iter()
                .any(|expr| expression_uses_wrapper(expr, &state.imports)),
            Declaration::Function {
                decorators,
                parameters,
                returns,
                ..
            } => {
                decorators
                    .iter()
                    .any(|expr| expression_uses_wrapper(expr, &state.imports))
                    || parameters.iter().any(|parameter| {
                        parameter.annotation.as_ref().is_some_and(|annotation| {
                            expression_uses_wrapper(annotation, &state.imports)
                        })
                    })
                    || returns.as_ref().is_some_and(|annotation| {
                        expression_uses_wrapper(annotation, &state.imports)
                    })
            }
            Declaration::Assignment { value, .. } => value
                .as_ref()
                .is_some_and(|expr| expression_uses_wrapper(expr, &state.imports)),
            _ => false,
        };
        if uses_wrapper {
            return true;
        }

        match declaration {
            Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } if module == "__future__" && names.iter().any(|name| name.name == "annotations") => {}
            Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } => {
                for name in names {
                    let local = name.alias.as_deref().unwrap_or(&name.name);
                    state.clear_name(local, "", &mut bindings);
                    register_import(&name.name, local, module, &mut state);
                }
            }
            Declaration::ImportFrom { names, .. } => {
                for name in names {
                    let local = name.alias.as_deref().unwrap_or(&name.name);
                    state.clear_name(local, "", &mut bindings);
                }
            }
            Declaration::Import { names, .. } => {
                for name in names {
                    let local = name
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| name.name.split('.').next().unwrap_or(&name.name));
                    state.clear_name(local, "", &mut bindings);
                }
            }
            Declaration::Class { name, .. } | Declaration::Function { name, .. } => {
                state.clear_name(name, "", &mut bindings);
            }
            Declaration::Assignment {
                name,
                value: Some(_),
                ..
            } => {
                if let Some(name) = name {
                    state.clear_name(name, "", &mut bindings);
                }
            }
            Declaration::Delete { names, .. } => {
                for name in names.iter().flatten() {
                    state.clear_name(name, "", &mut bindings);
                }
            }
            Declaration::TypeAlias { .. } | Declaration::Unsupported { .. } => {
                state.clear_module("", &mut bindings);
            }
            Declaration::Assignment { .. } => {}
        }
    }
    false
}

pub(crate) fn inspect_module(
    file: &SourceFile,
    python_version: &str,
    summary: &ParseSummary,
    context: &ModuleContext,
    check_cancelled: impl FnMut(),
) -> ModuleAnalysis {
    let prepared = BTreeMap::from([(file.path.clone(), summary.clone())]);
    let project = inspect_inner(
        std::slice::from_ref(file),
        python_version,
        &prepared,
        Some(context),
        check_cancelled,
    );
    let names = project
        .module_exports
        .get(&file.module)
        .cloned()
        .unwrap_or_default();
    let classes = names
        .values()
        .filter_map(|view| {
            project.symbols.get(&view.symbol).map(|class| {
                (
                    view.symbol.clone(),
                    ClassDecl {
                        closed_by: class.closed_by.clone(),
                        // Source locations are local diagnostics, not part
                        // of the semantic export queried by dependents.
                        closed_by_span: None,
                    },
                )
            })
        })
        .collect();
    ModuleAnalysis {
        inspection: project.inspection,
        exports: SemanticExports { classes, names },
    }
}

pub(crate) fn combine_project(
    mut result: Inspection,
    modules: impl IntoIterator<Item = Inspection>,
) -> Inspection {
    for mut module in modules {
        if module.coverage.status == "incomplete" {
            result.coverage.status = "incomplete".into();
        }
        result.coverage.reasons.append(&mut module.coverage.reasons);
        result.diagnostics.append(&mut module.diagnostics);
        result.contracts.append(&mut module.contracts);
    }
    result.contracts.sort_by(|left, right| {
        (
            &left.source.file,
            left.source.line,
            left.source.column,
            &left.base_symbol,
        )
            .cmp(&(
                &right.source.file,
                right.source.line,
                right.source.column,
                &right.base_symbol,
            ))
    });
    if result.contracts.len() > DEFAULT_MAX_DECLARATIONS {
        let overflow = result.contracts[DEFAULT_MAX_DECLARATIONS].source.clone();
        result.contracts.truncate(DEFAULT_MAX_DECLARATIONS);
        result.coverage.status = "incomplete".into();
        result.coverage.reasons.push(format!(
            "{}:{}: project contract limit reached; remaining contracts were not emitted",
            overflow.file, overflow.line
        ));
        result.diagnostics.push(Diagnostic {
            code: "TP9001".into(),
            severity: "warning".into(),
            message: "project contract limit reached; remaining contracts were not emitted".into(),
            primary: overflow,
            related: Vec::new(),
            certainty: "unknown".into(),
        });
    }
    result.coverage.declarations = result.contracts.len();
    result.diagnostics.sort_by(|left, right| {
        (
            &left.primary.file,
            left.primary.line,
            left.primary.column,
            &left.code,
        )
            .cmp(&(
                &right.primary.file,
                right.primary.line,
                right.primary.column,
                &right.code,
            ))
    });
    result.coverage.reasons.sort();
    result.coverage.reasons.dedup();
    result
}

#[derive(Default)]
struct FileState {
    future_annotations: bool,
    imports: BTreeMap<String, String>,
    unresolved_imports: BTreeMap<String, UnresolvedImport>,
    aliases: BTreeMap<String, String>,
    alias_lifecycle: BTreeMap<String, (String, Span)>,
    alias_access: BTreeMap<String, (Access, Span)>,
    exports: BTreeMap<String, ExportedView>,
}

#[derive(Clone)]
struct UnresolvedImport {
    module: String,
    span: Span,
}

impl FileState {
    fn new(_module: &str) -> Self {
        Self::default()
    }

    fn clear_name(&mut self, name: &str, module: &str, bindings: &mut BTreeMap<String, String>) {
        self.imports.remove(name);
        self.unresolved_imports.remove(name);
        self.aliases.remove(name);
        self.alias_lifecycle.remove(name);
        self.alias_access.remove(name);
        self.exports.remove(name);
        bindings.remove(&format!("{module}.{name}"));
    }

    fn clear_module(&mut self, module: &str, bindings: &mut BTreeMap<String, String>) {
        let prefix = format!("{module}.");
        bindings.retain(|name, _| !name.starts_with(&prefix));
        self.imports.clear();
        self.unresolved_imports.clear();
        self.aliases.clear();
        self.alias_lifecycle.clear();
        self.alias_access.clear();
        self.exports.clear();
    }
}

impl Declaration {
    fn range(&self) -> SourceRange {
        match self {
            Self::ImportFrom { range, .. }
            | Self::Import { range, .. }
            | Self::Class { range, .. }
            | Self::Function { range, .. }
            | Self::Assignment { range, .. }
            | Self::Delete { range, .. }
            | Self::TypeAlias { range, .. }
            | Self::Unsupported { range, .. } => *range,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_import_from(
    result: &mut Inspection,
    file: &SourceFile,
    module: &str,
    level: u32,
    names: &[ImportName],
    range: SourceRange,
    supplied_modules: &BTreeSet<String>,
    ambiguous_modules: &BTreeSet<String>,
    symbols: &BTreeMap<String, ClassDecl>,
    module_exports: &BTreeMap<String, BTreeMap<String, ExportedView>>,
    bindings: &mut BTreeMap<String, String>,
    state: &mut FileState,
) {
    if level != 0 {
        for item in names {
            let local = item.alias.as_deref().unwrap_or(&item.name);
            state.clear_name(local, &file.module, bindings);
        }
        unsupported(
            result,
            file,
            range,
            "relative imports are unsupported; use explicit absolute module inputs",
        );
        return;
    }
    if names.len() == 1 && names[0].name == "*" {
        unsupported(result, file, range, "star imports are unsupported");
        state.clear_module(&file.module, bindings);
        return;
    }
    for item in names {
        let local = item.alias.as_deref().unwrap_or(&item.name);
        state.clear_name(local, &file.module, bindings);
        register_import(&item.name, local, module, state);
        register_supplied_module_import(
            result,
            file,
            module,
            &item.name,
            local,
            item.range,
            supplied_modules,
            ambiguous_modules,
            symbols,
            module_exports,
            bindings,
            state,
        );
    }
}

fn register_import(original: &str, local: &str, module: &str, state: &mut FileState) {
    let canonical = match format!("{module}.{original}").as_str() {
        "typing.Annotated" => "typing::Annotated",
        "typepact.resource" => "typepact::resource",
        "typepact.Resource" | "typepact.contracts.Resource" => "typepact::contracts::Resource",
        "typepact.contracts.SharedBorrow" => "typepact::contracts::SharedBorrow",
        "typepact.contracts.OwnedAccess" => "typepact::contracts::OwnedAccess",
        "typepact.contracts.ExclusiveBorrow" => "typepact::contracts::ExclusiveBorrow",
        "typepact.contracts.Borrowed" | "typepact.rust.Borrowed" => "typepact::Borrowed",
        "typepact.contracts.Owned" | "typepact.rust.Owned" => "typepact::Owned",
        "typepact.contracts.BorrowedMut" | "typepact.rust.BorrowedMut" => "typepact::BorrowedMut",
        _ => "",
    };
    state.imports.insert(local.to_owned(), canonical.to_owned());
}

#[allow(clippy::too_many_arguments)]
fn register_supplied_module_import(
    result: &mut Inspection,
    file: &SourceFile,
    module: &str,
    original: &str,
    local: &str,
    range: SourceRange,
    supplied_modules: &BTreeSet<String>,
    ambiguous_modules: &BTreeSet<String>,
    symbols: &BTreeMap<String, ClassDecl>,
    module_exports: &BTreeMap<String, BTreeMap<String, ExportedView>>,
    bindings: &mut BTreeMap<String, String>,
    state: &mut FileState,
) {
    if module == "typepact" || module.starts_with("typepact.") {
        return;
    }
    if ambiguous_modules.contains(module) {
        unsupported(
            result,
            file,
            range,
            &format!("import from ambiguous supplied module {module}"),
        );
        return;
    }
    if !supplied_modules.contains(module) {
        if !is_known_external_module(module) {
            state.unresolved_imports.insert(
                local.to_owned(),
                UnresolvedImport {
                    module: module.to_owned(),
                    span: crate::source_span(&file.path, &file.source, range),
                },
            );
        }
        return;
    }
    let Some(view) = module_exports
        .get(module)
        .and_then(|exports| exports.get(original))
        .cloned()
        .filter(|view| symbols.contains_key(&view.symbol))
    else {
        unsupported(
            result,
            file,
            range,
            &format!("supplied module {module} does not directly export class or supported alias {original}"),
        );
        return;
    };
    state.aliases.insert(local.to_owned(), view.symbol.clone());
    let import_span = crate::source_span(&file.path, &file.source, range);
    if view.access != Access::Unspecified {
        state.alias_access.insert(
            local.to_owned(),
            (
                view.access,
                view.access_span
                    .clone()
                    .unwrap_or_else(|| import_span.clone()),
            ),
        );
    }
    if let Some(closed_by) = view.closed_by {
        state.alias_lifecycle.insert(
            local.to_owned(),
            (closed_by, view.closed_by_span.unwrap_or(import_span)),
        );
    }
    bindings.insert(format!("{}.{local}", file.module), view.symbol);
}

fn is_known_external_module(module: &str) -> bool {
    matches!(
        module,
        "typing"
            | "typing_extensions"
            | "builtins"
            | "collections"
            | "collections.abc"
            | "dataclasses"
            | "datetime"
            | "decimal"
            | "enum"
            | "fractions"
            | "pathlib"
            | "types"
            | "uuid"
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_class(
    result: &mut Inspection,
    file: &SourceFile,
    name: &str,
    decorators: &[AstExpr],
    state: &mut FileState,
    symbols: &mut BTreeMap<String, ClassDecl>,
    bindings: &mut BTreeMap<String, String>,
) {
    let mut closed: Option<(String, Span)> = None;
    let mut conflict = false;
    let mut unsupported_decorator = None;
    for decorator in decorators {
        let (function, arguments, keywords) = match &decorator.kind {
            AstExprKind::Call {
                function,
                arguments,
                keywords,
            } => (function.as_ref(), arguments.as_slice(), keywords.as_slice()),
            _ => {
                if canonical(decorator, &state.imports).as_deref() == Some("typepact::resource") {
                    unsupported(
                        result,
                        file,
                        decorator.range,
                        "resource decorator requires a literal closed_by keyword",
                    );
                    unsupported_decorator.get_or_insert(decorator.range);
                } else {
                    unsupported_decorator.get_or_insert(decorator.range);
                }
                continue;
            }
        };
        if canonical(function, &state.imports).as_deref() != Some("typepact::resource") {
            unsupported_decorator.get_or_insert(decorator.range);
            continue;
        }
        let Some(method) = parse_resource_decorator_call(arguments, keywords) else {
            unsupported(
                result,
                file,
                decorator.range,
                "resource decorator supports only one literal closed_by keyword",
            );
            unsupported_decorator.get_or_insert(decorator.range);
            continue;
        };
        if !is_identifier(&method) {
            declaration_error(
                result,
                file,
                decorator.range,
                "TP4002",
                "closed_by must be a valid method identifier",
                Vec::new(),
            );
            conflict = true;
            continue;
        }
        let origin = crate::source_span(&file.path, &file.source, decorator.range);
        if let Some((previous, previous_span)) = &closed {
            if previous != &method {
                declaration_error(
                    result,
                    file,
                    decorator.range,
                    "TP4002",
                    "conflicting resource decorators",
                    vec![previous_span.clone()],
                );
                conflict = true;
            }
        } else {
            closed = Some((method, origin));
        }
    }
    state.clear_name(name, &file.module, bindings);
    if let Some(range) = unsupported_decorator {
        unsupported(
            result,
            file,
            range,
            "class decorator expression is outside the supported TypePact resource subset",
        );
        state.clear_module(&file.module, bindings);
        return;
    }
    if conflict {
        return;
    }
    let base = format!("{}.{}", file.module, name);
    let generation = symbols
        .keys()
        .filter(|symbol| symbol.starts_with(&format!("{base}#")))
        .count();
    let symbol = if symbols.contains_key(&base) {
        format!("{base}#{}", generation + 2)
    } else {
        base.clone()
    };
    symbols.insert(
        symbol.clone(),
        ClassDecl {
            closed_by: closed.as_ref().map(|(method, _)| method.clone()),
            closed_by_span: closed.as_ref().map(|(_, span)| span.clone()),
        },
    );
    bindings.insert(base.clone(), symbol.clone());
    state.exports.insert(
        name.to_owned(),
        ExportedView {
            symbol: symbol.clone(),
            access: Access::Unspecified,
            closed_by: closed.as_ref().map(|(method, _)| method.clone()),
            access_span: None,
            closed_by_span: closed.as_ref().map(|(_, span)| span.clone()),
        },
    );
    if let Some((method, origin)) = closed {
        add_contract(
            result,
            Contract {
                schema_version: 0,
                base_symbol: symbol,
                display_name: name.to_owned(),
                lifecycle: Some(Lifecycle { closed_by: method }),
                access: Access::Unspecified.as_str().into(),
                source: origin,
                provenance: vec![Provenance {
                    origin: "python-resource-decorator".into(),
                    trust: "declared-unverified".into(),
                }],
            },
        );
    }
}

fn parse_resource_decorator_call(
    arguments: &[AstExpr],
    keywords: &[(Option<String>, AstExpr)],
) -> Option<String> {
    if !arguments.is_empty() || keywords.len() != 1 {
        return None;
    }
    let (name, value) = keywords.first()?;
    if name.as_deref() != Some("closed_by") {
        return None;
    }
    match &value.kind {
        AstExprKind::String(method) => Some(method.clone()),
        _ => None,
    }
}

fn lower_assignment(
    result: &mut Inspection,
    file: &SourceFile,
    name: &str,
    expression: Option<&AstExpr>,
    state: &mut FileState,
    symbols: &BTreeMap<String, ClassDecl>,
    bindings: &mut BTreeMap<String, String>,
) {
    let Some(expression) = expression else {
        return;
    };
    if !is_alias_candidate(expression, &state.imports) {
        state.clear_name(name, &file.module, bindings);
        return;
    }
    let parsed = parse_annotation(expression, &state.imports, file);
    if report_annotation_problem(result, file, expression.range, &parsed) {
        state.clear_name(name, &file.module, bindings);
        return;
    }
    let Some(base) = parsed.base.as_deref() else {
        if parsed.has_resource {
            unsupported(
                result,
                file,
                expression.range,
                "Resource metadata requires a resolvable class type",
            );
        }
        state.clear_name(name, &file.module, bindings);
        return;
    };
    let Some(symbol) = resolve_symbol(base, &file.module, symbols, &state.aliases, bindings) else {
        if let Some(import) = state.unresolved_imports.get(base) {
            unsupported_related_import(
                result,
                file,
                expression.range,
                &format!(
                    "annotation type {base} comes from unresolved module {}",
                    import.module
                ),
                import.span.clone(),
            );
        } else if parsed.has_resource || parsed.access != Access::Unspecified {
            unsupported(
                result,
                file,
                expression.range,
                &format!("unresolved base type in contract alias: {base}"),
            );
        }
        state.clear_name(name, &file.module, bindings);
        return;
    };

    let inherited_access = state.alias_access.get(base).cloned();
    let inherited_lifecycle = state.alias_lifecycle.get(base).cloned().or_else(|| {
        symbols.get(&symbol).and_then(|declaration| {
            declaration
                .closed_by
                .as_ref()
                .zip(declaration.closed_by_span.as_ref())
                .map(|(closed_by, span)| (closed_by.clone(), span.clone()))
        })
    });
    let Some(metadata) = merge_annotation_metadata(
        result,
        file,
        expression.range,
        &parsed,
        inherited_access,
        inherited_lifecycle,
    ) else {
        state.clear_name(name, &file.module, bindings);
        return;
    };

    // Assignment evaluates its right-hand side in the old namespace, then
    // replaces the target binding. Resolve above before clearing this name.
    state.clear_name(name, &file.module, bindings);
    state.aliases.insert(name.to_owned(), symbol.clone());
    if metadata.access != Access::Unspecified {
        let access_span = metadata
            .access_span
            .clone()
            .unwrap_or_else(|| crate::source_span(&file.path, &file.source, expression.range));
        state
            .alias_access
            .insert(name.to_owned(), (metadata.access, access_span));
    }
    if let Some(closed_by) = metadata.closed_by.clone() {
        let lifecycle_span = metadata
            .closed_by_span
            .clone()
            .unwrap_or_else(|| crate::source_span(&file.path, &file.source, expression.range));
        state
            .alias_lifecycle
            .insert(name.to_owned(), (closed_by, lifecycle_span));
    }
    state.exports.insert(
        name.to_owned(),
        ExportedView {
            symbol: symbol.clone(),
            access: metadata.access,
            closed_by: metadata.closed_by.clone(),
            access_span: metadata.access_span.clone(),
            closed_by_span: metadata.closed_by_span.clone(),
        },
    );
    if parsed.has_resource {
        let closed_by = metadata
            .closed_by
            .clone()
            .expect("validated resource metadata has a lifecycle method");
        add_contract(
            result,
            Contract {
                schema_version: 0,
                base_symbol: symbol,
                display_name: name.to_owned(),
                lifecycle: Some(Lifecycle { closed_by }),
                access: metadata.access.as_str().into(),
                source: crate::source_span(&file.path, &file.source, expression.range),
                provenance: vec![Provenance {
                    origin: "annotated-resource-metadata".into(),
                    trust: "declared-unverified".into(),
                }],
            },
        );
    }
}

#[derive(Clone)]
struct ResolvedMetadata {
    access: Access,
    access_span: Option<Span>,
    closed_by: Option<String>,
    closed_by_span: Option<Span>,
}

fn merge_annotation_metadata(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    parsed: &ParsedAnnotation,
    inherited_access: Option<(Access, Span)>,
    inherited_lifecycle: Option<(String, Span)>,
) -> Option<ResolvedMetadata> {
    if parsed.has_resource && parsed.closed_by.is_none() {
        unsupported(
            result,
            file,
            range,
            "Resource metadata requires a literal closed_by method",
        );
        return None;
    }
    if parsed
        .closed_by
        .as_ref()
        .is_some_and(|closed_by| !is_identifier(closed_by))
    {
        declaration_error(
            result,
            file,
            range,
            "TP4002",
            "closed_by must be a valid method identifier",
            Vec::new(),
        );
        return None;
    }
    if let Some((inherited, inherited_span)) = &inherited_access {
        if parsed.access != Access::Unspecified && parsed.access != *inherited {
            metadata_conflict(
                result,
                parsed
                    .access_span
                    .clone()
                    .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range)),
                inherited_span.clone(),
                "conflicting inherited and explicit access qualifiers",
            );
            return None;
        }
    }
    if let (Some(closed_by), Some((inherited, inherited_span))) =
        (&parsed.closed_by, &inherited_lifecycle)
    {
        if closed_by != inherited {
            metadata_conflict(
                result,
                parsed
                    .closed_by_span
                    .clone()
                    .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range)),
                inherited_span.clone(),
                "conflicting inherited and explicit lifecycle declarations",
            );
            return None;
        }
    }

    let (access, access_span) = if parsed.access != Access::Unspecified {
        (parsed.access, parsed.access_span.clone())
    } else {
        match inherited_access {
            Some((access, span)) => (access, Some(span)),
            None => (Access::Unspecified, None),
        }
    };
    let (closed_by, closed_by_span) = if let Some(closed_by) = &parsed.closed_by {
        (Some(closed_by.clone()), parsed.closed_by_span.clone())
    } else {
        inherited_lifecycle
            .map(|(closed_by, span)| (Some(closed_by), Some(span)))
            .unwrap_or((None, None))
    };
    Some(ResolvedMetadata {
        access,
        access_span,
        closed_by,
        closed_by_span,
    })
}

fn metadata_conflict(result: &mut Inspection, primary: Span, related: Span, message: &str) {
    result.coverage.status = "incomplete".into();
    result.diagnostics.push(Diagnostic {
        code: "TP4002".into(),
        severity: "error".into(),
        message: message.into(),
        primary,
        related: vec![related],
        certainty: "definite".into(),
    });
}

fn is_alias_candidate(expression: &AstExpr, imports: &BTreeMap<String, String>) -> bool {
    match &expression.kind {
        AstExprKind::Name(_) | AstExprKind::Attribute(_, _) => true,
        AstExprKind::Subscript(container, _) => {
            canonical(container, imports).is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "typing::Annotated"
                        | "typepact::Borrowed"
                        | "typepact::Owned"
                        | "typepact::BorrowedMut"
                )
            })
        }
        _ => false,
    }
}

fn report_annotation_problem(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    parsed: &ParsedAnnotation,
) -> bool {
    if let Some(message) = parsed.conflict {
        let mut spans = parsed.conflict_spans.clone();
        let primary = spans
            .first()
            .cloned()
            .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range));
        if !spans.is_empty() {
            spans.remove(0);
        }
        result.coverage.status = "incomplete".into();
        result.diagnostics.push(Diagnostic {
            code: "TP4002".into(),
            severity: "error".into(),
            message: message.into(),
            primary,
            related: spans,
            certainty: "definite".into(),
        });
        return true;
    }
    if let Some(message) = parsed.unsupported_reason {
        unsupported(result, file, range, message);
        return true;
    }
    false
}

fn lower_function_annotation(
    result: &mut Inspection,
    file: &SourceFile,
    expression: &AstExpr,
    display_name: &str,
    state: &FileState,
    symbols: &BTreeMap<String, ClassDecl>,
    bindings: &BTreeMap<String, String>,
) {
    let parsed = parse_annotation(expression, &state.imports, file);
    if report_annotation_problem(result, file, expression.range, &parsed) {
        return;
    }
    let Some(base) = parsed.base.as_deref() else {
        if matches!(expression.kind, AstExprKind::String(_)) {
            unsupported(
                result,
                file,
                expression.range,
                "string and future annotations are not resolved in 0.1",
            );
        } else if parsed.has_resource || parsed.access != Access::Unspecified {
            unsupported(
                result,
                file,
                expression.range,
                "computed annotation base type is unsupported",
            );
        }
        return;
    };
    let Some(symbol) = resolve_symbol(base, &file.module, symbols, &state.aliases, bindings) else {
        if let Some(import) = state.unresolved_imports.get(base) {
            unsupported_related_import(
                result,
                file,
                expression.range,
                &format!(
                    "annotation type {base} comes from unresolved module {}",
                    import.module
                ),
                import.span.clone(),
            );
        } else if parsed.access != Access::Unspecified || parsed.has_resource {
            unsupported(
                result,
                file,
                expression.range,
                &format!("unresolved annotation base type {base}"),
            );
        }
        return;
    };
    let inherited_access = state.alias_access.get(base).cloned();
    let inherited_lifecycle = state.alias_lifecycle.get(base).cloned().or_else(|| {
        symbols.get(&symbol).and_then(|declaration| {
            declaration
                .closed_by
                .as_ref()
                .zip(declaration.closed_by_span.as_ref())
                .map(|(closed_by, span)| (closed_by.clone(), span.clone()))
        })
    });
    let Some(metadata) = merge_annotation_metadata(
        result,
        file,
        expression.range,
        &parsed,
        inherited_access,
        inherited_lifecycle,
    ) else {
        return;
    };
    let declaration = symbols.get(&symbol);
    let lifecycle = metadata.closed_by.map(|closed_by| Lifecycle { closed_by });
    let mut provenance = vec![Provenance {
        origin: parsed.origin.into(),
        trust: "declared-unverified".into(),
    }];
    if declaration.is_some_and(|declaration| declaration.closed_by.is_some()) {
        provenance.push(Provenance {
            origin: "python-resource-decorator".into(),
            trust: "declared-unverified".into(),
        });
    }
    add_contract(
        result,
        Contract {
            schema_version: 0,
            base_symbol: symbol,
            display_name: display_name.to_owned(),
            lifecycle,
            access: metadata.access.as_str().into(),
            source: crate::source_span(&file.path, &file.source, expression.range),
            provenance,
        },
    );
}

fn parse_annotation(
    expression: &AstExpr,
    imports: &BTreeMap<String, String>,
    file: &SourceFile,
) -> ParsedAnnotation {
    let AstExprKind::Subscript(container, arguments) = &expression.kind else {
        if let Some(path) = expression_path(expression) {
            if let Some(identity) = canonical(expression, imports) {
                let access = match identity.as_str() {
                    "typepact::Borrowed" => Access::Shared,
                    "typepact::Owned" => Access::Owned,
                    "typepact::BorrowedMut" => Access::Exclusive,
                    _ => Access::Unspecified,
                };
                if access != Access::Unspecified {
                    return ParsedAnnotation {
                        base: None,
                        access,
                        origin: "explicit-access-alias",
                        has_resource: false,
                        closed_by: None,
                        access_span: Some(crate::source_span(
                            &file.path,
                            &file.source,
                            expression.range,
                        )),
                        closed_by_span: None,
                        conflict: None,
                        conflict_spans: Vec::new(),
                        unsupported_reason: None,
                    };
                }
            }
            return ParsedAnnotation::plain(Some(path));
        }
        return ParsedAnnotation {
            base: None,
            access: Access::Unspecified,
            origin: "python-annotation",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: match expression.kind {
                AstExprKind::String(_) => {
                    Some("string and future annotations are not resolved in 0.1")
                }
                AstExprKind::Call { .. } | AstExprKind::Other => {
                    Some("computed annotation expressions are unsupported")
                }
                _ => None,
            },
        };
    };
    let Some(identity) = canonical(container, imports) else {
        return ParsedAnnotation::plain(None);
    };
    let base = arguments.first().and_then(expression_path);
    let access = match identity.as_str() {
        "typepact::Borrowed" => Access::Shared,
        "typepact::Owned" => Access::Owned,
        "typepact::BorrowedMut" => Access::Exclusive,
        _ => Access::Unspecified,
    };
    if access != Access::Unspecified {
        let access_span = crate::source_span(&file.path, &file.source, expression.range);
        let has_resolvable_base = base.is_some();
        return ParsedAnnotation {
            base,
            access,
            origin: "explicit-access-alias",
            has_resource: false,
            closed_by: None,
            access_span: Some(access_span),
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: if arguments.len() == 1 && has_resolvable_base {
                None
            } else {
                Some("access annotations require one resolvable base type")
            },
        };
    }
    if identity != "typing::Annotated" {
        return ParsedAnnotation::plain(None);
    }
    if arguments.len() < 2 {
        return ParsedAnnotation {
            base,
            access: Access::Unspecified,
            origin: "annotated",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: Some(
                "Annotated requires a base type and at least one metadata item",
            ),
        };
    }

    let mut access_values = Vec::<(Access, Span)>::new();
    let mut resources = Vec::<(Option<String>, Span)>::new();
    let mut unsupported_reason = None;
    for metadata in arguments.iter().skip(1) {
        let AstExprKind::Call {
            function,
            arguments: call_arguments,
            keywords,
        } = &metadata.kind
        else {
            continue;
        };
        let Some(identity) = canonical(function, imports) else {
            continue;
        };
        match identity.as_str() {
            "typepact::contracts::Resource" => {
                let closed_by = parse_resource_metadata_call(call_arguments, keywords);
                if closed_by.is_none() {
                    unsupported_reason =
                        Some("Resource metadata supports one literal closed_by value");
                }
                resources.push((
                    closed_by,
                    crate::source_span(&file.path, &file.source, metadata.range),
                ));
            }
            "typepact::contracts::SharedBorrow"
            | "typepact::contracts::OwnedAccess"
            | "typepact::contracts::ExclusiveBorrow" => {
                let access = match identity.as_str() {
                    "typepact::contracts::SharedBorrow" => Access::Shared,
                    "typepact::contracts::OwnedAccess" => Access::Owned,
                    _ => Access::Exclusive,
                };
                access_values.push((
                    access,
                    crate::source_span(&file.path, &file.source, metadata.range),
                ));
                if !call_arguments.is_empty() || !keywords.is_empty() {
                    unsupported_reason = Some("access metadata calls must not have arguments");
                }
            }
            _ => {}
        }
    }
    let base_is_missing = base.is_none();
    if base_is_missing {
        unsupported_reason = Some("Annotated requires a resolvable base type");
    }
    let access = access_values
        .first()
        .map(|(access, _)| *access)
        .unwrap_or(Access::Unspecified);
    let access_conflict = access_values
        .iter()
        .any(|(candidate, _)| *candidate != access);
    let resource_conflict = resources
        .first()
        .is_some_and(|first| resources.iter().any(|resource| resource.0 != first.0));
    let mut conflict_spans = Vec::new();
    let conflict = if access_conflict {
        conflict_spans.extend(access_values.iter().map(|(_, span)| span.clone()));
        Some("multiple incompatible access qualifiers")
    } else if resource_conflict {
        conflict_spans.extend(resources.iter().map(|(_, span)| span.clone()));
        Some("conflicting lifecycle declarations")
    } else {
        None
    };
    let closed_by = resources.first().and_then(|resource| resource.0.clone());
    let access_span = access_values.first().map(|(_, span)| span.clone());
    let closed_by_span = resources.first().map(|(_, span)| span.clone());
    ParsedAnnotation {
        base,
        access,
        origin: if access != Access::Unspecified {
            "annotated-access-metadata"
        } else if !resources.is_empty() {
            "annotated-resource-metadata"
        } else {
            "annotated"
        },
        has_resource: !resources.is_empty(),
        closed_by,
        access_span,
        closed_by_span,
        conflict,
        conflict_spans,
        unsupported_reason,
    }
}

fn parse_resource_metadata_call(
    arguments: &[AstExpr],
    keywords: &[(Option<String>, AstExpr)],
) -> Option<String> {
    match (arguments, keywords) {
        (
            [AstExpr {
                kind: AstExprKind::String(method),
                ..
            }],
            [],
        ) => Some(method.clone()),
        (
            [],
            [(
                Some(name),
                AstExpr {
                    kind: AstExprKind::String(method),
                    ..
                },
            )],
        ) if name == "closed_by" => Some(method.clone()),
        _ => None,
    }
}

fn expression_path(expression: &AstExpr) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(name.clone()),
        AstExprKind::Attribute(value, attr) => {
            Some(format!("{}.{}", expression_path(value)?, attr))
        }
        _ => None,
    }
}

fn canonical(expression: &AstExpr, imports: &BTreeMap<String, String>) -> Option<String> {
    let path = expression_path(expression)?;
    if let Some(value) = imports.get(&path).filter(|value| !value.is_empty()) {
        return Some(value.clone());
    }
    let (root, suffix) = path.split_once('.')?;
    let root_identity = imports.get(root).filter(|value| !value.is_empty())?;
    Some(format!("{root_identity}::{suffix}"))
}

fn resolve_symbol(
    base: &str,
    module: &str,
    symbols: &BTreeMap<String, ClassDecl>,
    aliases: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(symbol) = aliases.get(base) {
        return symbols.contains_key(symbol).then(|| symbol.clone());
    }
    let local = format!("{module}.{base}");
    bindings
        .get(&local)
        .filter(|symbol| symbols.contains_key(*symbol))
        .cloned()
}

fn order_project_files<'a>(
    files: &[SourceFile],
    absolute_imports_for: impl Fn(&str) -> Option<&'a [String]>,
    result: &mut Inspection,
) -> Vec<usize> {
    let mut modules = BTreeMap::<&str, Vec<usize>>::new();
    for (index, file) in files.iter().enumerate() {
        modules.entry(&file.module).or_default().push(index);
    }
    for (module, indexes) in &modules {
        if indexes.len() > 1 {
            for index in indexes {
                unsupported(
                    result,
                    &files[*index],
                    SourceRange { start: 0, end: 0 },
                    &format!("duplicate supplied module identity {module}"),
                );
            }
        }
    }
    let mut indegree = vec![0usize; files.len()];
    let mut dependents = vec![BTreeSet::new(); files.len()];
    let mut import_edges = 0usize;
    let mut edge_limit_reported = false;
    for (index, file) in files.iter().enumerate() {
        let imports = absolute_imports_for(&file.path).unwrap_or_default();
        for imported in imports {
            let Some(indexes) = modules.get(imported.as_str()) else {
                continue;
            };
            if indexes.len() != 1 || indexes[0] == index {
                continue;
            }
            let dependency = indexes[0];
            if dependents[dependency].contains(&index) {
                continue;
            }
            if import_edges >= crate::DEFAULT_MAX_IMPORT_EDGES {
                if !edge_limit_reported {
                    unsupported(
                        result,
                        file,
                        SourceRange { start: 0, end: 0 },
                        "supplied-module import graph exceeds the 50,000 edge work limit",
                    );
                    edge_limit_reported = true;
                }
                continue;
            }
            dependents[dependency].insert(index);
            indegree[index] += 1;
            import_edges += 1;
        }
    }
    let mut ready = BTreeSet::new();
    for (index, file) in files.iter().enumerate() {
        if indegree[index] == 0 {
            ready.insert((file.module.as_str(), index));
        }
    }
    let mut order = Vec::with_capacity(files.len());
    while let Some((_, index)) = ready.pop_first() {
        order.push(index);
        for dependent in dependents[index].iter().copied() {
            indegree[dependent] -= 1;
            if indegree[dependent] == 0 {
                ready.insert((files[dependent].module.as_str(), dependent));
            }
        }
    }
    let mut cyclic: Vec<_> = files
        .iter()
        .enumerate()
        .filter_map(|(index, _)| (indegree[index] > 0).then_some(index))
        .collect();
    cyclic.sort_by_key(|index| (&files[*index].module, &files[*index].path));
    for index in cyclic {
        let file = &files[index];
        unsupported(
            result,
            file,
            SourceRange { start: 0, end: 0 },
            &format!("supplied-module import cycle includes {}", file.module),
        );
        order.push(index);
    }
    order
}

fn add_contract(result: &mut Inspection, contract: Contract) {
    if result.coverage.declarations >= DEFAULT_MAX_DECLARATIONS {
        return;
    }
    result.contracts.push(contract);
    result.coverage.declarations += 1;
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(character) if character == '_' || unicode_ident::is_xid_start(character))
        && chars.all(|character| character == '_' || unicode_ident::is_xid_continue(character))
}

fn unsupported(result: &mut Inspection, file: &SourceFile, range: SourceRange, message: &str) {
    let primary = crate::source_span(&file.path, &file.source, range);
    result.coverage.status = "incomplete".into();
    result
        .coverage
        .reasons
        .push(format!("{}:{}: {message}", file.path, primary.line));
    result.diagnostics.push(Diagnostic {
        code: "TP9001".into(),
        severity: "warning".into(),
        message: message.into(),
        primary,
        related: Vec::new(),
        certainty: "unknown".into(),
    });
}

fn unsupported_related_import(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    message: &str,
    import_span: Span,
) {
    let primary = crate::source_span(&file.path, &file.source, range);
    result.coverage.status = "incomplete".into();
    result
        .coverage
        .reasons
        .push(format!("{}:{}: {message}", file.path, primary.line));
    result.diagnostics.push(Diagnostic {
        code: "TP9001".into(),
        severity: "warning".into(),
        message: message.into(),
        primary,
        related: vec![import_span],
        certainty: "unknown".into(),
    });
}

fn declaration_error(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    code: &str,
    message: &str,
    related: Vec<Span>,
) {
    result.coverage.status = "incomplete".into();
    result.diagnostics.push(Diagnostic {
        code: code.into(),
        severity: "error".into(),
        message: message.into(),
        primary: crate::source_span(&file.path, &file.source, range),
        related,
        certainty: "definite".into(),
    });
}

fn syntax_error(result: &mut Inspection, file: &SourceFile, summary: &ParseSummary) {
    let offset = summary
        .error_offset
        .unwrap_or_default()
        .min(file.source.len());
    let range = SourceRange {
        start: offset,
        end: offset.saturating_add(1).min(file.source.len()),
    };
    result.coverage.status = "incomplete".into();
    result.coverage.reasons.push(format!(
        "{}:{}: Python syntax error",
        file.path,
        crate::source_span(&file.path, &file.source, range).line
    ));
    result.diagnostics.push(Diagnostic {
        code: "TP4001".into(),
        severity: "error".into(),
        message: format!(
            "Python syntax error: {}",
            summary.error.as_deref().unwrap_or("unknown parse error")
        ),
        primary: crate::source_span(&file.path, &file.source, range),
        related: Vec::new(),
        certainty: "definite".into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ast_ranges_use_one_based_utf8_byte_columns() {
        let source = "from typepact.rust import Borrowed\nclass C: pass\ndef use(label: str = 'é', *, value: Borrowed[C]): pass\n";
        let output = crate::frontend::parse_frontend(source, "3.11").unwrap();
        let Declaration::Function { parameters, .. } = output
            .declarations
            .iter()
            .find(|declaration| matches!(declaration, Declaration::Function { .. }))
            .unwrap()
        else {
            panic!("expected function declaration")
        };
        let annotation = parameters[1].annotation.as_ref().unwrap();
        let line = source.lines().nth(2).unwrap();
        assert_eq!(
            crate::source_span("x.py", source, annotation.range).column,
            line.find("Borrowed").unwrap() + 1
        );
    }

    #[test]
    fn ast_expressions_retain_multiline_source_ranges() {
        let source = "from typepact.rust import Borrowed\nclass C: pass\ndef use(\n    value: Borrowed[C],\n): pass\n";
        let output = crate::frontend::parse_frontend(source, "3.11").unwrap();
        let Declaration::Function {
            parameters, range, ..
        } = output
            .declarations
            .iter()
            .find(|declaration| matches!(declaration, Declaration::Function { .. }))
            .unwrap()
        else {
            panic!("expected function declaration")
        };
        let annotation = parameters[0].annotation.as_ref().unwrap();
        assert_eq!(crate::source_span("x.py", source, annotation.range).line, 4);
        assert_eq!(crate::source_span("x.py", source, *range).end_line, 5);
    }

    #[test]
    fn deeply_nested_ast_expressions_stop_at_the_work_limit() {
        let source = format!("Alias = {}A\n", "A.".repeat(200));
        let result = crate::inspect(&[SourceFile::new("deep.py", "deep", source)], "3.11");
        assert_eq!(result.coverage.status, "incomplete");
        assert!(result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("128-level or 50,000-node")));
    }
}

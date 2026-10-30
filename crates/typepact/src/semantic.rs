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
}

#[derive(Clone, Debug)]
struct ParsedAnnotation {
    base: Option<String>,
    access: Access,
    origin: &'static str,
    has_resource: bool,
    closed_by: Option<String>,
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
                        state.clear_name(name, &file.module, &mut bindings);
                        lower_assignment(
                            &mut result,
                            file,
                            name,
                            value.as_ref(),
                            &mut state,
                            &symbols,
                            &bindings,
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
                    // Python evaluates eager annotations before installing the new function binding.
                    state.clear_name(name, &file.module, &mut bindings);
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
    for module in &summary.absolute_imports {
        match plan.module_counts.get(module).copied() {
            Some(1) => {
                context.supplied_modules.insert(module.clone());
                if let Some(exports) = computed_exports.get(module) {
                    context
                        .dependency_exports
                        .insert(module.clone(), exports.clone());
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
            project
                .symbols
                .get(&view.symbol)
                .map(|class| (view.symbol.clone(), class.clone()))
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
    imports: BTreeMap<String, String>,
    aliases: BTreeMap<String, String>,
    alias_lifecycle: BTreeMap<String, String>,
    alias_access: BTreeMap<String, Access>,
    exports: BTreeMap<String, ExportedView>,
}

impl FileState {
    fn new(_module: &str) -> Self {
        Self::default()
    }

    fn clear_name(&mut self, name: &str, module: &str, bindings: &mut BTreeMap<String, String>) {
        self.imports.remove(name);
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
        "typepact.contracts.Resource" => "typepact::contracts::Resource",
        "typepact.contracts.SharedBorrow" => "typepact::contracts::SharedBorrow",
        "typepact.contracts.OwnedAccess" => "typepact::contracts::OwnedAccess",
        "typepact.contracts.ExclusiveBorrow" => "typepact::contracts::ExclusiveBorrow",
        "typepact.rust.Borrowed" => "typepact::Borrowed",
        "typepact.rust.Owned" => "typepact::Owned",
        "typepact.rust.BorrowedMut" => "typepact::BorrowedMut",
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
    if view.access != Access::Unspecified {
        state.alias_access.insert(local.to_owned(), view.access);
    }
    if let Some(closed_by) = view.closed_by {
        state.alias_lifecycle.insert(local.to_owned(), closed_by);
    }
    bindings.insert(format!("{}.{local}", file.module), view.symbol);
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
    for decorator in decorators {
        let AstExprKind::Call {
            function,
            arguments,
            keywords,
        } = &decorator.kind
        else {
            continue;
        };
        if canonical(function, &state.imports).as_deref() != Some("typepact::resource") {
            continue;
        }
        let closed_by = keywords
            .iter()
            .find(|(key, _)| key.as_deref() == Some("closed_by"))
            .map(|(_, value)| value);
        let method = closed_by.and_then(|value| match &value.kind {
            AstExprKind::String(value) => Some(value.clone()),
            _ => None,
        });
        let Some(method) = method else {
            unsupported(
                result,
                file,
                decorator.range,
                "resource decorator requires a literal closed_by method",
            );
            continue;
        };
        if !is_identifier(&method) || !arguments.is_empty() {
            declaration_error(
                result,
                file,
                decorator.range,
                "TP4002",
                "closed_by must be a literal method name",
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
        },
    );
    bindings.insert(base.clone(), symbol.clone());
    state.exports.insert(
        name.to_owned(),
        ExportedView {
            symbol: symbol.clone(),
            access: Access::Unspecified,
            closed_by: closed.as_ref().map(|(method, _)| method.clone()),
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

fn lower_assignment(
    result: &mut Inspection,
    file: &SourceFile,
    name: &str,
    expression: Option<&AstExpr>,
    state: &mut FileState,
    symbols: &BTreeMap<String, ClassDecl>,
    bindings: &BTreeMap<String, String>,
) {
    let Some(expression) = expression else {
        return;
    };
    if !is_alias_candidate(expression, &state.imports) {
        return;
    }
    let mut parsed = parse_annotation(expression, &state.imports, file);
    if report_annotation_problem(result, file, expression.range, &parsed) {
        return;
    }
    if parsed.access == Access::Unspecified {
        if let Some(access) = parsed
            .base
            .as_deref()
            .and_then(|base| state.alias_access.get(base))
        {
            parsed.access = *access;
        }
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
        return;
    };
    let Some(symbol) = resolve_symbol(base, &file.module, symbols, &state.aliases, bindings) else {
        if parsed.has_resource || parsed.access != Access::Unspecified {
            unsupported(
                result,
                file,
                expression.range,
                &format!("unresolved base type in contract alias: {base}"),
            );
        }
        return;
    };
    state.aliases.insert(name.to_owned(), symbol.clone());
    if parsed.access != Access::Unspecified {
        state.alias_access.insert(name.to_owned(), parsed.access);
    }
    if let Some(closed_by) = parsed.closed_by.clone() {
        state
            .alias_lifecycle
            .insert(name.to_owned(), closed_by.clone());
    } else if let Some(previous_lifecycle) = state.alias_lifecycle.get(base).cloned() {
        state
            .alias_lifecycle
            .insert(name.to_owned(), previous_lifecycle);
    }
    let inherited_lifecycle = parsed
        .closed_by
        .clone()
        .or_else(|| state.alias_lifecycle.get(base).cloned())
        .or_else(|| {
            symbols
                .get(&symbol)
                .and_then(|declaration| declaration.closed_by.clone())
        });
    state.exports.insert(
        name.to_owned(),
        ExportedView {
            symbol: symbol.clone(),
            access: parsed.access,
            closed_by: inherited_lifecycle,
        },
    );
    if parsed.has_resource {
        let Some(closed_by) = parsed.closed_by else {
            unsupported(
                result,
                file,
                expression.range,
                "Resource metadata requires a literal closed_by method",
            );
            return;
        };
        if !is_identifier(&closed_by) {
            declaration_error(
                result,
                file,
                expression.range,
                "TP4002",
                "closed_by must be a literal method name",
                Vec::new(),
            );
            return;
        }
        add_contract(
            result,
            Contract {
                schema_version: 0,
                base_symbol: symbol,
                display_name: name.to_owned(),
                lifecycle: Some(Lifecycle { closed_by }),
                access: parsed.access.as_str().into(),
                source: crate::source_span(&file.path, &file.source, expression.range),
                provenance: vec![Provenance {
                    origin: "annotated-resource-metadata".into(),
                    trust: "declared-unverified".into(),
                }],
            },
        );
    }
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
    let mut parsed = parse_annotation(expression, &state.imports, file);
    if report_annotation_problem(result, file, expression.range, &parsed) {
        return;
    }
    if parsed.access == Access::Unspecified {
        if let Some(access) = parsed
            .base
            .as_deref()
            .and_then(|base| state.alias_access.get(base))
        {
            parsed.access = *access;
        }
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
    if let Some(symbol) = resolve_symbol(base, &file.module, symbols, &state.aliases, bindings) {
        let declaration = symbols.get(&symbol);
        let lifecycle = state
            .alias_lifecycle
            .get(base)
            .cloned()
            .or_else(|| declaration.and_then(|decl| decl.closed_by.clone()))
            .map(|closed_by| Lifecycle { closed_by });
        let mut provenance = vec![Provenance {
            origin: parsed.origin.into(),
            trust: "declared-unverified".into(),
        }];
        if lifecycle.is_some() {
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
                access: parsed.access.as_str().into(),
                source: crate::source_span(&file.path, &file.source, expression.range),
                provenance,
            },
        );
    } else if parsed.access != Access::Unspecified || parsed.has_resource {
        unsupported(
            result,
            file,
            expression.range,
            &format!("unresolved annotation base type {base}"),
        );
    }
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
        return ParsedAnnotation {
            base,
            access,
            origin: "explicit-access-alias",
            has_resource: false,
            closed_by: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: if arguments.len() == 1 && arguments[0].kind != AstExprKind::Other {
                None
            } else {
                Some("computed base type in access annotation is unsupported")
            },
        };
    }
    if identity != "typing::Annotated" {
        return ParsedAnnotation::plain(None);
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
                let closed_by = keywords
                    .iter()
                    .find(|(key, _)| key.as_deref() == Some("closed_by"))
                    .and_then(|(_, value)| match &value.kind {
                        AstExprKind::String(method) => Some(method.clone()),
                        _ => None,
                    });
                if closed_by.is_none() || !call_arguments.is_empty() {
                    unsupported_reason =
                        Some("Resource metadata requires a literal closed_by method");
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
        conflict,
        conflict_spans,
        unsupported_reason,
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
    matches!(chars.next(), Some(character) if character == '_' || character.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
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

use crate::{
    db::ParseSummary,
    frontend::{AstExpr, AstExprKind, Declaration, ImportName, SourceRange},
    Access, Contract, Coverage, Diagnostic, Inspection, Lifecycle, Provenance, SourceFile, Span,
    DEFAULT_MAX_DECLARATIONS, SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
mod annotations;
mod must_use;
mod project;
use annotations::{
    canonical, canonical_path, contains_codepact_contract, expression_path, is_alias_candidate,
    lower_function_annotation, merge_annotation_metadata, parse_annotation,
    report_annotation_problem,
};
use must_use::{
    annotation_is_must_use, check_must_use_body, contains_must_use_marker, function_is_must_use,
    is_misplaced_must_use_decorator, report_misplaced_must_use_annotation,
    supported_method_must_use,
};
use project::order_project_files;
pub(crate) use project::{context_for_module, plan_project};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ClassDecl {
    closed_by: Option<String>,
    closed_by_span: Option<Span>,
}

struct SemanticProject {
    inspection: Inspection,
    module_exports: BTreeMap<String, BTreeMap<String, ExportedView>>,
    module_functions: BTreeMap<String, BTreeSet<String>>,
    module_must_use_callables: BTreeMap<String, BTreeMap<String, Span>>,
    symbols: BTreeMap<String, ClassDecl>,
}

fn inspect_inner(
    files: &[SourceFile],
    python_version: &str,
    prepared: &BTreeMap<String, ParseSummary>,
    module_context: Option<&ModuleContext>,
    run_must_use: bool,
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

    let file_order = order_project_files(
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
    let mut module_functions = module_context
        .into_iter()
        .flat_map(|context| context.dependency_exports.iter())
        .map(|(module, exports)| (module.clone(), exports.functions.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut module_must_use_callables = module_context
        .into_iter()
        .flat_map(|context| context.dependency_exports.iter())
        .map(|(module, exports)| (module.clone(), exports.must_use_callables.clone()))
        .collect::<BTreeMap<_, _>>();
    for index in file_order.order {
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
        let mut state = FileState::new(python_version);
        let mut pending_must_use_bodies = Vec::<(
            Vec<crate::frontend::FlowStatement>,
            Option<(String, Option<String>)>,
            Vec<String>,
        )>::new();
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
                            "future annotations are not resolved",
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
                            &module_functions,
                            &module_must_use_callables,
                            &mut bindings,
                            &mut state,
                        );
                    }
                }
                Declaration::Import { names, .. } => {
                    for item in names {
                        let local = item
                            .alias
                            .as_deref()
                            .unwrap_or_else(|| item.name.split('.').next().unwrap_or(&item.name));
                        state.clear_name(local, &file.module, &mut bindings);
                        if item.name == "contextlib"
                            && !supplied_modules.contains(&item.name)
                            && !ambiguous_modules.contains(&item.name)
                        {
                            state
                                .imports
                                .insert(local.to_owned(), "external:contextlib".into());
                        } else if item.name == "codepact" || item.name.starts_with("codepact.") {
                            let identity = if item.alias.is_some() {
                                item.name.replace('.', "::")
                            } else {
                                item.name.split('.').next().unwrap_or(&item.name).to_owned()
                            };
                            state.imports.insert(local.to_owned(), identity);
                        } else {
                            unsupported(
                                &mut result,
                                file,
                                item.range,
                                "module imports are not resolved",
                            );
                        }
                    }
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
                    class_annotations,
                    methods,
                    unsupported: has_unsupported_syntax,
                    range,
                    ..
                } => {
                    if run_must_use {
                        for decorator in decorators {
                            if is_misplaced_must_use_decorator(decorator, &state.imports) {
                                declaration_error(
                                    &mut result,
                                    file,
                                    decorator.range,
                                    "TP4002",
                                    "must-use metadata is supported only on function return annotations",
                                    Vec::new(),
                                );
                            }
                        }
                        for annotation in class_annotations {
                            report_misplaced_must_use_annotation(
                                &mut result,
                                file,
                                annotation,
                                &state.imports,
                            );
                        }
                        for method in methods {
                            for decorator in &method.decorators {
                                if is_misplaced_must_use_decorator(decorator, &state.imports) {
                                    declaration_error(
                                        &mut result,
                                        file,
                                        decorator.range,
                                        "TP4002",
                                        "must-use metadata is supported only on function return annotations",
                                        Vec::new(),
                                    );
                                }
                            }
                            for (_, annotation) in &method.parameter_annotations {
                                report_misplaced_must_use_annotation(
                                    &mut result,
                                    file,
                                    annotation,
                                    &state.imports,
                                );
                            }
                        }
                    }
                    let mut method_contracts = Vec::new();
                    for method in methods {
                        match supported_method_must_use(
                            &method.decorators,
                            method.returns.as_ref(),
                            &state.imports,
                        ) {
                            Ok(true) if method.is_async || method.is_generator => {
                                if run_must_use {
                                    unsupported(
                                        &mut result,
                                        file,
                                        method.range,
                                        "must-use result consumption for async or generator methods is unsupported in 0.4",
                                    );
                                }
                            }
                            Ok(true) => method_contracts.push((
                                format!("{name}.{}", method.name),
                                crate::source_span(&file.path, &file.source, method.range),
                            )),
                            Ok(false) => {}
                            Err(()) => {
                                if run_must_use {
                                    unsupported(
                                        &mut result,
                                        file,
                                        method.range,
                                        "must-use method has an unsupported decorator identity or order",
                                    );
                                }
                            }
                        }
                    }
                    let pending_methods = methods
                        .iter()
                        .filter(|method| !method.is_generator)
                        .map(|method| {
                            let is_static = method.decorators.iter().any(|decorator| {
                                canonical(decorator, &state.imports).as_deref()
                                    == Some("builtins::staticmethod")
                            });
                            (
                                method.body.clone(),
                                Some((
                                    name.clone(),
                                    (!is_static)
                                        .then(|| method.parameters.first().cloned())
                                        .flatten(),
                                )),
                                method.parameters.clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    lower_class(
                        &mut result,
                        file,
                        name,
                        decorators,
                        &mut state,
                        &mut symbols,
                        &mut bindings,
                    );
                    for (callable, span) in method_contracts {
                        state.must_use_callables.insert(callable, span);
                    }
                    if !has_unsupported_syntax {
                        state.known_classes.insert(name.clone());
                    }
                    if *has_unsupported_syntax {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "Ruff AST marked syntax outside the supported declaration subset (class bases, keywords, and type parameters are not resolved)",
                        );
                    }
                    for (body, class_scope, parameters) in pending_methods {
                        pending_must_use_bodies.push((body, class_scope, parameters));
                    }
                }
                Declaration::Assignment {
                    name,
                    annotation,
                    value,
                    range,
                } => {
                    if run_must_use
                        && annotation.as_ref().is_some_and(|annotation| {
                            contains_must_use_marker(annotation, &state.imports)
                        })
                    {
                        report_misplaced_must_use_annotation(
                            &mut result,
                            file,
                            annotation.as_ref().unwrap(),
                            &state.imports,
                        );
                    }
                    if let Some(name) = name {
                        let source_name = value.as_ref().and_then(expression_path);
                        let must_use_source = source_name
                            .as_ref()
                            .and_then(|source| state.must_use_callables.get(source).cloned());
                        let instance_class = value.as_ref().and_then(|value| {
                            let AstExprKind::Call { function, .. } = &value.kind else {
                                return source_name
                                    .as_deref()
                                    .and_then(|source| state.instances.get(source).cloned());
                            };
                            expression_path(function)
                                .filter(|class| state.known_classes.contains(class))
                        });
                        lower_assignment(
                            &mut result,
                            file,
                            name,
                            value.as_ref(),
                            &mut state,
                            &symbols,
                            &mut bindings,
                        );
                        if let Some(span) = must_use_source {
                            state.must_use_callables.insert(name.clone(), span);
                            state.callables.insert(name.clone());
                        } else {
                            state.must_use_callables.remove(name);
                        }
                        state.instances.remove(name);
                        if let Some(class) = instance_class {
                            state.instances.insert(name.clone(), class);
                        }
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
                        "Python type-alias statements are not resolved",
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
                    body,
                    is_async,
                    is_generator,
                    ..
                } => {
                    let marked = function_is_must_use(decorators, returns.as_ref(), &state.imports);
                    if run_must_use {
                        for parameter in parameters {
                            if let Some(annotation) = &parameter.annotation {
                                report_misplaced_must_use_annotation(
                                    &mut result,
                                    file,
                                    annotation,
                                    &state.imports,
                                );
                            }
                        }
                        if returns.as_ref().is_some_and(|annotation| {
                            contains_must_use_marker(annotation, &state.imports)
                                && !annotation_is_must_use(annotation, &state.imports)
                        }) {
                            unsupported(
                            &mut result,
                            file,
                            returns.as_ref().unwrap().range,
                            "must-use return metadata is nested in an unsupported annotation shape",
                        );
                        }
                        for decorator in decorators {
                            if is_misplaced_must_use_decorator(decorator, &state.imports) {
                                declaration_error(
                                    &mut result,
                                    file,
                                    decorator.range,
                                    "TP4002",
                                    "must-use metadata is supported only on function return annotations",
                                    Vec::new(),
                                );
                            }
                        }
                    }
                    let only_known_decorators = decorators.iter().all(|decorator| {
                        canonical(decorator, &state.imports).as_deref()
                            == Some("codepact::must_use")
                    });
                    if run_must_use && marked && (*is_async || *is_generator) {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "must-use result consumption for async or generator functions is unsupported in 0.4",
                        );
                    }
                    if *has_unsupported_syntax {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "nested or generic function declarations are unsupported",
                        );
                    }
                    if !decorators.is_empty() && !only_known_decorators {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "decorated functions are unresolved",
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
                        if marked && !*is_async && !*is_generator {
                            state.must_use_callables.insert(
                                name.clone(),
                                crate::source_span(&file.path, &file.source, *range),
                            );
                        }
                        if !*is_generator {
                            pending_must_use_bodies.push((
                                body.clone(),
                                None,
                                parameters
                                    .iter()
                                    .map(|parameter| parameter.name.clone())
                                    .collect(),
                            ));
                        }
                        if !has_unsupported_syntax {
                            state.callables.insert(name.clone());
                        }
                    }
                }
                Declaration::Unsupported { message, range, .. } => {
                    unsupported(&mut result, file, *range, message);
                    state.clear_module(&file.module, &mut bindings);
                }
                Declaration::Expression { value, range } => {
                    if run_must_use {
                        let statement = crate::frontend::FlowStatement::Expression {
                            value: value.clone(),
                            range: *range,
                        };
                        check_must_use_body(
                            &mut result,
                            file,
                            std::slice::from_ref(&statement),
                            &state,
                            None,
                            &[],
                        );
                    } else {
                        unsupported(
                            &mut result,
                            file,
                            *range,
                            "module-level control flow and executable statements are unsupported",
                        );
                        state.clear_module(&file.module, &mut bindings);
                    }
                }
            }
        }
        if !stop {
            if run_must_use {
                for (body, class_scope, parameters) in pending_must_use_bodies {
                    check_must_use_body(
                        &mut result,
                        file,
                        &body,
                        &state,
                        class_scope
                            .as_ref()
                            .map(|(class, receiver)| (class.as_str(), receiver.as_deref())),
                        &parameters,
                    );
                }
            }
            module_exports.insert(file.module.clone(), state.exports.clone());
            module_functions.insert(file.module.clone(), state.callables.clone());
            module_must_use_callables.insert(file.module.clone(), state.must_use_callables.clone());
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
        module_functions,
        module_must_use_callables,
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
    functions: BTreeSet<String>,
    must_use_callables: BTreeMap<String, Span>,
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

fn summary_uses_contract_wrapper(summary: &ParseSummary) -> bool {
    fn expression_uses_wrapper(expression: &AstExpr, imports: &BTreeMap<String, String>) -> bool {
        match &expression.kind {
            AstExprKind::Subscript(container, arguments) => {
                canonical(container, imports).is_some_and(|identity| {
                    matches!(
                        identity.as_str(),
                        "typing::Annotated"
                            | "codepact::Borrowed"
                            | "codepact::Owned"
                            | "codepact::BorrowedMut"
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
            AstExprKind::Tuple(values) | AstExprKind::Collection(values) => values
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
            Declaration::Assignment { .. } | Declaration::Expression { .. } => {}
        }
    }
    false
}

pub(crate) fn inspect_module(
    file: &SourceFile,
    python_version: &str,
    summary: &ParseSummary,
    context: &ModuleContext,
    run_must_use: bool,
    check_cancelled: impl FnMut(),
) -> ModuleAnalysis {
    let prepared = BTreeMap::from([(file.path.clone(), summary.clone())]);
    let project = inspect_inner(
        std::slice::from_ref(file),
        python_version,
        &prepared,
        Some(context),
        run_must_use,
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
    let functions = project
        .module_functions
        .get(&file.module)
        .cloned()
        .unwrap_or_default();
    let must_use_callables = project
        .module_must_use_callables
        .get(&file.module)
        .cloned()
        .unwrap_or_default();
    ModuleAnalysis {
        inspection: project.inspection,
        exports: SemanticExports {
            classes,
            names,
            functions,
            must_use_callables,
        },
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
            semantic_identity: Vec::new(),
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
    python_version: String,
    future_annotations: bool,
    imports: BTreeMap<String, String>,
    must_use_callables: BTreeMap<String, Span>,
    known_classes: BTreeSet<String>,
    instances: BTreeMap<String, String>,
    unresolved_imports: BTreeMap<String, UnresolvedImport>,
    aliases: BTreeMap<String, String>,
    alias_lifecycle: BTreeMap<String, (String, Span)>,
    alias_access: BTreeMap<String, (Access, Span)>,
    exports: BTreeMap<String, ExportedView>,
    callables: BTreeSet<String>,
}

#[derive(Clone)]
struct UnresolvedImport {
    module: String,
    span: Span,
}

impl FileState {
    fn new(python_version: &str) -> Self {
        let imports = BTreeMap::from([
            ("staticmethod".into(), "builtins::staticmethod".into()),
            ("classmethod".into(), "builtins::classmethod".into()),
        ]);
        Self {
            python_version: python_version.into(),
            imports,
            ..Self::default()
        }
    }

    fn clear_name(&mut self, name: &str, module: &str, bindings: &mut BTreeMap<String, String>) {
        self.imports.remove(name);
        self.unresolved_imports.remove(name);
        self.aliases.remove(name);
        self.alias_lifecycle.remove(name);
        self.alias_access.remove(name);
        self.exports.remove(name);
        self.callables.remove(name);
        self.must_use_callables.remove(name);
        let method_prefix = format!("{name}.");
        self.must_use_callables
            .retain(|callable, _| !callable.starts_with(&method_prefix));
        self.known_classes.remove(name);
        self.instances.remove(name);
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
        self.callables.clear();
        self.must_use_callables.clear();
        self.known_classes.clear();
        self.instances.clear();
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
            | Self::Expression { range, .. }
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
    module_functions: &BTreeMap<String, BTreeSet<String>>,
    module_must_use_callables: &BTreeMap<String, BTreeMap<String, Span>>,
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
            module_functions,
            module_must_use_callables,
            bindings,
            state,
        );
    }
}

fn register_import(original: &str, local: &str, module: &str, state: &mut FileState) {
    let canonical = match format!("{module}.{original}").as_str() {
        "typing.Annotated" => "typing::Annotated",
        "codepact.rust" => "codepact::rust",
        "codepact.contracts" => "codepact::contracts",
        "codepact.resource" => "codepact::resource",
        "codepact.must_use" => "codepact::must_use",
        "codepact.discard" => "codepact::discard",
        "codepact.move" => "codepact::move",
        "builtins.staticmethod" => "builtins::staticmethod",
        "builtins.classmethod" => "builtins::classmethod",
        "codepact.Resource" | "codepact.contracts.Resource" => "codepact::contracts::Resource",
        "codepact.contracts.SharedBorrow" => "codepact::contracts::SharedBorrow",
        "codepact.contracts.OwnedAccess" => "codepact::contracts::OwnedAccess",
        "codepact.contracts.ExclusiveBorrow" => "codepact::contracts::ExclusiveBorrow",
        "codepact.contracts.MustUseResult" => "codepact::contracts::MustUseResult",
        "codepact.contracts.Borrowed" | "codepact.rust.Borrowed" => "codepact::Borrowed",
        "codepact.contracts.Owned" | "codepact.rust.Owned" => "codepact::Owned",
        "codepact.contracts.BorrowedMut" | "codepact.rust.BorrowedMut" => "codepact::BorrowedMut",
        "codepact.rust.MustUse" | "codepact.contracts.MustUse" => "codepact::MustUse",
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
    module_functions: &BTreeMap<String, BTreeSet<String>>,
    module_must_use_callables: &BTreeMap<String, BTreeMap<String, Span>>,
    bindings: &mut BTreeMap<String, String>,
    state: &mut FileState,
) {
    if module == "contextlib"
        && !supplied_modules.contains(module)
        && !ambiguous_modules.contains(module)
        && matches!(original, "closing" | "aclosing")
    {
        state.callables.insert(local.to_owned());
        return;
    }
    if module == "codepact" || module.starts_with("codepact.") {
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
        if let Some(declaration) = module_must_use_callables
            .get(module)
            .and_then(|functions| functions.get(original))
        {
            state
                .must_use_callables
                .insert(local.to_owned(), declaration.clone());
            state.callables.insert(local.to_owned());
            return;
        }
        if module_functions
            .get(module)
            .is_some_and(|functions| functions.contains(original))
        {
            state.callables.insert(local.to_owned());
            return;
        }
        unsupported(
            result,
            file,
            range,
            &format!("supplied module {module} does not directly export class or supported alias {original}"),
        );
        return;
    };
    state.exports.insert(local.to_owned(), view.clone());
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
                if canonical(decorator, &state.imports).as_deref() == Some("codepact::resource") {
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
        if canonical(function, &state.imports).as_deref() != Some("codepact::resource") {
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
        if !crate::is_python_identifier(&method, &state.python_version) {
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
            "class decorator expression is outside the supported CodePact resource subset",
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
        if matches!(
            expression.kind,
            AstExprKind::Subscript(..) | AstExprKind::Tuple(_)
        ) && contains_codepact_contract(expression, state)
        {
            unsupported(
                result,
                file,
                expression.range,
                "nested CodePact contracts are unsupported in this annotation form",
            );
        }
        state.clear_name(name, &file.module, bindings);
        return;
    }
    let parsed = parse_annotation(expression, state, file);
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
        &state.python_version,
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

pub(crate) fn cyclic_import_modules(
    files: &[SourceFile],
    summaries: &BTreeMap<String, crate::db::ParseSummary>,
) -> BTreeSet<String> {
    let mut result = Inspection {
        schema_version: SCHEMA_VERSION,
        assurance: "contract-inspection-only".into(),
        contracts: Vec::new(),
        diagnostics: Vec::new(),
        coverage: Coverage {
            status: "complete-for-supported-subset".into(),
            reasons: Vec::new(),
            source_bytes: 0,
            declarations: 0,
        },
    };
    order_project_files(
        files,
        |path| {
            summaries
                .get(path)
                .map(|summary| summary.absolute_imports.as_slice())
        },
        &mut result,
    )
    .cyclic_modules
}

fn add_contract(result: &mut Inspection, contract: Contract) {
    if result.coverage.declarations >= DEFAULT_MAX_DECLARATIONS {
        return;
    }
    result.contracts.push(contract);
    result.coverage.declarations += 1;
}

fn unsupported(result: &mut Inspection, file: &SourceFile, range: SourceRange, message: &str) {
    let primary = crate::source_span(&file.path, &file.source, range);
    result.coverage.status = "incomplete".into();
    result
        .coverage
        .reasons
        .push(format!("{}:{}: {message}", file.path, primary.line));
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
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
        semantic_identity: Vec::new(),
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
        semantic_identity: Vec::new(),
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
        semantic_identity: Vec::new(),
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
        let source = "from codepact.rust import Borrowed\nclass C: pass\ndef use(label: str = 'é', *, value: Borrowed[C]): pass\n";
        let output = crate::frontend::parse_frontend(source, "test", "3.11").unwrap();
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
        let source = "from codepact.rust import Borrowed\nclass C: pass\ndef use(\n    value: Borrowed[C],\n): pass\n";
        let output = crate::frontend::parse_frontend(source, "test", "3.11").unwrap();
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

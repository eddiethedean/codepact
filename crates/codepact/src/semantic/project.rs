use super::*;

pub(super) struct ProjectFileOrder {
    pub(super) order: Vec<usize>,
    pub(super) cyclic_modules: BTreeSet<String>,
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
    let file_order = order_project_files(
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
        order: file_order.order,
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
    let mut imported_names = BTreeMap::<String, BTreeSet<String>>::new();
    if let Some(declarations) = summary.declarations.as_ref() {
        for declaration in declarations {
            let Declaration::ImportFrom {
                module,
                level: 0,
                names,
                ..
            } = declaration
            else {
                continue;
            };
            imported_names
                .entry(module.clone())
                .or_default()
                .extend(names.iter().map(|name| name.name.clone()));
        }
    }
    for module in &summary.absolute_imports {
        match plan.module_counts.get(module).copied() {
            Some(1) => {
                context.supplied_modules.insert(module.clone());
                if let Some(exports) = computed_exports.get(module) {
                    let mut exports = exports.clone();
                    let names = imported_names.get(module);
                    exports
                        .names
                        .retain(|name, _| names.is_some_and(|names| names.contains(name)));
                    exports
                        .functions
                        .retain(|name| names.is_some_and(|names| names.contains(name)));
                    exports
                        .must_use_callables
                        .retain(|name, _| names.is_some_and(|names| names.contains(name)));
                    exports.classes.retain(|symbol, _| {
                        exports.names.values().any(|view| &view.symbol == symbol)
                    });
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

pub(super) fn order_project_files<'a>(
    files: &[SourceFile],
    absolute_imports_for: impl Fn(&str) -> Option<&'a [String]>,
    result: &mut Inspection,
) -> ProjectFileOrder {
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
    let mut cyclic_modules = BTreeSet::new();
    for index in cyclic {
        let file = &files[index];
        cyclic_modules.insert(file.module.clone());
        unsupported(
            result,
            file,
            SourceRange { start: 0, end: 0 },
            &format!("supplied-module import cycle includes {}", file.module),
        );
        order.push(index);
    }
    ProjectFileOrder {
        order,
        cyclic_modules,
    }
}

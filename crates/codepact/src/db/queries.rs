use super::{Db, FileInput, ParseSummary};
use crate::{Coverage, Inspection, SourceFile};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FlowFileContext {
    model_catalog: crate::model::FlowModelCatalog,
    cyclic_modules: BTreeSet<String>,
    ownership: crate::Ownership,
}
#[salsa::tracked]
pub fn parse_file(db: &dyn Db, file: FileInput, python_version: String) -> ParseSummary {
    db.unwind_if_revision_cancelled();
    if file.source(db).len() > crate::DEFAULT_MAX_SOURCE_BYTES {
        return ParseSummary {
            ok: false,
            error: Some("source file exceeds the inspection byte limit".into()),
            error_offset: Some(0),
            declarations: None,
            module_body: Vec::new(),
            absolute_imports: Vec::new(),
        };
    }
    let parsed =
        crate::frontend::parse_frontend(file.semantic_source(db), file.module(db), &python_version);
    db.unwind_if_revision_cancelled();
    match parsed {
        Ok(frontend) => ParseSummary {
            ok: true,
            error: None,
            error_offset: None,
            declarations: Some(frontend.declarations),
            module_body: frontend.module_body,
            absolute_imports: frontend.absolute_imports,
        },
        Err((error, error_offset)) => ParseSummary {
            ok: false,
            error: Some(error),
            error_offset: Some(error_offset),
            declarations: None,
            module_body: Vec::new(),
            absolute_imports: Vec::new(),
        },
    }
}

#[salsa::tracked(returns(ref))]
fn inspect_module_query(
    db: &dyn Db,
    file: FileInput,
    python_version: String,
    context: crate::semantic::ModuleContext,
    run_must_use: bool,
) -> crate::semantic::ModuleAnalysis {
    let source = SourceFile::new(file.path(db), file.module(db), file.semantic_source(db));
    let summary = parse_file(db, file, python_version.clone());
    let analysis = crate::source::with_indexed_sources([source.source.as_str()], || {
        crate::semantic::inspect_module(
            &source,
            &python_version,
            summary,
            &context,
            run_must_use,
            || {
                db.unwind_if_revision_cancelled();
            },
        )
    });
    analysis
}

#[salsa::tracked(returns(ref))]
fn module_exports_query(
    db: &dyn Db,
    file: FileInput,
    python_version: String,
    context: crate::semantic::ModuleContext,
) -> crate::semantic::SemanticExports {
    inspect_module_query(db, file, python_version, context, false)
        .exports
        .clone()
}

#[salsa::tracked(returns(ref))]
pub(super) fn inspect_project_query(
    db: &dyn Db,
    files: Vec<FileInput>,
    python_version: String,
    run_must_use: bool,
) -> Inspection {
    db.unwind_if_revision_cancelled();
    let sizes: Vec<_> = files
        .iter()
        .map(|file| (file.path(db).to_string(), file.source(db).len()))
        .collect();
    let mut total_bytes = 0usize;
    let exceeds_limit = sizes.iter().any(|(_, bytes)| {
        total_bytes = total_bytes.saturating_add(*bytes);
        *bytes > crate::DEFAULT_MAX_SOURCE_BYTES || total_bytes > crate::DEFAULT_MAX_SOURCE_BYTES
    });
    if exceeds_limit {
        db.unwind_if_revision_cancelled();
        return crate::source_limit_inspection(&sizes);
    }

    let mut sources = Vec::with_capacity(files.len());
    let mut parsed = BTreeMap::new();
    let mut total_declarations = 0usize;
    let total_source_bytes = sizes
        .iter()
        .fold(0usize, |total, (_, bytes)| total.saturating_add(*bytes));
    for file in files.iter().copied() {
        let source = SourceFile::new(file.path(db), file.module(db), file.semantic_source(db));
        let summary = parse_file(db, file, python_version.clone());
        total_declarations = total_declarations
            .saturating_add(summary.declarations.as_ref().map_or(0, std::vec::Vec::len));
        if total_declarations > crate::DEFAULT_MAX_TOTAL_AST_DECLARATIONS {
            db.unwind_if_revision_cancelled();
            return crate::invalid_input_inspection(
                &source.path,
                "total declaration work exceeds the 50,000 statement request limit",
                total_source_bytes,
            );
        }
        parsed.insert(source.path.clone(), summary);
        sources.push(source);
    }
    let plan = crate::semantic::plan_project(&sources, &python_version, &parsed);
    let mut computed_exports = BTreeMap::new();
    let mut module_inspections = Vec::new();
    for index in plan.order.iter().copied() {
        db.unwind_if_revision_cancelled();
        let source = &sources[index];
        let file = files[index];
        let Some(summary) = parsed.get(&source.path) else {
            continue;
        };
        let context =
            crate::semantic::context_for_module(source, summary, &plan, &computed_exports);
        let analysis = inspect_module_query(
            db,
            file,
            python_version.clone(),
            context.clone(),
            run_must_use,
        );
        let exports = module_exports_query(db, file, python_version.clone(), context);
        computed_exports.insert(source.module.clone(), exports.clone());
        module_inspections.push(analysis.inspection.clone());
    }
    let inspection = crate::semantic::combine_project(plan.inspection, module_inspections);
    db.unwind_if_revision_cancelled();
    inspection
}

#[salsa::tracked(returns(ref))]
fn check_flow_file_query(
    db: &dyn Db,
    file: FileInput,
    dependencies: Vec<FileInput>,
    python_version: String,
    flow_budget: usize,
    context: FlowFileContext,
    mut inspection: Inspection,
) -> Inspection {
    db.unwind_if_revision_cancelled();
    let sources: Vec<_> = dependencies
        .iter()
        .map(|dependency| {
            Arc::new(SourceFile::new(
                dependency.path(db),
                dependency.module(db),
                dependency.semantic_source(db),
            ))
        })
        .collect();
    let summaries: BTreeMap<_, _> = dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.path(db).to_string(),
                parse_file(db, *dependency, python_version.clone()).clone(),
            )
        })
        .collect();
    crate::flow::reset_flow_budget(flow_budget);
    crate::source::with_indexed_sources(
        sources.iter().map(|source| source.source.as_str()),
        || {
            crate::flow::check_file_bodies(
                file.path(db),
                &sources,
                &summaries,
                &mut inspection,
                crate::flow::FlowOptions {
                    python_version: &python_version,
                    models: &context.model_catalog,
                    cyclic_modules: &context.cyclic_modules,
                    ownership: context.ownership,
                },
            );
        },
    );
    db.unwind_if_revision_cancelled();
    inspection
}

#[salsa::tracked(returns(ref))]
pub(super) fn check_flow_query(
    db: &dyn Db,
    files: Vec<FileInput>,
    python_version: String,
    model_catalog: crate::model::FlowModelCatalog,
    mut inspection: Inspection,
    ownership: crate::Ownership,
) -> Inspection {
    db.unwind_if_revision_cancelled();
    let sources: Vec<_> = files
        .iter()
        .map(|file| SourceFile::new(file.path(db), file.module(db), file.semantic_source(db)))
        .collect();
    let summaries: BTreeMap<_, _> = files
        .iter()
        .map(|file| {
            (
                file.path(db).to_string(),
                parse_file(db, *file, python_version.clone()).clone(),
            )
        })
        .collect();
    let cyclic_modules = crate::semantic::cyclic_import_modules(&sources, &summaries);
    let sources: Vec<_> = sources.into_iter().map(Arc::new).collect();
    inspection.diagnostics.retain(|diagnostic| {
        !diagnostic
            .message
            .contains("module-level control flow and executable statements are unsupported")
    });
    inspection.coverage.reasons.retain(|reason| {
        !reason.contains("module-level control flow and executable statements are unsupported")
    });
    if inspection.coverage.reasons.is_empty() {
        inspection.coverage.status = "complete-for-supported-subset".into();
    }
    let mut module_files = BTreeMap::<String, Vec<(usize, FileInput)>>::new();
    for (index, file) in files.iter().copied().enumerate() {
        module_files
            .entry(file.module(db).to_string())
            .or_default()
            .push((index, file));
    }
    let mut contracts_by_path = BTreeMap::<String, Vec<usize>>::new();
    for (index, contract) in inspection.contracts.iter().enumerate() {
        contracts_by_path
            .entry(contract.source.file.clone())
            .or_default()
            .push(index);
    }
    // Fixed shares keep an edit in one file from changing another file's query key.
    // Since the input file cap is lower than the total flow-step limit, every file
    // receives a nonzero share and the sum of all per-file limits stays bounded.
    let flow_budget = crate::flow::MAX_FLOW_STEPS / files.len().max(1);
    for (target, source) in files.iter().copied().zip(&sources) {
        db.unwind_if_revision_cancelled();
        let mut dependency_modules = BTreeSet::from([source.module.clone()]);
        let mut pending = vec![source.module.clone()];
        while let Some(module) = pending.pop() {
            for (_, dependency) in module_files.get(&module).into_iter().flatten() {
                let Some(summary) = summaries.get(dependency.path(db)) else {
                    continue;
                };
                for imported in &summary.absolute_imports {
                    if module_files.contains_key(imported)
                        && dependency_modules.insert(imported.clone())
                    {
                        pending.push(imported.clone());
                    }
                }
            }
        }
        let mut dependencies: Vec<_> = dependency_modules
            .iter()
            .filter_map(|module| module_files.get(module))
            .flatten()
            .copied()
            .collect();
        dependencies.sort_by_key(|(index, _)| *index);
        let dependencies: Vec<_> = dependencies
            .iter()
            .map(|(_, dependency)| *dependency)
            .collect();
        let mut contract_indices: Vec<_> = dependencies
            .iter()
            .filter_map(|dependency| contracts_by_path.get(dependency.path(db)))
            .flatten()
            .copied()
            .collect();
        contract_indices.sort_unstable();
        let flow_input = Inspection {
            schema_version: inspection.schema_version,
            assurance: inspection.assurance.clone(),
            contracts: contract_indices
                .iter()
                .map(|index| inspection.contracts[*index].clone())
                .collect(),
            diagnostics: Vec::new(),
            coverage: Coverage {
                status: "complete-for-supported-subset".into(),
                reasons: Vec::new(),
                source_bytes: 0,
                declarations: 0,
            },
        };
        let resource_symbols = flow_input
            .contracts
            .iter()
            .map(|contract| contract.base_symbol.clone())
            .collect::<BTreeSet<_>>();
        let flow_context = FlowFileContext {
            model_catalog: model_catalog.for_resource_symbols(&resource_symbols),
            cyclic_modules: cyclic_modules.clone(),
            ownership,
        };
        let flow = check_flow_file_query(
            db,
            target,
            dependencies,
            python_version.clone(),
            flow_budget,
            flow_context,
            flow_input,
        );
        inspection
            .diagnostics
            .extend(flow.diagnostics.iter().cloned());
        for reason in &flow.coverage.reasons {
            if !inspection.coverage.reasons.contains(reason) {
                inspection.coverage.reasons.push(reason.clone());
            }
        }
        if flow.coverage.status == "incomplete" {
            inspection.coverage.status = "incomplete".into();
        }
    }
    db.unwind_if_revision_cancelled();
    inspection
}

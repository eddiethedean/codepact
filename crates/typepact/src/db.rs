use crate::{
    frontend::{Declaration, FlowStatement},
    Coverage, Inspection, SourceFile,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex};

#[salsa::input]
pub struct FileInput {
    #[returns(deref)]
    pub path: String,
    #[returns(deref)]
    pub module: String,
    #[returns(deref)]
    pub source: String,
}

#[salsa::db]
pub trait Db: salsa::Database {}

#[salsa::db]
#[derive(Default)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseSummary {
    pub ok: bool,
    pub error: Option<String>,
    pub(crate) error_offset: Option<usize>,
    pub(crate) declarations: Option<Vec<Declaration>>,
    pub(crate) module_body: Vec<FlowStatement>,
    pub(crate) absolute_imports: Vec<String>,
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
    let parsed = crate::frontend::parse_frontend(file.source(db), file.module(db), &python_version);
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

#[salsa::tracked(returns(clone))]
fn inspect_module_query(
    db: &dyn Db,
    file: FileInput,
    python_version: String,
    context_json: String,
) -> String {
    let source = SourceFile::new(file.path(db), file.module(db), file.source(db));
    let summary = parse_file(db, file, python_version.clone());
    let context: crate::semantic::ModuleContext =
        serde_json::from_str(&context_json).expect("module context serializes");
    let analysis =
        crate::semantic::inspect_module(&source, &python_version, summary, &context, || {
            db.unwind_if_revision_cancelled();
        });
    serde_json::to_string(&analysis).expect("module analysis serializes")
}

#[salsa::tracked(returns(clone))]
fn module_exports_query(
    db: &dyn Db,
    file: FileInput,
    python_version: String,
    context_json: String,
) -> String {
    let analysis: crate::semantic::ModuleAnalysis = serde_json::from_str(&inspect_module_query(
        db,
        file,
        python_version,
        context_json,
    ))
    .expect("module analysis deserializes");
    serde_json::to_string(&analysis.exports).expect("module exports serialize")
}

#[salsa::tracked(returns(clone))]
fn inspect_project_query(db: &dyn Db, files: Vec<FileInput>, python_version: String) -> String {
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
        return serde_json::to_string(&crate::source_limit_inspection(&sizes))
            .expect("source-limit result serializes");
    }

    let mut sources = Vec::with_capacity(files.len());
    let mut parsed = BTreeMap::new();
    let mut total_declarations = 0usize;
    let total_source_bytes = sizes
        .iter()
        .fold(0usize, |total, (_, bytes)| total.saturating_add(*bytes));
    for file in files.iter().copied() {
        let source = SourceFile::new(file.path(db), file.module(db), file.source(db));
        let summary = parse_file(db, file, python_version.clone());
        total_declarations = total_declarations
            .saturating_add(summary.declarations.as_ref().map_or(0, std::vec::Vec::len));
        if total_declarations > crate::DEFAULT_MAX_TOTAL_AST_DECLARATIONS {
            db.unwind_if_revision_cancelled();
            return serde_json::to_string(&crate::invalid_input_inspection(
                &source.path,
                "total declaration work exceeds the 50,000 statement request limit",
                total_source_bytes,
            ))
            .expect("declaration-limit result serializes");
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
        let context_json = serde_json::to_string(&context).expect("module context serializes");
        let analysis: crate::semantic::ModuleAnalysis = serde_json::from_str(
            &inspect_module_query(db, file, python_version.clone(), context_json.clone()),
        )
        .expect("module analysis deserializes");
        let exports: crate::semantic::SemanticExports = serde_json::from_str(
            &module_exports_query(db, file, python_version.clone(), context_json),
        )
        .expect("module exports deserialize");
        computed_exports.insert(source.module.clone(), exports);
        module_inspections.push(analysis.inspection);
    }
    let inspection = crate::semantic::combine_project(plan.inspection, module_inspections);
    db.unwind_if_revision_cancelled();
    serde_json::to_string(&inspection).expect("project inspection result serializes")
}

#[salsa::tracked(returns(clone))]
fn check_flow_file_query(
    db: &dyn Db,
    file: FileInput,
    dependencies: Vec<FileInput>,
    python_version: String,
    flow_budget: usize,
    model_catalog_json: String,
    inspection_json: String,
) -> String {
    db.unwind_if_revision_cancelled();
    let sources: Vec<_> = dependencies
        .iter()
        .map(|dependency| {
            SourceFile::new(
                dependency.path(db),
                dependency.module(db),
                dependency.source(db),
            )
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
    let mut inspection: Inspection =
        serde_json::from_str(&inspection_json).expect("flow file input deserializes");
    let model_catalog: crate::model::FlowModelCatalog =
        serde_json::from_str(&model_catalog_json).expect("flow model catalog deserializes");
    crate::flow::reset_flow_budget(flow_budget);
    crate::flow::check_file_bodies(
        file.path(db),
        &sources,
        &summaries,
        &mut inspection,
        &python_version,
        &model_catalog,
    );
    db.unwind_if_revision_cancelled();
    serde_json::to_string(&inspection).expect("flow file result serializes")
}

#[salsa::tracked(returns(clone))]
fn check_flow_query(
    db: &dyn Db,
    files: Vec<FileInput>,
    python_version: String,
    model_catalog_json: String,
    inspection_json: String,
) -> String {
    db.unwind_if_revision_cancelled();
    let model_catalog: crate::model::FlowModelCatalog =
        serde_json::from_str(&model_catalog_json).expect("flow model catalog deserializes");
    let sources: Vec<_> = files
        .iter()
        .map(|file| SourceFile::new(file.path(db), file.module(db), file.source(db)))
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
    let mut inspection: Inspection =
        serde_json::from_str(&inspection_json).expect("inspection query result deserializes");
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
    let mut module_files = BTreeMap::<String, Vec<FileInput>>::new();
    for file in files.iter().copied() {
        module_files
            .entry(file.module(db).to_string())
            .or_default()
            .push(file);
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
            for dependency in module_files.get(&module).into_iter().flatten() {
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
        let dependencies: Vec<_> = files
            .iter()
            .copied()
            .filter(|dependency| dependency_modules.contains(dependency.module(db)))
            .collect();
        let dependency_paths: BTreeSet<_> = dependencies
            .iter()
            .map(|dependency| dependency.path(db).to_string())
            .collect();
        let flow_input = Inspection {
            schema_version: inspection.schema_version,
            assurance: inspection.assurance.clone(),
            contracts: inspection
                .contracts
                .iter()
                .filter(|contract| dependency_paths.contains(&contract.source.file))
                .cloned()
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
        let applicable_models =
            serde_json::to_string(&model_catalog.for_resource_symbols(&resource_symbols))
                .expect("applicable flow models serialize");
        let flow_json = check_flow_file_query(
            db,
            target,
            dependencies,
            python_version.clone(),
            flow_budget,
            applicable_models,
            serde_json::to_string(&flow_input).expect("flow file input serializes"),
        );
        let flow: Inspection =
            serde_json::from_str(&flow_json).expect("flow file result deserializes");
        inspection.diagnostics.extend(flow.diagnostics);
        for reason in flow.coverage.reasons {
            if !inspection.coverage.reasons.contains(&reason) {
                inspection.coverage.reasons.push(reason);
            }
        }
        if flow.coverage.status == "incomplete" {
            inspection.coverage.status = "incomplete".into();
        }
    }
    db.unwind_if_revision_cancelled();
    serde_json::to_string(&inspection).expect("check result serializes")
}

#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<Mutex<salsa::CancellationToken>>);

impl CancellationToken {
    /// Request cancellation of work currently running in the associated session.
    pub fn cancel(&self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_cancelled()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisCancelled;

impl fmt::Display for AnalysisCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("analysis was cancelled before a result was available")
    }
}

impl std::error::Error for AnalysisCancelled {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInspection {
    pub revision: u64,
    pub inspection: Inspection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionCheckResult {
    pub revision: u64,
    pub result: crate::CheckResult,
}

fn rejected_check_input(files: &[SourceFile]) -> Option<Inspection> {
    let source_bytes = files.iter().fold(0usize, |total, file| {
        total.saturating_add(file.source.len())
    });
    if files.len() > crate::DEFAULT_MAX_INPUT_FILES {
        return Some(crate::invalid_input_inspection(
            files.first().map_or("<input>", |file| file.path.as_str()),
            "input file count exceeds the 10,000 file inspection limit",
            source_bytes,
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some(duplicate) = files.iter().find(|file| !seen.insert(&file.path)) {
        return Some(crate::invalid_input_inspection(
            &duplicate.path,
            "duplicate source path supplied to one inspection",
            source_bytes,
        ));
    }
    if source_bytes > crate::DEFAULT_MAX_SOURCE_BYTES {
        let path = files
            .iter()
            .scan(0usize, |total, file| {
                *total = total.saturating_add(file.source.len());
                Some((*total, file))
            })
            .find(|(total, file)| {
                *total > crate::DEFAULT_MAX_SOURCE_BYTES
                    || file.source.len() > crate::DEFAULT_MAX_SOURCE_BYTES
            })
            .map_or("<input>", |(_, file)| file.path.as_str());
        return Some(crate::invalid_input_inspection(
            path,
            "supplied source files exceed the 1,000,000 byte total inspection limit",
            source_bytes,
        ));
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionInputError {
    FileLimitExceeded,
    SourceLimitExceeded,
    TotalSourceLimitExceeded,
}

impl fmt::Display for SessionInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileLimitExceeded => f.write_str("session input exceeds the 10,000 file limit"),
            Self::SourceLimitExceeded => {
                f.write_str("session source exceeds the 1,000,000 byte file limit")
            }
            Self::TotalSourceLimitExceeded => {
                f.write_str("session inputs exceed the 1,000,000 byte total limit")
            }
        }
    }
}

impl std::error::Error for SessionInputError {}

#[derive(Default)]
pub struct AnalysisSession {
    db: Database,
    files: BTreeMap<String, FileInput>,
    revision: u64,
    target_version: Option<String>,
    model_catalog: crate::model::FlowModelCatalog,
    mutations_since_reset: usize,
    cancellation: Arc<Mutex<salsa::CancellationToken>>,
}

impl AnalysisSession {
    #[cfg(test)]
    pub(crate) fn set_flow_model_catalog(
        &mut self,
        model_catalog: crate::model::FlowModelCatalog,
    ) -> bool {
        if self.model_catalog == model_catalog {
            return false;
        }
        self.model_catalog = model_catalog;
        self.revision = self.revision.saturating_add(1);
        true
    }

    pub fn set_file(
        &mut self,
        path: String,
        module: String,
        source: String,
    ) -> Result<(), SessionInputError> {
        if source.len() > crate::DEFAULT_MAX_SOURCE_BYTES {
            return Err(SessionInputError::SourceLimitExceeded);
        }
        let old_size = self
            .files
            .get(&path)
            .map(|file| file.source(&self.db).len())
            .unwrap_or_default();
        let total_bytes = self.files.values().fold(0usize, |total, file| {
            total.saturating_add(file.source(&self.db).len())
        });
        if total_bytes
            .saturating_sub(old_size)
            .saturating_add(source.len())
            > crate::DEFAULT_MAX_SOURCE_BYTES
        {
            return Err(SessionInputError::TotalSourceLimitExceeded);
        }
        if !self.files.contains_key(&path) && self.files.len() >= crate::DEFAULT_MAX_INPUT_FILES {
            return Err(SessionInputError::FileLimitExceeded);
        }
        if !self.file_would_change(&path, &module, &source) {
            return Ok(());
        }
        self.reset_storage_if_needed();
        if self.set_file_input(path, module, source) {
            self.revision = self.revision.saturating_add(1);
            self.mutations_since_reset += 1;
        }
        Ok(())
    }

    fn file_would_change(&self, path: &str, module: &str, source: &str) -> bool {
        self.files
            .get(path)
            .is_none_or(|file| file.module(&self.db) != module || file.source(&self.db) != source)
    }

    fn reset_storage_if_needed(&mut self) {
        if self.mutations_since_reset < crate::DEFAULT_MAX_SESSION_MUTATIONS {
            return;
        }
        let inputs = self
            .files
            .iter()
            .map(|(path, file)| {
                (
                    path.clone(),
                    file.module(&self.db).to_string(),
                    file.source(&self.db).to_string(),
                )
            })
            .collect::<Vec<_>>();
        self.db = Database::default();
        self.files.clear();
        for (path, module, source) in inputs {
            let input = FileInput::new(&self.db, path.clone(), module, source);
            self.files.insert(path, input);
        }
        self.mutations_since_reset = 0;
        *self
            .cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            salsa::Database::cancellation_token(&self.db);
    }

    fn set_file_input(&mut self, path: String, module: String, source: String) -> bool {
        use salsa::Setter;
        if let Some(file) = self.files.get(&path).copied() {
            let mut changed = false;
            if file.module(&self.db) != module {
                file.set_module(&mut self.db).to(module);
                changed = true;
            }
            if file.source(&self.db) != source {
                file.set_source(&mut self.db).to(source);
                changed = true;
            }
            changed
        } else {
            let file = FileInput::new(&self.db, path.clone(), module, source);
            self.files.insert(path, file);
            true
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        if self.files.contains_key(path) {
            self.reset_storage_if_needed();
        }
        if self.files.remove(path).is_some() {
            self.revision = self.revision.saturating_add(1);
            self.mutations_since_reset += 1;
        }
    }

    /// Clear all inputs and memoized Salsa state held by this session.
    pub fn reset(&mut self) {
        self.db = Database::default();
        self.files.clear();
        self.target_version = None;
        self.revision = self.revision.saturating_add(1);
        self.mutations_since_reset = 0;
        *self
            .cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            salsa::Database::cancellation_token(&self.db);
    }

    /// Create a token that can cancel an in-flight inspection from another thread.
    pub fn cancellation_token(&self) -> CancellationToken {
        *self
            .cancellation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            salsa::Database::cancellation_token(&self.db);
        CancellationToken(Arc::clone(&self.cancellation))
    }

    pub fn parse_summary(&self, path: &str) -> Option<ParseSummary> {
        self.parse_summary_for(path, "3.11")
    }

    pub fn parse_summary_for(&self, path: &str, python_version: &str) -> Option<ParseSummary> {
        let file = self.files.get(path).copied()?;
        salsa::Cancelled::catch(|| parse_file(&self.db, file, python_version.to_string()))
            .ok()
            .cloned()
    }

    pub fn files(&self) -> impl Iterator<Item = FileInput> + '_ {
        self.files.values().copied()
    }

    pub fn inspect(&mut self, files: &[SourceFile], python_version: &str) -> Inspection {
        match self.inspect_cancellable(files, python_version) {
            Ok(result) => result.inspection,
            Err(_) => crate::cancelled_inspection(files),
        }
    }

    /// Check resource cleanup duties in the bounded supported subset.
    pub fn check(&mut self, files: &[SourceFile], python_version: &str) -> Inspection {
        match self.check_cancellable(files, python_version) {
            Ok(result) => result.inspection,
            Err(_) => {
                let mut inspection = crate::cancelled_inspection(files);
                inspection.assurance = "lifecycle-checking-for-supported-exit-flow".into();
                inspection
            }
        }
    }

    /// Check resource cleanup duties using the versioned checker result schema.
    pub fn check_report(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> crate::CheckResult {
        let inspection = self.check(files, python_version);
        crate::CheckResult::from_inspection_with_models(
            inspection,
            python_version,
            &self.model_catalog,
        )
    }

    /// Check resource cleanup duties and return the versioned result with its
    /// exact analyzed session revision. Cancellation publishes no partial result.
    pub fn check_report_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Result<SessionCheckResult, AnalysisCancelled> {
        let checked = self.check_cancellable(files, python_version)?;
        Ok(SessionCheckResult {
            revision: checked.revision,
            result: crate::CheckResult::from_inspection_with_models(
                checked.inspection,
                python_version,
                &self.model_catalog,
            ),
        })
    }

    /// Check lifecycle uses and return the exact session revision that was analyzed.
    pub fn check_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Result<SessionInspection, AnalysisCancelled> {
        if let Some(mut inspection) = rejected_check_input(files) {
            inspection.assurance = "lifecycle-checking-for-supported-exit-flow".into();
            return Ok(SessionInspection {
                revision: self.revision,
                inspection,
            });
        }
        let inspected = self.inspect_cancellable(files, python_version)?;
        let revision = inspected.revision;
        let inspection = inspected.inspection;
        let inspection_json =
            serde_json::to_string(&inspection).expect("inspection result serializes");
        let model_catalog_json =
            serde_json::to_string(&self.model_catalog).expect("flow model catalog serializes");
        let result_json = salsa::Cancelled::catch(|| {
            check_flow_query(
                &self.db,
                self.files.values().copied().collect(),
                python_version.to_string(),
                model_catalog_json,
                inspection_json,
            )
        });
        let result_json = result_json.map_err(|_| AnalysisCancelled)?;
        let mut result: Inspection =
            serde_json::from_str(&result_json).expect("check query returns valid JSON");
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
        result.assurance = "lifecycle-checking-for-supported-exit-flow".into();
        Ok(SessionInspection {
            revision,
            inspection: result,
        })
    }

    /// Inspect files and return the session revision used to compute the result.
    /// Cancellation returns an error and never publishes a partial or stale result.
    pub fn inspect_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Result<SessionInspection, AnalysisCancelled> {
        if files.len() > crate::DEFAULT_MAX_INPUT_FILES {
            let source_bytes = files.iter().fold(0usize, |total, file| {
                total.saturating_add(file.source.len())
            });
            return Ok(SessionInspection {
                revision: self.revision,
                inspection: crate::invalid_input_inspection(
                    files.first().map_or("<input>", |file| file.path.as_str()),
                    "input file count exceeds the 10,000 file inspection limit",
                    source_bytes,
                ),
            });
        }
        let mut seen_paths = std::collections::BTreeSet::new();
        if let Some(duplicate) = files.iter().find(|file| !seen_paths.insert(&file.path)) {
            return Ok(SessionInspection {
                revision: self.revision,
                inspection: crate::invalid_input_inspection(
                    &duplicate.path,
                    "duplicate source path supplied to one inspection",
                    files.iter().fold(0usize, |total, file| {
                        total.saturating_add(file.source.len())
                    }),
                ),
            });
        }
        let total_source_bytes = files.iter().fold(0usize, |total, file| {
            total.saturating_add(file.source.len())
        });
        if total_source_bytes > crate::DEFAULT_MAX_SOURCE_BYTES {
            let first_over_limit = files
                .iter()
                .scan(0usize, |total, file| {
                    *total = total.saturating_add(file.source.len());
                    Some((*total, file))
                })
                .find(|(total, file)| {
                    *total > crate::DEFAULT_MAX_SOURCE_BYTES
                        || file.source.len() > crate::DEFAULT_MAX_SOURCE_BYTES
                })
                .map(|(_, file)| file.path.as_str())
                .unwrap_or("<input>");
            return Ok(SessionInspection {
                revision: self.revision,
                inspection: crate::invalid_input_inspection(
                    first_over_limit,
                    "supplied source files exceed the 1,000,000 byte total inspection limit",
                    total_source_bytes,
                ),
            });
        }
        let target_changed = self.target_version.as_deref() != Some(python_version);
        let stale_paths = self
            .files
            .keys()
            .filter(|path| !files.iter().any(|file| &file.path == *path))
            .cloned()
            .collect::<Vec<_>>();
        for path in stale_paths {
            self.remove_file(&path);
        }

        // Apply shrinks and same-size replacements before growth. This keeps
        // every intermediate session state under the aggregate byte limit,
        // even when a valid batch moves bytes between files in opposite order.
        let (non_growing, growing): (Vec<_>, Vec<_>) = files.iter().partition(|file| {
            self.files
                .get(&file.path)
                .is_some_and(|existing| existing.source(&self.db).len() >= file.source.len())
        });
        for file in non_growing.into_iter().chain(growing) {
            self.set_file(file.path.clone(), file.module.clone(), file.source.clone())
                .expect("validated inspection inputs fit session limits");
        }
        self.target_version = Some(python_version.to_string());
        if target_changed {
            self.revision = self.revision.saturating_add(1);
        }
        let result = salsa::Cancelled::catch(|| {
            inspect_project_query(
                &self.db,
                self.files.values().copied().collect(),
                python_version.to_string(),
            )
        })
        .map_err(|_| AnalysisCancelled)?;
        let inspection = serde_json::from_str(&result).expect("project query returns valid JSON");
        Ok(SessionInspection {
            revision: self.revision,
            inspection,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn unchanged_session_inspection_reuses_the_project_query() {
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                observed.fetch_add(1, Ordering::Relaxed);
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let file = SourceFile::new("module.py", "module", "class C: pass\n");
        let other = SourceFile::new("other.py", "other", "class Other: pass\n");

        session.inspect(&[file.clone(), other.clone()], "3.11");
        let cold_executions = executions.load(Ordering::Relaxed);
        assert!(cold_executions >= 3);
        session.inspect(&[file.clone(), other.clone()], "3.11");
        assert_eq!(executions.load(Ordering::Relaxed), cold_executions);

        let edited = SourceFile::new("module.py", "module", "class D: pass\n");
        session.inspect(&[edited, other], "3.11");
        assert_eq!(executions.load(Ordering::Relaxed) - cold_executions, 4);
    }

    #[test]
    fn unchanged_session_check_reuses_the_check_query() {
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                observed.fetch_add(1, Ordering::Relaxed);
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let file = SourceFile::new(
            "module.py",
            "module",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n",
        );

        let first = session.check(std::slice::from_ref(&file), "3.11");
        let executions_after_first_check = executions.load(Ordering::Relaxed);
        assert!(executions_after_first_check >= 3);
        let second = session.check(std::slice::from_ref(&file), "3.11");

        assert_eq!(second, first);
        assert_eq!(
            executions.load(Ordering::Relaxed),
            executions_after_first_check
        );
    }

    #[test]
    fn contextlib_model_selection_refreshes_when_target_version_changes() {
        let file = SourceFile::new(
            "module.py",
            "module",
            "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n",
        );
        let mut session = AnalysisSession::default();
        let supported = session
            .check_report_cancellable(std::slice::from_ref(&file), "3.11")
            .expect("supported-target check completes");
        assert!(supported.result.diagnostics.is_empty(), "{supported:#?}");
        assert_eq!(supported.result.available_models.len(), 2, "{supported:#?}");
        assert_eq!(
            supported.result.coverage.status,
            "complete-for-supported-subset"
        );

        let unsupported = session
            .check_report_cancellable(std::slice::from_ref(&file), "3.14")
            .expect("unsupported-target check completes with a boundary");
        assert!(unsupported.revision > supported.revision);
        assert!(unsupported.result.available_models.is_empty());
        assert!(unsupported.result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }));
        assert_eq!(unsupported.result.coverage.status, "incomplete");

        let fresh = AnalysisSession::default().check_report(&[file], "3.14");
        assert_eq!(unsupported.result, fresh);
    }

    #[test]
    fn reviewed_exceptional_close_outcomes_flow_through_session_results() {
        let file = SourceFile::new(
            "model.py",
            "model",
            "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self):\n        raise RuntimeError()\ndef cleanup():\n    resource = Connection()\n    try:\n        resource.close()\n    except RuntimeError:\n        pass\n",
        );

        for (id, postcondition, expected_certainty, expected_coverage) in [
            (
                "fixture-exceptionally-closed",
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                "fixture-exceptionally-open",
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                "fixture-exceptionally-unknown",
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: id.into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "close".into(),
                    source_revision: "fixture-v1".into(),
                    source_url: "fixture://model.Connection".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: false,
                    normal: Some(crate::model::LifecyclePostcondition::Closed),
                    exceptional: Some(postcondition),
                    cancelled: None,
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["exceptional lifecycle postcondition".into()],
                    invalid_cases: vec!["open postcondition retains a duty".into()],
                    unknown_cases: vec!["unknown postcondition weakens certainty".into()],
                })
                .unwrap();
            let mut session = AnalysisSession::default();
            assert!(session.set_flow_model_catalog(catalog));

            let checked = session.check_report(std::slice::from_ref(&file), "3.11");
            let leaks = checked
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{id}: {checked:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{id}: {checked:#?}");
                }
                None => assert!(leaks.is_empty(), "{id}: {checked:#?}"),
            }
            assert_eq!(
                checked.coverage.status, expected_coverage,
                "{id}: {checked:#?}"
            );
            let selected = checked
                .available_models
                .iter()
                .find(|model| model.id == id)
                .expect("the selected outcome model is visible in the result");
            assert!(selected
                .modeled_behavior
                .contains(&format!("exceptional=Some({postcondition:?})")));
            assert!(selected.qualification.contains("reviewer=fixture-test"));
        }
    }

    #[test]
    fn reviewed_normal_close_outcomes_flow_through_session_results() {
        let file = SourceFile::new(
            "model.py",
            "model",
            "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n",
        );

        for (id, postcondition, expected_certainty, expected_coverage) in [
            (
                "fixture-normally-closed",
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                "fixture-normally-open",
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                "fixture-normal-preserve",
                crate::model::LifecyclePostcondition::Preserve,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                "fixture-normal-unknown",
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: id.into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "close".into(),
                    source_revision: "fixture-v1".into(),
                    source_url: "fixture://model.Connection".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: false,
                    normal: Some(postcondition),
                    exceptional: None,
                    cancelled: None,
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["normal lifecycle postcondition".into()],
                    invalid_cases: vec!["open postcondition retains a duty".into()],
                    unknown_cases: vec!["unknown postcondition weakens certainty".into()],
                })
                .unwrap();
            let mut session = AnalysisSession::default();
            assert!(session.set_flow_model_catalog(catalog));

            let checked = session.check_report(std::slice::from_ref(&file), "3.11");
            let leaks = checked
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{id}: {checked:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{id}: {checked:#?}");
                }
                None => assert!(leaks.is_empty(), "{id}: {checked:#?}"),
            }
            assert_eq!(
                checked.coverage.status, expected_coverage,
                "{id}: {checked:#?}"
            );
            let selected = checked
                .available_models
                .iter()
                .find(|model| model.id == id)
                .expect("the selected normal outcome model is visible in the result");
            assert!(selected
                .modeled_behavior
                .contains(&format!("normal=Some({postcondition:?})")));
            assert!(selected.qualification.contains("reviewer=fixture-test"));
        }
    }

    #[test]
    fn reviewed_async_cancelled_close_outcomes_flow_through_session_results() {
        let file = SourceFile::new(
            "model.py",
            "model",
            "from typepact import resource\n@resource(closed_by='aclose')\nclass Connection:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = Connection()\n    await resource.aclose()\n",
        );

        for (id, postcondition, expected_certainty, expected_coverage) in [
            (
                "fixture-cancelled-closed",
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                "fixture-cancelled-open",
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                "fixture-cancelled-unknown",
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: id.into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "aclose".into(),
                    source_revision: "fixture-v1".into(),
                    source_url: "fixture://model.Connection".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: true,
                    normal: Some(crate::model::LifecyclePostcondition::Closed),
                    exceptional: None,
                    cancelled: Some(postcondition),
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["cancelled lifecycle postcondition".into()],
                    invalid_cases: vec!["open postcondition retains a duty".into()],
                    unknown_cases: vec!["unknown postcondition weakens certainty".into()],
                })
                .unwrap();
            let mut session = AnalysisSession::default();
            assert!(session.set_flow_model_catalog(catalog));

            let checked = session.check_report(std::slice::from_ref(&file), "3.11");
            let leaks = checked
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{id}: {checked:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{id}: {checked:#?}");
                    assert_eq!(leaks[0].completion.as_deref(), Some("cancelled"));
                }
                None => assert!(leaks.is_empty(), "{id}: {checked:#?}"),
            }
            assert_eq!(
                checked.coverage.status, expected_coverage,
                "{id}: {checked:#?}"
            );
            let selected = checked
                .available_models
                .iter()
                .find(|model| model.id == id)
                .expect("the selected async outcome model is visible in the result");
            assert!(selected
                .modeled_behavior
                .contains(&format!("cancelled=Some({postcondition:?})")));
            assert!(selected.qualification.contains("reviewer=fixture-test"));
        }
    }

    #[test]
    fn resource_model_edits_invalidate_reused_cleanup_queries() {
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{database_key:?}"));
            }
        })));
        let source = "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n    resource.close()\n";
        let file = SourceFile::new("model.py", "model", source);
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let initial = session.check(&[file.clone(), unrelated.clone()], "3.11");
        assert!(initial.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message == "repeat-close behavior is not modeled"
        }));

        let model = crate::model::ResourceEffectModel {
            id: "fixture-idempotent-close".into(),
            qualified_symbol: "model.Connection".into(),
            close_method: "close".into(),
            source_revision: "fixture-v1".into(),
            source_url: "fixture://model.Connection".into(),
            supported_python_versions: BTreeSet::from(["3.11".into()]),
            platform_assumptions: vec!["platform-independent fixture".into()],
            argument_aliases: Vec::new(),
            result_alias: None,
            duty_handoff: "none".into(),
            suppression: "not-a-context-manager".into(),
            asynchronous: false,
            normal: Some(crate::model::LifecyclePostcondition::Closed),
            exceptional: None,
            cancelled: None,
            repeat_close: crate::model::RepeatClosePolicy::Idempotent,
            post_close_methods: BTreeSet::new(),
            trust: "reviewed-fixture".into(),
            reviewer: "fixture-test".into(),
            valid_cases: vec!["idempotent repeat close".into()],
            invalid_cases: vec!["overlapping model version".into()],
            unknown_cases: vec!["unsupported target version".into()],
        };
        let mut catalog = crate::model::FlowModelCatalog::default();
        catalog.insert(model.clone()).unwrap();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        assert!(session.set_flow_model_catalog(catalog.clone()));
        let modeled = session.check(&[file.clone(), unrelated.clone()], "3.11");
        assert!(!modeled.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message == "repeat-close behavior is not modeled"
        }));
        assert_eq!(modeled.coverage.status, "complete-for-supported-subset");
        let modeled_report = session.check_report(&[file.clone(), unrelated.clone()], "3.11");
        let fixture_metadata = modeled_report
            .available_models
            .iter()
            .find(|model| model.id == "fixture-idempotent-close")
            .expect("selected fixture model is visible in check metadata");
        assert!(fixture_metadata.modeled_behavior.contains("Idempotent"));
        assert!(fixture_metadata
            .qualification
            .contains("reviewer=fixture-test"));
        let executed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            executed_flow_queries.len(),
            1,
            "only the file with the modeled resource should be rechecked: {executed_flow_queries:#?}"
        );

        let mut requires_open_model = model.clone();
        requires_open_model.id = "fixture-requires-open-close".into();
        requires_open_model.repeat_close = crate::model::RepeatClosePolicy::RequiresOpen;
        let mut requires_open_catalog = crate::model::FlowModelCatalog::default();
        requires_open_catalog.insert(requires_open_model).unwrap();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        assert!(session.set_flow_model_catalog(requires_open_catalog.clone()));
        let requires_open = session.check(&[file.clone(), unrelated.clone()], "3.11");
        assert!(requires_open.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1001"
                && diagnostic.message == "resource close requires an open resource"
                && diagnostic.certainty == "definite"
        }));
        assert!(!requires_open.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message == "repeat-close behavior is not modeled"
        }));
        assert_eq!(
            requires_open.coverage.status,
            "complete-for-supported-subset"
        );
        let requires_open_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            requires_open_flow_queries.len(),
            1,
            "changing close policy should refresh only the modeled file: {requires_open_flow_queries:#?}"
        );

        let unsupported = session.check(&[file.clone(), unrelated.clone()], "3.12");
        assert!(unsupported.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message == "repeat-close behavior is not modeled"
        }));
        assert!(!session
            .check_report(&[file.clone(), unrelated.clone()], "3.12")
            .available_models
            .iter()
            .any(|model| model.id == "fixture-idempotent-close"));

        let mut fresh = AnalysisSession::default();
        fresh.set_flow_model_catalog(catalog.clone());
        assert_eq!(
            modeled,
            fresh.check(&[file.clone(), unrelated.clone()], "3.11")
        );
        let mut fresh_requires_open = AnalysisSession::default();
        fresh_requires_open.set_flow_model_catalog(requires_open_catalog);
        assert_eq!(
            requires_open,
            fresh_requires_open.check(&[file.clone(), unrelated.clone()], "3.11")
        );
        assert_eq!(unsupported, fresh.check(&[file, unrelated], "3.12"));
    }

    #[test]
    fn unrelated_edit_reuses_other_file_lifecycle_checks() {
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let consumer = SourceFile::new(
            "consumer.py",
            "consumer",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nvalue = C()\nvalue.close()\nvalue.status()\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first = session.check(&[consumer.clone(), unrelated.clone()], "3.11");
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited = SourceFile::new("unrelated.py", "unrelated", "value = 2\n");
        let after = session.check(&[consumer.clone(), edited.clone()], "3.11");
        let executed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(
            executed_flow_queries.len(),
            1,
            "only the edited file's lifecycle query should execute: {executed_flow_queries:#?}"
        );
        assert_eq!(after, crate::check(&[consumer, edited], "3.11"));
    }

    #[test]
    fn imported_effect_edits_refresh_callers_but_reuse_unrelated_checks() {
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\n",
        );
        let client = SourceFile::new(
            "client.py",
            "client",
            "from provider import C, shutdown\nvalue = C()\nshutdown(value)\nvalue.status()\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first = session.check(
            &[provider.clone(), client.clone(), unrelated.clone()],
            "3.11",
        );
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    pass\n",
        );
        let after = session.check(
            &[edited_provider.clone(), client.clone(), unrelated.clone()],
            "3.11",
        );
        let executed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();

        assert!(!after
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(
            executed_flow_queries.len(),
            2,
            "provider and its caller should refresh; unrelated checks should reuse: {executed_flow_queries:#?}"
        );
        assert_eq!(
            after,
            crate::check(&[edited_provider, client, unrelated], "3.11")
        );
    }

    #[test]
    fn conditional_and_model_edits_refresh_imported_lifecycle_findings() {
        let mut session = AnalysisSession::default();
        let client = SourceFile::new(
            "client.py",
            "client",
            "from provider import C, shutdown\nvalue = C()\nshutdown(value)\nvalue.status()\n",
        );
        let provider_source = |lifecycle: &str, effect: &str| {
            format!(
                "from typepact import resource\n@resource(closed_by='{lifecycle}')\nclass C:\n    def close(self): pass\n    def finish(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n{effect}"
            )
        };
        let open_provider = SourceFile::new(
            "provider.py",
            "provider",
            provider_source("close", "    pass\n"),
        );
        let open = session.check(&[open_provider.clone(), client.clone()], "3.11");
        assert!(!open
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));

        let conditional_provider = SourceFile::new(
            "provider.py",
            "provider",
            provider_source("close", "    if should_close:\n        value.close()\n"),
        );
        let conditional = session.check(&[conditional_provider.clone(), client.clone()], "3.11");
        assert!(conditional.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1001" && diagnostic.certainty == "possible"
        }));
        assert_eq!(
            conditional,
            crate::check(&[conditional_provider.clone(), client.clone()], "3.11")
        );

        let definite_provider = SourceFile::new(
            "provider.py",
            "provider",
            provider_source("close", "    value.close()\n"),
        );
        let definite = session.check(&[definite_provider.clone(), client.clone()], "3.11");
        assert!(definite.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1001" && diagnostic.certainty == "definite"
        }));
        assert_eq!(
            definite,
            crate::check(&[definite_provider.clone(), client.clone()], "3.11")
        );

        let changed_model = SourceFile::new(
            "provider.py",
            "provider",
            provider_source("finish", "    value.close()\n"),
        );
        let after_model_edit = session.check(&[changed_model.clone(), client.clone()], "3.11");
        assert!(!after_model_edit
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(
            after_model_edit,
            crate::check(&[changed_model, client], "3.11")
        );
    }

    #[test]
    fn adding_and_removing_a_lifecycle_provider_matches_fresh_checks() {
        let mut session = AnalysisSession::default();
        let client = SourceFile::new(
            "client.py",
            "client",
            "from provider import Connection\nvalue = Connection()\nvalue.close()\nvalue.status()\n",
        );
        let before = session.check(std::slice::from_ref(&client), "3.11");
        assert_eq!(before, crate::check(std::slice::from_ref(&client), "3.11"));
        assert!(!before
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));

        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
        );
        let added = session.check(&[client.clone(), provider.clone()], "3.11");
        assert!(added
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(added, crate::check(&[provider, client.clone()], "3.11"));

        let removed = session.check(std::slice::from_ref(&client), "3.11");
        assert_eq!(removed, crate::check(std::slice::from_ref(&client), "3.11"));
        assert_eq!(removed.coverage.status, "incomplete");
        assert!(!removed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
    }

    #[test]
    fn moving_an_imported_close_refreshes_the_related_lifecycle_span() {
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\n",
        );
        let client = SourceFile::new(
            "client.py",
            "client",
            "from provider import C, shutdown\nvalue = C()\nshutdown(value)\nvalue.status()\n",
        );
        let first = session.check(&[provider.clone(), client.clone()], "3.11");
        let first_use = first
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "TP1001")
            .expect("the imported helper closes the resource");
        assert_eq!(first_use.primary.file, "client.py");
        let first_close = first_use
            .related
            .first()
            .expect("the finding retains the provider close origin")
            .clone();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let moved_provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\n\ndef shutdown(value):\n    value.close()\n",
        );
        let moved = session.check(&[moved_provider.clone(), client.clone()], "3.11");
        let moved_use = moved
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "TP1001")
            .expect("the imported helper still closes the resource");
        let moved_close = moved_use
            .related
            .first()
            .expect("the refreshed finding retains its provider origin");

        assert_eq!(moved_use.primary, first_use.primary);
        assert_eq!(moved_use.certainty, first_use.certainty);
        assert_eq!(moved_close.file, first_close.file);
        assert_eq!(
            moved_close.line,
            first_close.line + 1,
            "before: {first:#?}\nafter: {moved:#?}"
        );
        let executed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            executed_flow_queries.len(),
            2,
            "provider and its caller refresh locations: {executed_flow_queries:#?}"
        );
        assert_eq!(moved, crate::check(&[moved_provider, client], "3.11"));
    }

    #[test]
    fn lifecycle_callee_edits_match_a_fresh_check() {
        let mut session = AnalysisSession::default();
        let before = SourceFile::new(
            "module.py",
            "module",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\nconnection = C()\nshutdown(connection)\nconnection.status()\n",
        );
        let first = session.check(std::slice::from_ref(&before), "3.11");
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));

        let after = SourceFile::new(
            "module.py",
            "module",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    pass\nconnection = C()\nshutdown(connection)\nconnection.status()\n",
        );
        let updated = session.check(std::slice::from_ref(&after), "3.11");

        assert!(!updated
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(updated, crate::check(&[after], "3.11"));
    }

    #[test]
    fn batch_update_applies_new_files_after_existing_file_shrinks() {
        let mut session = AnalysisSession::default();
        let existing = SourceFile::new("existing.py", "existing", "#".repeat(800_000));
        session.inspect(std::slice::from_ref(&existing), "3.11");

        let added = SourceFile::new("added.py", "added", "#".repeat(800_000));
        let shrunk = SourceFile::new("existing.py", "existing", "#".repeat(100_000));
        let files = [added, shrunk];
        let session_result = session.inspect(&files, "3.11");

        assert_eq!(session_result.coverage.source_bytes, 900_000);
        assert_eq!(session_result, crate::inspect(&files, "3.11"));
    }

    #[test]
    fn dependency_body_edit_does_not_recompute_consumer_semantics() {
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                observed.fetch_add(1, Ordering::Relaxed);
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new("provider.py", "provider", "class C: pass\n");
        let consumer = SourceFile::new(
            "consumer.py",
            "consumer",
            "from provider import C\ndef use(value: C): pass\n",
        );
        let before = session.inspect(&[provider.clone(), consumer.clone()], "3.11");
        assert_eq!(before.contracts.len(), 1, "{before:#?}");

        executions.store(0, Ordering::Relaxed);
        let edited_provider = SourceFile::new(
            "provider.py",
            "provider",
            "class C: pass\ndef helper(): return 1\n",
        );
        let after = session.inspect(&[edited_provider, consumer.clone()], "3.11");
        assert_eq!(after.contracts, before.contracts);
        assert_eq!(after.diagnostics, before.diagnostics);
        assert_eq!(after.coverage.declarations, before.coverage.declarations);
        assert_eq!(
            after.coverage.source_bytes,
            before.coverage.source_bytes + 23
        );
        assert_eq!(executions.load(Ordering::Relaxed), 4);

        let fresh = crate::inspect(
            &[
                consumer,
                SourceFile::new(
                    "provider.py",
                    "provider",
                    "class C: pass\ndef helper(): return 1\n",
                ),
            ],
            "3.11",
        );
        assert_eq!(after, fresh);
    }

    #[test]
    fn changed_export_invalidates_dependent_contracts() {
        let mut session = AnalysisSession::default();
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
        );
        let consumer = SourceFile::new(
            "consumer.py",
            "consumer",
            "from provider import C\ndef use(value: C): pass\n",
        );
        let first = session.inspect(&[provider.clone(), consumer.clone()], "3.11");
        let consumer_contract = first
            .contracts
            .iter()
            .find(|contract| contract.display_name == "use.value")
            .expect("consumer annotation is inspected");
        assert_eq!(
            consumer_contract.lifecycle.as_ref().unwrap().closed_by,
            "close"
        );

        let updated_provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by=\"dispose\")\nclass C: pass\n",
        );
        let second = session.inspect(&[updated_provider.clone(), consumer.clone()], "3.11");
        let consumer_contract = second
            .contracts
            .iter()
            .find(|contract| contract.display_name == "use.value")
            .expect("consumer annotation remains inspected");
        assert_eq!(
            consumer_contract.lifecycle.as_ref().unwrap().closed_by,
            "dispose"
        );
        assert_eq!(
            second,
            crate::inspect(&[consumer, updated_provider], "3.11")
        );
    }

    #[test]
    fn moved_exported_spans_refresh_without_changing_consumer_facts() {
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                observed.fetch_add(1, Ordering::Relaxed);
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from typepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
        );
        let consumer = SourceFile::new(
            "consumer.py",
            "consumer",
            "from provider import C\ndef use(value: C): pass\n",
        );
        let first = session.inspect(&[provider.clone(), consumer.clone()], "3.11");
        let first_provider_span = first
            .contracts
            .iter()
            .find(|contract| contract.display_name == "C")
            .unwrap()
            .source
            .clone();
        let first_consumer = first
            .contracts
            .iter()
            .find(|contract| contract.display_name == "use.value")
            .unwrap()
            .clone();

        executions.store(0, Ordering::Relaxed);
        let moved_provider = SourceFile::new(
            "provider.py",
            "provider",
            "# shifted\nfrom typepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
        );
        let moved = session.inspect(&[moved_provider, consumer], "3.11");
        let moved_provider_span = moved
            .contracts
            .iter()
            .find(|contract| contract.display_name == "C")
            .unwrap()
            .source
            .clone();
        let moved_consumer = moved
            .contracts
            .iter()
            .find(|contract| contract.display_name == "use.value")
            .unwrap();
        assert_eq!(moved_provider_span.line, first_provider_span.line + 1);
        assert_eq!(moved_consumer, &first_consumer);
        assert_eq!(executions.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn cancellation_discards_the_result_and_the_session_recovers() {
        let mut session = AnalysisSession::default();
        let token = session.cancellation_token();
        token.cancel();
        let file = SourceFile::new("module.py", "module", "class C: pass\n");
        assert!(matches!(
            session.inspect_cancellable(std::slice::from_ref(&file), "3.11"),
            Err(AnalysisCancelled)
        ));

        let updated = SourceFile::new("module.py", "module", "class D: pass\n");
        let recovered = session
            .inspect_cancellable(&[updated], "3.11")
            .expect("the session is reusable after cancellation");
        assert_eq!(
            recovered.inspection.coverage.status,
            "complete-for-supported-subset"
        );
        assert_eq!(recovered.inspection.contracts, Vec::new());
    }

    #[test]
    fn cancelled_cleanup_check_discards_the_result_and_recovers() {
        let mut session = AnalysisSession::default();
        let token = session.cancellation_token();
        token.cancel();
        let leaking = SourceFile::new(
            "module.py",
            "module",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n",
        );
        assert!(matches!(
            session.check_report_cancellable(std::slice::from_ref(&leaking), "3.11"),
            Err(AnalysisCancelled)
        ));

        let cleaned = SourceFile::new(
            "module.py",
            "module",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    resource.close()\n",
        );
        let recovered = session
            .check_report_cancellable(std::slice::from_ref(&cleaned), "3.11")
            .expect("the checker session recovers after request cancellation");
        let fresh = AnalysisSession::default()
            .check_report_cancellable(std::slice::from_ref(&cleaned), "3.11")
            .unwrap();
        assert_eq!(recovered.result, fresh.result);
        assert_eq!(
            recovered.result.coverage.status,
            "complete-for-supported-subset"
        );
        assert!(!recovered
            .result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"));
    }

    #[test]
    fn cancellation_token_survives_automatic_session_cache_reset() {
        let mut session = AnalysisSession::default();
        session
            .set_file(
                "module.py".into(),
                "module".into(),
                "class C0: pass\n".into(),
            )
            .unwrap();
        let token = session.cancellation_token();
        let mut latest = String::new();
        for index in 0..=crate::DEFAULT_MAX_SESSION_MUTATIONS {
            latest = format!("class C{index}: pass\n");
            session
                .set_file("module.py".into(), "module".into(), latest.clone())
                .unwrap();
        }
        token.cancel();
        let file = SourceFile::new("module.py", "module", latest);
        assert!(matches!(
            session.inspect_cancellable(&[file], "3.11"),
            Err(AnalysisCancelled)
        ));
    }

    #[test]
    fn session_revision_is_stable_for_noops_and_changes_with_settings() {
        let mut session = AnalysisSession::default();
        let file = SourceFile::new("module.py", "module", "class C: pass\n");
        let initial = session
            .inspect_cancellable(std::slice::from_ref(&file), "3.11")
            .unwrap()
            .revision;
        let unchanged = session
            .inspect_cancellable(std::slice::from_ref(&file), "3.11")
            .unwrap()
            .revision;
        let changed_target = session
            .inspect_cancellable(std::slice::from_ref(&file), "3.12")
            .unwrap()
            .revision;
        assert_eq!(initial, unchanged);
        assert!(changed_target > unchanged);
    }

    #[test]
    fn request_wide_ast_declaration_budget_stops_before_semantic_lowering() {
        let source = "x = 1\n".repeat(17_000);
        let files = [
            SourceFile::new("a.py", "a", source.clone()),
            SourceFile::new("b.py", "b", source.clone()),
            SourceFile::new("c.py", "c", source),
        ];
        let result = AnalysisSession::default().inspect(&files, "3.11");
        assert_eq!(result.coverage.status, "incomplete");
        assert!(result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("50,000 statement request limit")));
        assert_eq!(result.coverage.declarations, 0);
    }

    #[test]
    fn inspection_applies_file_size_swaps_without_exceeding_intermediate_limit() {
        fn comment_file(path: &str, module: &str, bytes: usize) -> SourceFile {
            let body = "x".repeat(bytes - 2);
            SourceFile::new(path, module, format!("#{body}\n"))
        }

        let mut session = AnalysisSession::default();
        let first = [
            comment_file("a.py", "a", 100_002),
            comment_file("b.py", "b", 800_002),
        ];
        let before = session.inspect(&first, "3.11");
        assert_eq!(before.coverage.source_bytes, 900_004);

        let swapped = [
            comment_file("a.py", "a", 800_002),
            comment_file("b.py", "b", 100_002),
        ];
        let after = session.inspect(&swapped, "3.11");
        assert_eq!(after.coverage.source_bytes, 900_004);
        assert_eq!(after.coverage.status, "complete-for-supported-subset");
        assert_eq!(session.revision, 5);
    }

    #[test]
    fn tracked_parse_uses_the_selected_python_grammar() {
        let mut session = AnalysisSession::default();
        session
            .set_file(
                "aliases.py".into(),
                "aliases".into(),
                "type Alias[T] = list[T]\n".into(),
            )
            .unwrap();
        assert!(!session.parse_summary_for("aliases.py", "3.11").unwrap().ok);
        assert!(session.parse_summary_for("aliases.py", "3.12").unwrap().ok);
    }

    #[test]
    fn flow_budget_is_an_input_to_incremental_file_queries() {
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            revision: 0,
            target_version: None,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let mut expensive = String::from("def f0():\n    pass\n");
        for level in 1..=6 {
            expensive.push_str(&format!("def f{level}():\n"));
            for _ in 0..10 {
                expensive.push_str(&format!("    f{}()\n", level - 1));
            }
        }
        expensive.push_str("f6()\n");
        let expensive = SourceFile::new("a.py", "a", expensive);
        let resource = SourceFile::new(
            "b.py",
            "b",
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nc = C()\nc.close()\nc.status()\n",
        );
        let first = session.check(&[expensive.clone(), resource.clone()], "3.11");
        assert_eq!(first.coverage.status, "incomplete");
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let edited = SourceFile::new("a.py", "a", "pass\n");
        let warm = session.check(&[edited.clone(), resource.clone()], "3.11");
        let executed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        let fresh = crate::check(&[edited, resource], "3.11");
        assert_eq!(warm, fresh);
        assert_eq!(executed_flow_queries.len(), 1, "{executed_flow_queries:#?}");
        assert!(warm
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(warm.coverage.status, "complete-for-supported-subset");
        assert!(warm
            .coverage
            .reasons
            .iter()
            .all(|reason| !reason.contains("flow work budget exceeded")));
    }
}

use crate::{frontend::Declaration, Inspection, SourceFile};
use std::collections::BTreeMap;
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
            absolute_imports: Vec::new(),
        };
    }
    let parsed = crate::frontend::parse_frontend(file.source(db), &python_version);
    db.unwind_if_revision_cancelled();
    match parsed {
        Ok(frontend) => ParseSummary {
            ok: true,
            error: None,
            error_offset: None,
            declarations: Some(frontend.declarations),
            absolute_imports: frontend.absolute_imports,
        },
        Err((error, error_offset)) => ParseSummary {
            ok: false,
            error: Some(error),
            error_offset: Some(error_offset),
            declarations: None,
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
    mutations_since_reset: usize,
    cancellation: Arc<Mutex<salsa::CancellationToken>>,
}

impl AnalysisSession {
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
        for file in files {
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
}

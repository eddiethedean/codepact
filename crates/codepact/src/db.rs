use crate::{
    frontend::{Declaration, FlowStatement},
    Inspection, SourceFile,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

mod check;
mod queries;
use queries::inspect_project_query;
pub use queries::parse_file;

#[salsa::input]
pub struct FileInput {
    #[returns(deref)]
    pub path: String,
    #[returns(deref)]
    pub module: String,
    #[returns(deref)]
    pub source: String,
    /// Source consumed by semantic queries. Policy-only comments are removed
    /// while preserving lines and code columns; raw source remains for reports.
    #[returns(deref)]
    pub semantic_source: String,
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
    total_source_bytes: usize,
    revision: u64,
    target_version: Option<String>,
    ownership: crate::Ownership,
    finite_matches: bool,
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
        self.set_file_values(Cow::Owned(path), Cow::Owned(module), Cow::Owned(source))
    }

    fn set_file_ref(
        &mut self,
        path: &str,
        module: &str,
        source: &str,
    ) -> Result<(), SessionInputError> {
        self.set_file_values(
            Cow::Borrowed(path),
            Cow::Borrowed(module),
            Cow::Borrowed(source),
        )
    }

    fn set_file_values(
        &mut self,
        path: Cow<'_, str>,
        module: Cow<'_, str>,
        source: Cow<'_, str>,
    ) -> Result<(), SessionInputError> {
        if source.len() > crate::DEFAULT_MAX_SOURCE_BYTES {
            return Err(SessionInputError::SourceLimitExceeded);
        }
        let old_size = self
            .files
            .get(path.as_ref())
            .map(|file| file.source(&self.db).len())
            .unwrap_or_default();
        let updated_total = self
            .total_source_bytes
            .saturating_sub(old_size)
            .saturating_add(source.len());
        if updated_total > crate::DEFAULT_MAX_SOURCE_BYTES {
            return Err(SessionInputError::TotalSourceLimitExceeded);
        }
        if !self.files.contains_key(path.as_ref())
            && self.files.len() >= crate::DEFAULT_MAX_INPUT_FILES
        {
            return Err(SessionInputError::FileLimitExceeded);
        }
        if !self.file_would_change(path.as_ref(), module.as_ref(), source.as_ref()) {
            return Ok(());
        }
        self.reset_storage_if_needed();
        if self.set_file_input(path.into_owned(), module.into_owned(), source.into_owned()) {
            self.total_source_bytes = updated_total;
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
        self.total_source_bytes = 0;
        for (path, module, source) in inputs {
            let semantic_source = crate::frontend::semantic_source(&source);
            let input = FileInput::new(&self.db, path.clone(), module, source, semantic_source);
            self.total_source_bytes = self
                .total_source_bytes
                .saturating_add(input.source(&self.db).len());
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
        let semantic_source = crate::frontend::semantic_source(&source);
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
            if file.semantic_source(&self.db) != semantic_source {
                file.set_semantic_source(&mut self.db).to(semantic_source);
                changed = true;
            }
            changed
        } else {
            let file = FileInput::new(&self.db, path.clone(), module, source, semantic_source);
            self.files.insert(path, file);
            true
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        if self.files.contains_key(path) {
            self.reset_storage_if_needed();
        }
        if let Some(file) = self.files.remove(path) {
            self.total_source_bytes = self
                .total_source_bytes
                .saturating_sub(file.source(&self.db).len());
            self.revision = self.revision.saturating_add(1);
            self.mutations_since_reset += 1;
        }
    }

    /// Clear all inputs and memoized Salsa state held by this session.
    pub fn reset(&mut self) {
        self.db = Database::default();
        self.files.clear();
        self.total_source_bytes = 0;
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

    /// Check lifecycle and synchronous must-use contracts for the 0.4
    /// adoption surface. The existing `check` API keeps its 0.3 rule set.
    pub fn check_with_must_use(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Inspection {
        match self.check_with_must_use_cancellable(files, python_version) {
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
        self.check_cancellable_with_options(
            files,
            python_version,
            crate::Ownership::Disabled,
            None,
            check::CheckFeatures {
                must_use: false,
                finite_matches: false,
                review: false,
            },
        )
        .map(|(checked, _, _)| checked)
    }

    pub fn check_with_must_use_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Result<SessionInspection, AnalysisCancelled> {
        self.check_cancellable_with_options(
            files,
            python_version,
            crate::Ownership::Disabled,
            None,
            check::CheckFeatures {
                must_use: true,
                finite_matches: false,
                review: false,
            },
        )
        .map(|(checked, _, _)| checked)
    }

    pub fn check_with_ownership(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        ownership: crate::Ownership,
    ) -> crate::CheckResult {
        self.check_with_ownership_cancellable(files, python_version, ownership)
            .map(|checked| checked.result)
            .unwrap_or_else(|_| {
                crate::CheckResult::from_inspection(
                    crate::cancelled_inspection(files),
                    python_version,
                )
            })
    }

    pub fn check_with_ownership_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        ownership: crate::Ownership,
    ) -> Result<SessionCheckResult, AnalysisCancelled> {
        self.check_with_options_cancellable(
            files,
            python_version,
            crate::CheckOptions {
                ownership,
                ..crate::CheckOptions::default()
            },
        )
    }

    pub fn check_with_options(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: crate::CheckOptions,
    ) -> crate::CheckResult {
        self.check_with_options_cancellable(files, python_version, options)
            .map(|checked| checked.result)
            .unwrap_or_else(|_| {
                crate::CheckResult::from_inspection(
                    crate::cancelled_inspection(files),
                    python_version,
                )
            })
    }

    pub fn check_with_options_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: crate::CheckOptions,
    ) -> Result<SessionCheckResult, AnalysisCancelled> {
        self.run_check(files, python_version, options, None, false)
            .map(|(checked, _)| checked)
    }

    pub fn check_with_foundations(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: crate::CheckOptions,
        inputs: crate::foundation::FoundationInputs,
    ) -> crate::CheckResult {
        self.check_with_foundations_cancellable(files, python_version, options, inputs)
            .map(|checked| checked.result)
            .unwrap_or_else(|_| {
                crate::CheckResult::from_inspection(
                    crate::cancelled_inspection(files),
                    python_version,
                )
            })
    }

    pub fn check_with_foundations_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: crate::CheckOptions,
        inputs: crate::foundation::FoundationInputs,
    ) -> Result<SessionCheckResult, AnalysisCancelled> {
        self.run_check(files, python_version, options, Some(&inputs), false)
            .map(|(checked, _)| checked)
    }

    pub(crate) fn check_with_review_data(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: crate::CheckOptions,
        inputs: Option<&crate::foundation::FoundationInputs>,
    ) -> Result<(SessionCheckResult, crate::review::operations::Journal), AnalysisCancelled> {
        self.run_check(files, python_version, options, inputs, true)
    }

    /// Inspect files and return the session revision used to compute the result.
    /// Cancellation returns an error and never publishes a partial or stale result.
    pub fn inspect_cancellable(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
    ) -> Result<SessionInspection, AnalysisCancelled> {
        self.inspect_cancellable_with_must_use(files, python_version, false, false, None)
    }

    fn inspect_cancellable_with_must_use(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        run_must_use: bool,
        finite_matches: bool,
        foundations: Option<Arc<crate::foundation::PreparedFoundations>>,
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
        let mut incoming_paths = HashSet::with_capacity(files.len());
        if let Some(duplicate) = files
            .iter()
            .find(|file| !incoming_paths.insert(file.path.as_str()))
        {
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
            .filter(|path| !incoming_paths.contains(path.as_str()))
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
            self.set_file_ref(&file.path, &file.module, &file.source)
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
                run_must_use,
                finite_matches,
                foundations,
            )
        })
        .map_err(|_| AnalysisCancelled)?;
        Ok(SessionInspection {
            revision: self.revision,
            inspection: result.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn phase05_ownership_changes_refresh_dependents_and_reuse_unrelated_files() {
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = events.clone();
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let provider = "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass R:\n    def read(self): pass\n    def close(self): pass\ndef inspect(r: Borrowed[R]):\n    r.read()\n";
        let main = SourceFile::new("main.py", "main", "from codepact import move\nfrom provider import R, inspect\ndef run():\n    a = R()\n    inspect(a)\n    b = move(a)\n    b.close()\n");
        let unrelated = SourceFile::new(
            "independent.py",
            "independent",
            "def independent():\n    return 1\n",
        );
        let project = |source: String| {
            [
                main.clone(),
                SourceFile::new("provider.py", "provider", source),
                unrelated.clone(),
            ]
        };
        let flow_executions = || {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .count()
        };

        let original = project(provider.into());
        let mode = crate::Ownership::Enabled;
        assert!(session
            .check_with_ownership(&original, "3.11", mode)
            .diagnostics
            .is_empty());
        assert_eq!(flow_executions(), 3);
        events.lock().unwrap().clear();
        let changed = project(provider.replace("    r.read()", "    return r"));
        let reused = session.check_with_ownership(&changed, "3.11", mode);
        assert_eq!(flow_executions(), 2);
        assert_eq!(reused, crate::check_with_ownership(&changed, "3.11", mode));
        assert!(reused
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP2004"));
        events.lock().unwrap().clear();
        assert_eq!(session.check_with_ownership(&changed, "3.11", mode), reused);
        assert_eq!(flow_executions(), 0);
    }

    #[test]
    fn phase06_protocol_transition_edits_refresh_dependents_and_reuse_unrelated_files() {
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let provider = |target: &str| {
            format!(
                "from typing import Literal\nfrom codepact import state_machine, transition\nfrom codepact.rust import State\n@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='{target}')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\n"
            )
        };
        let main = SourceFile::new(
            "main.py",
            "main",
            "from provider import Channel\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        );
        let unrelated = SourceFile::new(
            "independent.py",
            "independent",
            "def independent():\n    return 1\n",
        );
        let project = |source: String| {
            [
                main.clone(),
                SourceFile::new("provider.py", "provider", source),
                unrelated.clone(),
            ]
        };
        let flow_executions = || {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>()
        };
        let original = project(provider("closed"));
        let mode = crate::Ownership::Advanced;
        let first = session.check_with_ownership(&original, "3.13", mode);
        let first_violation = first
            .diagnostics
            .iter()
            .find(|finding| finding.code == "TP2102")
            .expect("the close transition invalidates the receiver precondition");
        assert_eq!(first_violation.primary.file, "main.py");
        assert_eq!(flow_executions().len(), 3);

        events.lock().unwrap().clear();
        let changed = project(provider("ready"));
        let reused = session.check_with_ownership(&changed, "3.13", mode);
        assert!(!reused
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP2102"));
        assert_eq!(
            reused,
            crate::check_with_ownership(&changed, "3.13", mode),
            "updated protocol effects and source locations match a fresh advanced check"
        );
        assert_eq!(
            flow_executions().len(),
            2,
            "the changed provider and its dependent caller refresh, while the unrelated file reuses its query"
        );
    }

    #[test]
    fn phase06_task_coroutine_budget_and_query_event_evidence_cover_edits_and_cancellation() {
        use serde_json::json;
        use std::path::PathBuf;

        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let provider = |target: &str| {
            format!(
                "from typing import Literal\nfrom codepact import state_machine, transition\nfrom codepact.rust import State\n@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='{target}')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\n"
            )
        };
        let main = |tasks: usize| {
            let mut source = String::from(
                "from asyncio import TaskGroup\nfrom provider import Channel\nasync def worker(): pass\nasync def run():\n    async with TaskGroup() as group:\n",
            );
            for index in 0..tasks {
                source.push_str(&format!(
                    "        pending_{index} = worker()\n        group.create_task(pending_{index})\n"
                ));
            }
            source.push_str("    channel = Channel()\n    channel.close()\n    channel.send()\n");
            SourceFile::new("main.py", "main", source)
        };
        let unrelated = SourceFile::new(
            "independent.py",
            "independent",
            "def independent(): return 1\n",
        );
        let project = |task_count: usize, transition: &str| {
            [
                main(task_count),
                SourceFile::new("provider.py", "provider", provider(transition)),
                unrelated.clone(),
            ]
        };
        let event_keys = || events.lock().unwrap().clone();
        let query_event_names = || {
            event_keys()
                .into_iter()
                .map(|key| key.split('(').next().unwrap_or(&key).to_owned())
                .collect::<Vec<_>>()
        };
        let flow_query_keys = || {
            query_event_names()
                .into_iter()
                .filter(|key| key == "check_flow_file_query")
                .collect::<Vec<_>>()
        };
        let mut phases = Vec::new();
        let mode = crate::Ownership::AdvancedAssumeDefaultTaskFactory;

        let initial = project(128, "closed");
        let initial_result = session.check_with_ownership(&initial, "3.13", mode);
        assert_eq!(
            initial_result.coverage.status,
            "complete-for-supported-subset"
        );
        assert!(initial_result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP2102" && diagnostic.certainty == "definite"));
        let initial_events = flow_query_keys();
        assert_eq!(initial_events.len(), 3, "{initial_events:#?}");
        phases.push(json!({
            "phase": "initial-128-task-limit",
            "scheduled_task_count": 128,
            "retained_coroutine_count": 128,
            "coverage": initial_result.coverage.status,
            "tp2102_definite": initial_result.diagnostics.iter().any(|diagnostic| diagnostic.code == "TP2102" && diagnostic.certainty == "definite"),
            "flow_query_count": initial_events.len(),
            "flow_query_keys": initial_events,
            "query_events": query_event_names()
        }));

        events.lock().unwrap().clear();
        assert_eq!(
            session.check_with_ownership(&initial, "3.13", mode),
            initial_result
        );
        let warm_events = flow_query_keys();
        assert_eq!(
            warm_events.len(),
            0,
            "unchanged input must reuse all queries"
        );
        phases.push(json!({
            "phase": "unchanged-session",
            "flow_query_count": warm_events.len(),
            "flow_query_keys": warm_events,
            "query_events": query_event_names()
        }));

        events.lock().unwrap().clear();
        let over_budget = project(129, "closed");
        let over_budget_result = session.check_with_ownership(&over_budget, "3.13", mode);
        assert_eq!(over_budget_result.coverage.status, "incomplete");
        assert!(over_budget_result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("pending coroutine budget exceeded")
        }));
        assert!(!over_budget_result.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("structured task budget exceeded")
        }));
        assert!(over_budget_result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP2102" && diagnostic.certainty == "definite"));
        assert_eq!(
            over_budget_result,
            crate::check_with_ownership(&over_budget, "3.13", mode)
        );
        let task_edit_events = flow_query_keys();
        assert_eq!(task_edit_events.len(), 1, "{task_edit_events:#?}");
        phases.push(json!({
            "phase": "129th-task-coroutine-boundary",
            "scheduled_task_attempts": 129,
            "retained_coroutine_count": 128,
            "coverage": over_budget_result.coverage.status,
            "task_schedule_boundary": true,
            "separate_task_count_boundary": false,
            "coroutine_budget_boundary": true,
            "known_typestate_violation_preserved": true,
            "flow_query_count": task_edit_events.len(),
            "flow_query_keys": task_edit_events,
            "query_events": query_event_names()
        }));

        events.lock().unwrap().clear();
        let provider_edit = project(129, "ready");
        let provider_result = session.check_with_ownership(&provider_edit, "3.13", mode);
        assert!(!provider_result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP2102"));
        assert!(provider_result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("pending coroutine budget exceeded")
        }));
        assert_eq!(
            provider_result,
            crate::check_with_ownership(&provider_edit, "3.13", mode)
        );
        let provider_edit_events = flow_query_keys();
        assert_eq!(provider_edit_events.len(), 2, "{provider_edit_events:#?}");
        phases.push(json!({
            "phase": "provider-transition-edit",
            "dependent_typestate_violation_removed": true,
            "task_and_coroutine_budget_boundary_preserved": true,
            "refreshed_files": ["provider.py", "main.py"],
            "unrelated_file_reused": true,
            "flow_query_count": provider_edit_events.len(),
            "flow_query_keys": provider_edit_events,
            "query_events": query_event_names()
        }));

        events.lock().unwrap().clear();
        let recovered = project(128, "ready");
        let recovered_result = session.check_with_ownership(&recovered, "3.13", mode);
        assert_eq!(
            recovered_result.coverage.status,
            "complete-for-supported-subset"
        );
        assert!(
            recovered_result.diagnostics.is_empty(),
            "{recovered_result:#?}"
        );
        assert_eq!(
            recovered_result,
            crate::check_with_ownership(&recovered, "3.13", mode)
        );
        let recovery_events = flow_query_keys();
        assert_eq!(recovery_events.len(), 1, "{recovery_events:#?}");
        phases.push(json!({
            "phase": "task-budget-recovery",
            "scheduled_task_count": 128,
            "coverage": recovered_result.coverage.status,
            "matches_fresh": true,
            "flow_query_count": recovery_events.len(),
            "flow_query_keys": recovery_events,
            "query_events": query_event_names()
        }));

        let cancelled_input = project(128, "closed");
        let token = session.cancellation_token();
        token.cancel();
        events.lock().unwrap().clear();
        assert!(matches!(
            session.check_with_ownership_cancellable(&cancelled_input, "3.13", mode),
            Err(AnalysisCancelled)
        ));
        let cancellation_events = flow_query_keys();
        phases.push(json!({
            "phase": "cancelled-provider-edit",
            "cancelled_without_partial_result": true,
            "flow_query_count": cancellation_events.len(),
            "flow_query_keys": cancellation_events,
            "query_events": query_event_names()
        }));

        events.lock().unwrap().clear();
        let cancellation_recovery = session
            .check_with_ownership_cancellable(&cancelled_input, "3.13", mode)
            .expect("session recovers after cancelled provider edit");
        let fresh_recovery = crate::check_with_ownership(&cancelled_input, "3.13", mode);
        assert_eq!(cancellation_recovery.result, fresh_recovery);
        assert!(cancellation_recovery
            .result
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.code == "TP2102" && diagnostic.certainty == "definite"
            }));
        let cancellation_recovery_events = flow_query_keys();
        assert!(!cancellation_recovery_events.is_empty());
        phases.push(json!({
            "phase": "cancellation-recovery",
            "matches_fresh": true,
            "dependent_typestate_violation_restored": true,
            "flow_query_count": cancellation_recovery_events.len(),
            "flow_query_keys": cancellation_recovery_events,
            "query_events": query_event_names()
        }));

        if let Some(path) = std::env::var_os("CODEPACT_PHASE06_QUERY_EVIDENCE") {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let record = json!({
                "schema_version": 1,
                "target_python": "3.13",
                "ownership_mode": "advanced-assume-default-task-factory",
                "task_schedule_bound": "no separate task counter; scheduled coroutine records share the 128-record limit",
                "pending_coroutine_budget": 128,
                "source_execution": false,
                "phases": phases
            });
            std::fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
        }
    }

    #[test]
    fn phase08_manifest_edits_reuse_unrelated_flow_and_match_fresh_checks() {
        use crate::foundation::{export_manifest, FoundationInputs, ProviderPackage};
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let files = [
            SourceFile::new("caller.py", "caller", "from provider import op\nop()\n"),
            SourceFile::new("independent.py", "independent", "def op(): return 0\n"),
        ];
        let inputs = |version: &str, decorator: &str| {
            let mut package = ProviderPackage {
                distribution: "provider".into(),
                version: version.into(),
                import_scope: "provider".into(),
                sources: vec![SourceFile::new(
                    "provider/__init__.py",
                    "provider",
                    format!("from codepact import must_use\n{decorator}def op(): return 1\n"),
                )],
                marked: true,
                manifest: None,
            };
            package.manifest =
                Some(serde_json::to_string(&export_manifest(&package, "3.13").unwrap()).unwrap());
            FoundationInputs {
                providers: vec![package],
                ..FoundationInputs::default()
            }
        };
        let execute = |session: &mut AnalysisSession, inputs| {
            session.check_with_foundations(&files, "3.13", crate::CheckOptions::default(), inputs)
        };
        let executed = |query: &str| {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains(query))
                .count()
        };
        let first_inputs = inputs("1.0.0", "@must_use\n");
        let first = execute(&mut session, first_inputs.clone());
        assert!(first
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP3001"));
        assert_eq!(executed("select_provider_query"), 1);
        assert_eq!(executed("check_flow_file_query"), 2);

        events.lock().unwrap().clear();
        assert_eq!(execute(&mut session, first_inputs), first);
        assert_eq!(executed("select_provider_query"), 0);
        assert_eq!(executed("check_flow_file_query"), 0);

        let upgraded = inputs("2.0.0", "");
        let after = execute(&mut session, upgraded.clone());
        assert!(after
            .diagnostics
            .iter()
            .all(|finding| finding.code != "TP3001"));
        assert_eq!(after, execute(&mut AnalysisSession::default(), upgraded));
        assert_eq!(executed("select_provider_query"), 1);
        assert_eq!(executed("check_flow_file_query"), 1);
    }

    #[test]
    fn phase07_provider_summary_edits_refresh_consumers_and_reuse_unrelated_queries() {
        use serde_json::json;

        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let provider = |version: &str, marked: bool, outcome: &str| {
            let marker = if marked { "@must_use\n" } else { "" };
            crate::ProviderSource::new(
                "demo-dist",
                version,
                crate::SourceFile::new(
                    "demo/_impl.py",
                    "demo._impl",
                    format!(
                        "from codepact import Err, Ok, Option, Result, Some, Nothing, must_use\n{marker}def value() -> int: return 1\ndef outcome() -> {outcome}: return Ok(1)\n"
                    ),
                ),
            )
            .unwrap()
        };
        let main = SourceFile::new(
            "main.py",
            "main",
            "from codepact import Err, Ok\nfrom demo._impl import outcome, value\ndef inspect():\n    value()\n    current = outcome()\n    match current:\n        case Ok(_): pass\n        case Err(_): pass\n",
        );
        let independent_source = |value: usize| {
            format!(
                "from codepact import resource\n@resource(closed_by='close')\nclass Local:\n    def close(self): pass\ndef independent():\n    item = Local()\n    item.close()\n    return {value}\n"
            )
        };
        let independent = SourceFile::new("independent.py", "independent", independent_source(1));
        let project = |unrelated_source: &str| {
            vec![
                main.clone(),
                SourceFile::new("independent.py", "independent", unrelated_source),
            ]
        };
        let options = |source| crate::CheckOptions {
            experimental_finite_matches: true,
            provider_sources: vec![source],
            ..Default::default()
        };
        let flow_executions = || {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .count()
        };
        let mut phases = Vec::new();
        let files = project(&independent.source);

        let first_provider = provider("1.0.0", true, "Result[int, str]");
        let first = session.check_with_options(&files, "3.11", options(first_provider.clone()));
        assert!(first
            .diagnostics
            .iter()
            .any(|finding| { finding.code == "TP3001" && finding.provider_evidence.is_some() }));
        assert!(!first.diagnostics.iter().any(|finding| {
            finding.code == "TP5001" || finding.message.contains("outcome match pattern")
        }));
        assert_eq!(flow_executions(), 2);
        phases.push(json!({
            "phase": "initial-provider-and-outcome-summaries",
            "provider_version": "1.0.0",
            "coverage": first.coverage.status,
            "provider_must_use_finding": first.diagnostics.iter().any(|finding| {
                finding.code == "TP3001" && finding.provider_evidence.is_some()
            }),
            "finite_result_match_complete": !first.diagnostics.iter().any(|finding| finding.code == "TP5001"),
            "flow_query_count": 2,
            "independent_query_included": true
        }));

        events.lock().unwrap().clear();
        assert_eq!(
            session.check_with_options(&files, "3.11", options(first_provider.clone())),
            first,
            "unchanged provider, caller, and independent source reuse their summaries"
        );
        assert_eq!(flow_executions(), 0);
        phases.push(json!({"phase":"unchanged-session","flow_query_count":0}));

        events.lock().unwrap().clear();
        let unmarked_provider = provider("1.0.0", false, "Result[int, str]");
        let unmarked =
            session.check_with_options(&files, "3.11", options(unmarked_provider.clone()));
        assert!(!unmarked
            .diagnostics
            .iter()
            .any(|finding| { finding.code == "TP3001" && finding.provider_evidence.is_some() }));
        assert_eq!(
            unmarked,
            crate::check_with_options(&files, "3.11", options(unmarked_provider))
        );
        assert_eq!(
            flow_executions(),
            1,
            "provider summary refreshes its caller; independent resource query is reused"
        );
        phases.push(json!({
            "phase": "must-use-declaration-removed",
            "provider_must_use_finding_removed": !unmarked.diagnostics.iter().any(|finding| {
                finding.code == "TP3001" && finding.provider_evidence.is_some()
            }),
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        events.lock().unwrap().clear();
        let changed_signature = provider("1.0.0", true, "Option[int]");
        let signature_result =
            session.check_with_options(&files, "3.11", options(changed_signature.clone()));
        assert_eq!(
            signature_result,
            crate::check_with_options(&files, "3.11", options(changed_signature))
        );
        assert_eq!(
            flow_executions(),
            1,
            "signature changes refresh provider and consumer"
        );
        phases.push(json!({
            "phase": "provider-outcome-signature-edit",
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        events.lock().unwrap().clear();
        let new_version = provider("2.0.0", true, "Result[int, str]");
        let versioned = session.check_with_options(&files, "3.11", options(new_version.clone()));
        assert_eq!(
            versioned,
            crate::check_with_options(&files, "3.11", options(new_version))
        );
        assert_eq!(
            flow_executions(),
            1,
            "provider identity/version changes refresh the consumer"
        );
        phases.push(json!({
            "phase": "provider-version-2.0.0",
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        events.lock().unwrap().clear();
        let edited_independent = project(&independent_source(2));
        let unrelated = session.check_with_options(
            &edited_independent,
            "3.11",
            options(provider("2.0.0", true, "Result[int, str]")),
        );
        assert_eq!(
            unrelated,
            crate::check_with_options(
                &edited_independent,
                "3.11",
                options(provider("2.0.0", true, "Result[int, str]"))
            )
        );
        assert_eq!(
            flow_executions(),
            1,
            "unrelated edit does not recheck provider or caller"
        );
        phases.push(json!({
            "phase": "unrelated-resource-edit",
            "matches_fresh": true,
            "flow_query_count": 1,
            "provider_and_caller_reused": true
        }));

        let cancelled_files = project(&independent_source(3));
        session.cancellation_token().cancel();
        events.lock().unwrap().clear();
        assert!(matches!(
            session.check_with_options_cancellable(
                &cancelled_files,
                "3.11",
                options(provider("2.0.0", false, "Result[int, str]"))
            ),
            Err(AnalysisCancelled)
        ));
        phases.push(json!({
            "phase": "cancelled-provider-summary-edit",
            "cancelled_without_publishing_result": true,
            "query_events": events.lock().unwrap().len()
        }));
        events.lock().unwrap().clear();
        let recovered = session
            .check_with_options_cancellable(
                &cancelled_files,
                "3.11",
                options(provider("2.0.0", false, "Result[int, str]")),
            )
            .expect("the session recomputes fully after cancellation");
        assert_eq!(
            recovered.result,
            crate::check_with_options(
                &cancelled_files,
                "3.11",
                options(provider("2.0.0", false, "Result[int, str]"))
            )
        );
        phases.push(json!({
            "phase": "successful-retry-after-cancellation",
            "matches_fresh": true,
            "provider_must_use_finding_removed": !recovered.result.diagnostics.iter().any(|finding| {
                finding.code == "TP3001" && finding.provider_evidence.is_some()
            }),
            "flow_query_count": flow_executions()
        }));

        if let Some(path) = std::env::var_os("CODEPACT_PHASE07_QUERY_EVIDENCE") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&json!({
                    "schema_version": 1,
                    "analysis": "AnalysisSession::check_with_options",
                    "target_python": "3.11",
                    "experimental_finite_matches": true,
                    "source_execution": false,
                    "phases": phases
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn phase07_capture_adapter_signature_edits_refresh_match_consumers() {
        use serde_json::json;

        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let source = |signature: &str| {
            format!(
                "from codepact import Err, Ok, Option, Result, capture, must_use\n@must_use\ndef adapt() -> {signature}:\n    return capture(lambda: 1)\ndef inspect():\n    match adapt():\n        case Ok(_): pass\n        case Err(_): pass\n"
            )
        };
        let files = |signature: &str| {
            vec![
                SourceFile::new("main.py", "main", source(signature)),
                SourceFile::new("independent.py", "independent", "value = 1\n"),
            ]
        };
        let options = crate::CheckOptions {
            experimental_finite_matches: true,
            ..Default::default()
        };
        let flow_executions = || {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .count()
        };
        let mut phases = Vec::new();
        let result_type = files("Result[int, Exception]");
        let initial = session.check_with_options(&result_type, "3.11", options.clone());
        assert!(!initial
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP5001" || finding.code == "TP3001"));
        assert_eq!(flow_executions(), 2);
        phases.push(json!({
            "phase": "capture-adapter-returns-result",
            "coverage": initial.coverage.status,
            "outcome_match_complete": true,
            "flow_query_count": 2
        }));

        events.lock().unwrap().clear();
        assert_eq!(
            session.check_with_options(&result_type, "3.11", options.clone()),
            initial,
            "unchanged adapter and caller reuse their summaries"
        );
        assert_eq!(flow_executions(), 0);
        phases.push(json!({"phase":"unchanged-session","flow_query_count":0}));

        events.lock().unwrap().clear();
        let option_type = files("Option[int]");
        let changed = session.check_with_options(&option_type, "3.11", options.clone());
        assert!(changed.diagnostics.iter().any(|finding| {
            finding.code == "TP5001"
                && finding.message == "outcome match leaves Nothing, Some uncovered"
        }));
        assert_eq!(
            changed,
            crate::check_with_options(&option_type, "3.11", options.clone())
        );
        assert_eq!(flow_executions(), 1, "the independent query is reused");
        phases.push(json!({
            "phase": "capture-adapter-result-family-changes-to-option",
            "missing_option_variants_reported": true,
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        events.lock().unwrap().clear();
        let repaired = session.check_with_options(&result_type, "3.11", options.clone());
        assert!(!repaired
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP5001"));
        assert_eq!(
            repaired,
            crate::check_with_options(&result_type, "3.11", options)
        );
        assert_eq!(flow_executions(), 1);
        phases.push(json!({
            "phase": "capture-adapter-result-family-restored",
            "missing_variant_finding_removed": true,
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        if let Some(path) = std::env::var_os("CODEPACT_PHASE07_ADAPTER_EVIDENCE") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&json!({
                    "schema_version": 1,
                    "analysis": "capture adapter return summary invalidation",
                    "target_python": "3.11",
                    "experimental_finite_matches": true,
                    "source_execution": false,
                    "phases": phases
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn phase07_review_hint_evidence_edits_refresh_only_the_affected_file() {
        use serde_json::json;

        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&events);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                captured.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            ..AnalysisSession::default()
        };
        let source = |marked: bool, direct_return: bool| {
            format!(
                "from codepact import must_use\n{}def fetch() -> int: return 1\ndef wrapper():\n{}\n",
                if marked { "@must_use\n" } else { "" },
                if direct_return {
                    "    return fetch()"
                } else {
                    "    result = fetch()\n    return result"
                }
            )
        };
        let files = |marked, direct_return| {
            vec![
                SourceFile::new("main.py", "main", source(marked, direct_return)),
                SourceFile::new("independent.py", "independent", "value = 1\n"),
            ]
        };
        let flow_executions = || {
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .count()
        };
        let has_forwarding_hint = |result: &Inspection| {
            result
                .diagnostics
                .iter()
                .any(|finding| finding.code == "TP6002")
        };
        let mut phases = Vec::new();

        let unmarked_files = files(false, true);
        let unmarked = session.check_with_must_use(&unmarked_files, "3.11");
        assert!(!has_forwarding_hint(&unmarked));
        assert_eq!(flow_executions(), 2);
        phases.push(json!({
            "phase": "unmarked-callee-has-no-forwarding-hint",
            "hint_present": false,
            "flow_query_count": 2
        }));

        events.lock().unwrap().clear();
        assert_eq!(
            session.check_with_must_use(&unmarked_files, "3.11"),
            unmarked
        );
        assert_eq!(flow_executions(), 0);
        phases.push(json!({"phase":"unchanged-session","flow_query_count":0}));

        events.lock().unwrap().clear();
        let marked_files = files(true, true);
        let marked = session.check_with_must_use(&marked_files, "3.11");
        assert!(has_forwarding_hint(&marked));
        assert_eq!(marked, crate::check_with_must_use(&marked_files, "3.11"));
        assert_eq!(flow_executions(), 1);
        phases.push(json!({
            "phase": "must-use-marker-adds-forwarding-hint",
            "hint_present": true,
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        events.lock().unwrap().clear();
        let forwarded_files = files(true, false);
        let forwarded = session.check_with_must_use(&forwarded_files, "3.11");
        assert!(!has_forwarding_hint(&forwarded));
        assert_eq!(
            forwarded,
            crate::check_with_must_use(&forwarded_files, "3.11")
        );
        assert_eq!(flow_executions(), 1);
        phases.push(json!({
            "phase": "non-direct-return-removes-forwarding-hint",
            "hint_removed": true,
            "matches_fresh": true,
            "flow_query_count": 1,
            "unrelated_query_reused": true
        }));

        if let Some(path) = std::env::var_os("CODEPACT_PHASE07_HINT_EVIDENCE") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&json!({
                    "schema_version": 1,
                    "analysis": "must-use forwarding review-hint invalidation",
                    "target_python": "3.11",
                    "source_execution": false,
                    "phases": phases
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }

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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let file = SourceFile::new(
            "module.py",
            "module",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n",
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
            "from contextlib import closing\nfrom codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n",
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
    fn v03_23_target_model_and_root_edits_have_exact_query_refreshes() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let context = SourceFile::new(
            "phase03.py",
            "phase03",
            "from contextlib import closing\nfrom codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let flow_queries = |events: &Mutex<Vec<String>>| {
            events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>()
        };

        let supported = session.check(&[context.clone(), unrelated.clone()], "3.11");
        assert_eq!(supported.coverage.status, "complete-for-supported-subset");
        assert_eq!(flow_queries(&executions).len(), 2);

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        assert_eq!(
            session.check(&[context.clone(), unrelated.clone()], "3.11"),
            supported,
            "a no-op must reuse all cleanup queries"
        );
        assert!(flow_queries(&executions).is_empty());

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let unsupported = session.check(&[context.clone(), unrelated.clone()], "3.14");
        assert_eq!(
            unsupported,
            crate::check(&[context.clone(), unrelated.clone()], "3.14")
        );
        assert_eq!(unsupported.coverage.status, "incomplete");
        assert_eq!(
            flow_queries(&executions).len(),
            2,
            "target-version selection refreshes both supplied file queries"
        );

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let removed = session.check(std::slice::from_ref(&unrelated), "3.14");
        assert_eq!(
            removed,
            crate::check(std::slice::from_ref(&unrelated), "3.14")
        );
        assert!(
            removed.diagnostics.is_empty(),
            "removing the root drops stale findings"
        );
        assert_eq!(
            flow_queries(&executions).len(),
            1,
            "only the retained file query runs after root removal"
        );

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let restored = session.check(&[context.clone(), unrelated.clone()], "3.11");
        assert_eq!(restored, crate::check(&[context, unrelated], "3.11"));
        assert_eq!(restored.coverage.status, "complete-for-supported-subset");
        assert_eq!(
            flow_queries(&executions).len(),
            1,
            "restoring the supported context file refreshes its model-dependent query and reuses the unrelated file"
        );
    }

    #[test]
    fn cleanup_body_edits_refresh_only_the_changed_flow_query_and_match_fresh() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let source = |body: &str| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    {body}\n"
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let initial_file = source("pass");
        let initial = session.check(&[initial_file.clone(), unrelated.clone()], "3.11");
        assert!(initial.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }));

        for (body, expected_completion) in [
            ("resource.close()", None),
            ("raise RuntimeError()", Some("raise")),
        ] {
            executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            let edited_file = source(body);
            let updated = session.check(&[edited_file.clone(), unrelated.clone()], "3.11");
            assert_eq!(
                updated,
                crate::check(&[edited_file, unrelated.clone()], "3.11"),
                "body {body:?} must match a fresh analysis"
            );
            let leaks = updated
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_completion {
                Some(completion) => {
                    assert_eq!(leaks.len(), 1, "{body:?}: {updated:#?}");
                    assert_eq!(leaks[0].certainty, "definite", "{body:?}: {updated:#?}");
                    assert!(
                        leaks[0].message.ends_with(&format!("on {completion}")),
                        "{body:?}: {updated:#?}"
                    );
                }
                None => assert!(leaks.is_empty(), "{body:?}: {updated:#?}"),
            }
            let flow_queries = executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                flow_queries.len(),
                1,
                "only the edited cleanup file should refresh for {body:?}: {flow_queries:#?}"
            );
        }
    }

    #[test]
    fn v03_22_handler_expression_edit_refreshes_async_finally_cleanup_and_matches_fresh() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let source = |handler: &str| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\ndef handler_type():\n    raise LookupError()\nasync def cleanup():\n    resource = C()\n    try:\n        try:\n            raise RuntimeError()\n        except {handler}:\n            pass\n    finally:\n        await resource.aclose()\n"
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let initial_file = source("RuntimeError");
        let initial = session.check(&[initial_file.clone(), unrelated.clone()], "3.11");
        assert!(
            initial.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP1002"
                    && diagnostic.certainty == "possible"
                    && diagnostic.message.ends_with("on cancelled")
            }),
            "initial async cleanup retains its cancellation duty: {initial:#?}"
        );
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_file = source("handler_type()");
        let updated = session.check(&[edited_file.clone(), unrelated.clone()], "3.11");
        assert_eq!(
            updated,
            crate::check(&[edited_file, unrelated], "3.11"),
            "a handler-expression failure must still execute async finally cleanup and match a fresh analysis"
        );
        let duties = updated
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(duties.len(), 1, "{updated:#?}");
        assert_eq!(duties[0].certainty, "possible", "{updated:#?}");
        assert!(
            duties[0].message.ends_with("on cancelled"),
            "the remaining duty belongs to interrupted cleanup: {updated:#?}"
        );
        assert_eq!(duties[0].related[0].line, 8, "{updated:#?}");
        assert_eq!(updated.coverage.status, "incomplete", "{updated:#?}");
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "only the changed phase file should refresh its flow query: {flow_queries:#?}"
        );
    }

    #[test]
    fn reviewed_exceptional_close_outcomes_flow_through_session_results() {
        let source = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self):\n        raise RuntimeError()\ndef cleanup():\n    resource = Connection()\n    try:\n        resource.close()\n    except RuntimeError:\n        pass\n";
        let file = SourceFile::new("model.py", "model", source);
        let cleanup_exit_line = source
            .lines()
            .position(|line| line == "def cleanup():")
            .unwrap()
            + 1;
        let acquisition_line = source
            .lines()
            .position(|line| line.trim() == "resource = Connection()")
            .unwrap()
            + 1;

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
                    assert_eq!(
                        leaks[0].completion.as_deref(),
                        Some("fallthrough"),
                        "{id}: {checked:#?}"
                    );
                    assert_eq!(
                        leaks[0].primary.line, cleanup_exit_line,
                        "{id}: {checked:#?}"
                    );
                    assert_eq!(
                        leaks[0].related[0].line, acquisition_line,
                        "{id}: {checked:#?}"
                    );
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
    fn exceptional_close_outcome_matrix_keeps_duties_isolated() {
        let source = [
            "from codepact import resource",
            "@resource(closed_by='close')",
            "class Closed:",
            "    def close(self): raise RuntimeError()",
            "@resource(closed_by='close')",
            "class Open:",
            "    def close(self): raise RuntimeError()",
            "@resource(closed_by='close')",
            "class Preserved:",
            "    def close(self): raise RuntimeError()",
            "@resource(closed_by='close')",
            "class Unknown:",
            "    def close(self): raise RuntimeError()",
            "def cleanup():",
            "    closed_resource = Closed()",
            "    open_resource = Open()",
            "    preserved_resource = Preserved()",
            "    unknown_resource = Unknown()",
            "    try:",
            "        closed_resource.close()",
            "    except RuntimeError:",
            "        pass",
            "    try:",
            "        open_resource.close()",
            "    except RuntimeError:",
            "        pass",
            "    try:",
            "        preserved_resource.close()",
            "    except RuntimeError:",
            "        pass",
            "    try:",
            "        unknown_resource.close()",
            "    except RuntimeError:",
            "        pass",
        ]
        .join("\n");
        let file = SourceFile::new("matrix.py", "matrix", source.clone());
        let cleanup_exit_line = source
            .lines()
            .position(|line| line == "def cleanup():")
            .unwrap()
            + 1;
        let acquisition_lines = BTreeMap::from([
            (
                "Open",
                source
                    .lines()
                    .position(|line| line.trim() == "open_resource = Open()")
                    .unwrap()
                    + 1,
            ),
            (
                "Preserved",
                source
                    .lines()
                    .position(|line| line.trim() == "preserved_resource = Preserved()")
                    .unwrap()
                    + 1,
            ),
            (
                "Unknown",
                source
                    .lines()
                    .position(|line| line.trim() == "unknown_resource = Unknown()")
                    .unwrap()
                    + 1,
            ),
        ]);
        let cases = [
            (
                "fixture-exceptional-closed",
                "Closed",
                crate::model::LifecyclePostcondition::Closed,
            ),
            (
                "fixture-exceptional-open",
                "Open",
                crate::model::LifecyclePostcondition::Open,
            ),
            (
                "fixture-exceptional-preserve",
                "Preserved",
                crate::model::LifecyclePostcondition::Preserve,
            ),
            (
                "fixture-exceptional-unknown",
                "Unknown",
                crate::model::LifecyclePostcondition::Unknown,
            ),
        ];
        let mut catalog = crate::model::FlowModelCatalog::default();
        for (id, class_name, postcondition) in cases {
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: id.into(),
                    qualified_symbol: format!("matrix.{class_name}"),
                    close_method: "close".into(),
                    source_revision: "fixture-v1".into(),
                    source_url: format!("fixture://matrix.{class_name}"),
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
                    valid_cases: vec!["exceptional close outcome isolation".into()],
                    invalid_cases: vec!["open exceptional state retains its duty".into()],
                    unknown_cases: vec!["unknown state weakens only its own duty".into()],
                })
                .unwrap();
        }

        let mut session = AnalysisSession::default();
        assert!(session.set_flow_model_catalog(catalog));
        let checked = session.check_report(&[file], "3.11");
        let leaks = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(leaks.len(), 3, "{checked:#?}");
        let actual = leaks
            .iter()
            .map(|diagnostic| {
                assert_eq!(diagnostic.completion.as_deref(), Some("fallthrough"));
                assert_eq!(diagnostic.primary.line, cleanup_exit_line);
                let origin = diagnostic.related[0].line;
                (origin, diagnostic.certainty.as_str())
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            actual,
            BTreeMap::from([
                (acquisition_lines["Open"], "definite"),
                (acquisition_lines["Preserved"], "definite"),
                (acquisition_lines["Unknown"], "possible"),
            ]),
            "the known normal close postcondition must not leak onto any exception edge: {checked:#?}"
        );
        assert_eq!(checked.coverage.status, "incomplete", "{checked:#?}");
        for (id, class_name, postcondition) in cases {
            let selected = checked
                .available_models
                .iter()
                .find(|model| model.id == id)
                .unwrap_or_else(|| panic!("selected model {id} is missing: {checked:#?}"));
            assert!(selected
                .modeled_behavior
                .contains(&format!("exceptional=Some({postcondition:?})")));
            assert!(selected
                .qualified_symbol
                .contains(&format!("matrix.{class_name}::close")));
            assert!(selected.qualification.contains("reviewer=fixture-test"));
        }
    }

    #[test]
    fn reviewed_normal_close_outcomes_flow_through_session_results() {
        let source = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n";
        let file = SourceFile::new("model.py", "model", source);
        let cleanup_exit_line = source
            .lines()
            .position(|line| line == "def cleanup():")
            .unwrap()
            + 1;
        let acquisition_line = source
            .lines()
            .position(|line| line.trim() == "resource = Connection()")
            .unwrap()
            + 1;

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
                    assert_eq!(
                        leaks[0].completion.as_deref(),
                        Some("fallthrough"),
                        "{id}: {checked:#?}"
                    );
                    assert_eq!(
                        leaks[0].primary.line, cleanup_exit_line,
                        "{id}: {checked:#?}"
                    );
                    assert_eq!(
                        leaks[0].related[0].line, acquisition_line,
                        "{id}: {checked:#?}"
                    );
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
        let source = "from codepact import resource\n@resource(closed_by='aclose')\nclass Connection:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = Connection()\n    await resource.aclose()\n";
        let file = SourceFile::new("model.py", "model", source);
        let await_line = source
            .lines()
            .position(|line| line.trim() == "await resource.aclose()")
            .unwrap()
            + 1;
        let acquisition_line = source
            .lines()
            .position(|line| line.trim() == "resource = Connection()")
            .unwrap()
            + 1;

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
                    assert_eq!(leaks[0].primary.line, await_line, "{id}: {checked:#?}");
                    assert_eq!(
                        leaks[0].related[0].line, acquisition_line,
                        "{id}: {checked:#?}"
                    );
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
        let source = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n    resource.close()\n";
        let file = SourceFile::new("model.py", "model", source);
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
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

        let mixed_source = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup(flag):\n    resource = Connection()\n    if flag:\n        resource.close()\n    resource.close()\n";
        let mixed_file = SourceFile::new("model.py", "model", mixed_source);
        let mixed = session.check(&[mixed_file.clone(), unrelated.clone()], "3.11");
        let repeat_rejections = mixed
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "TP1001"
                    && diagnostic.message == "resource close requires an open resource"
            })
            .collect::<Vec<_>>();
        assert_eq!(repeat_rejections.len(), 1, "{mixed:#?}");
        assert_eq!(repeat_rejections[0].certainty, "possible", "{mixed:#?}");
        assert_eq!(repeat_rejections[0].primary.line, 9, "{mixed:#?}");
        assert!(!mixed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"));
        assert_eq!(mixed.coverage.status, "complete-for-supported-subset");

        let post_close_source = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n    resource.status()\n";
        let post_close_file = SourceFile::new("model.py", "model", post_close_source);
        let mut post_close_model = model.clone();
        post_close_model.id = "fixture-post-close-status".into();
        post_close_model.post_close_methods = BTreeSet::from(["status".into()]);
        let mut post_close_catalog = crate::model::FlowModelCatalog::default();
        post_close_catalog.insert(post_close_model).unwrap();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        assert!(session.set_flow_model_catalog(post_close_catalog.clone()));
        let post_close = session.check(&[post_close_file.clone(), unrelated.clone()], "3.11");
        assert!(!post_close
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert!(post_close.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001" && diagnostic.message.contains("unknown method effect")
        }));
        assert_eq!(post_close.coverage.status, "incomplete");
        let post_close_report =
            session.check_report(&[post_close_file.clone(), unrelated.clone()], "3.11");
        assert!(post_close_report.available_models.iter().any(|model| {
            model.id == "fixture-post-close-status"
                && model.modeled_behavior.contains("post-close-methods=status")
        }));
        let post_close_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            post_close_flow_queries.len(),
            1,
            "{post_close_flow_queries:#?}"
        );
        let mut fresh_post_close = AnalysisSession::default();
        fresh_post_close.set_flow_model_catalog(post_close_catalog);
        assert_eq!(
            post_close,
            fresh_post_close.check(&[post_close_file.clone(), unrelated.clone()], "3.11")
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
    fn resource_normal_outcome_edits_refresh_only_affected_flow_and_match_fresh() {
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
        let file = SourceFile::new(
            "model.py",
            "model",
            "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef cleanup():\n    resource = Connection()\n    resource.close()\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let baseline = session.check(&[file.clone(), unrelated.clone()], "3.11");
        assert!(
            !baseline
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "{baseline:#?}"
        );
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let make_catalog = |normal| {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: "fixture-normal-outcome".into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "close".into(),
                    source_revision: format!("normal-{normal:?}"),
                    source_url: "fixture://model.Connection.close".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: false,
                    normal: Some(normal),
                    exceptional: None,
                    cancelled: None,
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["normal postcondition applies".into()],
                    invalid_cases: vec!["open postcondition retains a duty".into()],
                    unknown_cases: vec!["unknown postcondition weakens certainty".into()],
                })
                .unwrap();
            catalog
        };

        for (postcondition, expected_certainty, expected_coverage) in [
            (
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let catalog = make_catalog(postcondition);
            executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            assert!(session.set_flow_model_catalog(catalog.clone()));
            let result = session.check(&[file.clone(), unrelated.clone()], "3.11");
            let leaks = result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{postcondition:?}: {result:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{postcondition:?}");
                }
                None => assert!(leaks.is_empty(), "{postcondition:?}: {result:#?}"),
            }
            assert_eq!(
                result.coverage.status, expected_coverage,
                "{postcondition:?}: {result:#?}"
            );
            let flow_queries = executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                flow_queries.len(),
                1,
                "{postcondition:?} edit should recheck only the modeled file: {flow_queries:#?}"
            );

            let mut fresh = AnalysisSession::default();
            fresh.set_flow_model_catalog(catalog);
            assert_eq!(
                result,
                fresh.check(&[file.clone(), unrelated.clone()], "3.11"),
                "{postcondition:?}"
            );
        }
    }

    #[test]
    fn resource_exceptional_outcome_edits_refresh_only_affected_flow_and_match_fresh() {
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
        let file = SourceFile::new(
            "model.py",
            "model",
            "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self):\n        raise RuntimeError()\ndef cleanup():\n    resource = Connection()\n    try:\n        resource.close()\n    except RuntimeError:\n        pass\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let _baseline = session.check(&[file.clone(), unrelated.clone()], "3.11");
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let make_catalog = |exceptional| {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: "fixture-exceptional-outcome".into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "close".into(),
                    source_revision: format!("exceptional-{exceptional:?}"),
                    source_url: "fixture://model.Connection.close".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: false,
                    normal: Some(crate::model::LifecyclePostcondition::Closed),
                    exceptional: Some(exceptional),
                    cancelled: None,
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["exceptional postcondition applies".into()],
                    invalid_cases: vec!["open exceptional state retains a duty".into()],
                    unknown_cases: vec!["unknown exceptional state retains a boundary".into()],
                })
                .unwrap();
            catalog
        };

        for (postcondition, expected_certainty, expected_coverage) in [
            (
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let catalog = make_catalog(postcondition);
            executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            assert!(session.set_flow_model_catalog(catalog.clone()));
            let result = session.check(&[file.clone(), unrelated.clone()], "3.11");
            let leaks = result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{postcondition:?}: {result:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{postcondition:?}");
                }
                None => assert!(leaks.is_empty(), "{postcondition:?}: {result:#?}"),
            }
            assert_eq!(
                result.coverage.status, expected_coverage,
                "{postcondition:?}: {result:#?}"
            );
            let flow_queries = executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                flow_queries.len(),
                1,
                "{postcondition:?} edit should recheck only the modeled file: {flow_queries:#?}"
            );

            let mut fresh = AnalysisSession::default();
            fresh.set_flow_model_catalog(catalog);
            assert_eq!(
                result,
                fresh.check(&[file.clone(), unrelated.clone()], "3.11"),
                "{postcondition:?}"
            );
        }
    }

    #[test]
    fn exceptional_model_edits_refresh_each_dependent_location_and_metadata() {
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
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self):\n        raise RuntimeError()\n",
        );
        let consumer_a = SourceFile::new(
            "consumer_a.py",
            "consumer_a",
            "from provider import Connection\ndef cleanup_a():\n    first = Connection()\n    try:\n        first.close()\n    except RuntimeError:\n        pass\n",
        );
        let consumer_b = SourceFile::new(
            "consumer_b.py",
            "consumer_b",
            "from provider import Connection\ndef cleanup_b():\n    second = Connection()\n    try:\n        second.close()\n    except RuntimeError:\n        pass\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let files = [
            provider.clone(),
            consumer_a.clone(),
            consumer_b.clone(),
            unrelated.clone(),
        ];
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let make_catalog = |exceptional, revision: &str| {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: "fixture-exceptional-edit".into(),
                    qualified_symbol: "provider.Connection".into(),
                    close_method: "close".into(),
                    source_revision: revision.into(),
                    source_url: "fixture://provider.Connection.close".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: false,
                    normal: Some(crate::model::LifecyclePostcondition::Closed),
                    exceptional: Some(exceptional),
                    cancelled: None,
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["exceptional close postcondition applies".into()],
                    invalid_cases: vec!["open exceptional state retains its duty".into()],
                    unknown_cases: vec!["unknown state weakens certainty".into()],
                })
                .unwrap();
            catalog
        };

        let baseline = session.check_report(&files, "3.11");
        let baseline_leaks = baseline
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(
            baseline_leaks.len(),
            2,
            "without an exceptional postcondition, each checked source close retains its open duty: {baseline:#?}"
        );
        assert!(baseline_leaks.iter().all(|diagnostic| {
            diagnostic.certainty == "definite"
                && diagnostic.completion.as_deref() == Some("fallthrough")
        }));

        let cases = [
            (
                crate::model::LifecyclePostcondition::Closed,
                "model-closed",
                Vec::<(&str, usize, usize)>::new(),
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Open,
                "model-open",
                vec![("consumer_a.py", 2, 3), ("consumer_b.py", 2, 3)],
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Preserve,
                "model-preserve",
                vec![("consumer_a.py", 2, 3), ("consumer_b.py", 2, 3)],
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Unknown,
                "model-unknown",
                vec![("consumer_a.py", 2, 3), ("consumer_b.py", 2, 3)],
                "incomplete",
            ),
        ];

        for (postcondition, revision, expected_locations, expected_coverage) in cases {
            let catalog = make_catalog(postcondition, revision);
            executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            assert!(session.set_flow_model_catalog(catalog.clone()));
            let result = session.check_report(&files, "3.11");
            let leaks = result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            let actual_locations = leaks
                .iter()
                .map(|diagnostic| {
                    assert!(
                        diagnostic.primary.file == "consumer_a.py"
                            || diagnostic.primary.file == "consumer_b.py",
                        "unexpected diagnostic file: {result:#?}"
                    );
                    assert_eq!(diagnostic.related.len(), 1, "{result:#?}");
                    let certainty =
                        if postcondition == crate::model::LifecyclePostcondition::Unknown {
                            "possible"
                        } else {
                            "definite"
                        };
                    assert_eq!(diagnostic.certainty, certainty, "{result:#?}");
                    assert_eq!(diagnostic.completion.as_deref(), Some("fallthrough"));
                    (
                        diagnostic.primary.file.as_str(),
                        diagnostic.primary.line,
                        diagnostic.related[0].line,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_locations, expected_locations, "{result:#?}");
            assert_eq!(result.coverage.status, expected_coverage, "{result:#?}");

            let selected = result
                .available_models
                .iter()
                .find(|model| model.id == "fixture-exceptional-edit")
                .expect("the selected exceptional model is retained in result metadata");
            assert_eq!(
                selected.source_revisions,
                [format!("{revision} (Python 3.11)")]
            );
            assert!(selected
                .modeled_behavior
                .contains(&format!("exceptional=Some({postcondition:?})")));
            assert!(selected.qualification.contains("reviewer=fixture-test"));

            let flow_queries = executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                flow_queries.len(),
                3,
                "the provider and both dependent consumers must refresh; unrelated.py must reuse its query: {flow_queries:#?}"
            );

            let mut fresh = AnalysisSession::default();
            fresh.set_flow_model_catalog(catalog);
            assert_eq!(
                result,
                fresh.check_report(&files, "3.11"),
                "{postcondition:?} model result, identities, metadata and spans must match a fresh session"
            );
        }
    }

    #[test]
    fn async_cancelled_outcome_edits_refresh_only_affected_flow_and_match_fresh() {
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
        let file = SourceFile::new(
            "model.py",
            "model",
            "from codepact import resource\n@resource(closed_by='aclose')\nclass Connection:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = Connection()\n    await resource.aclose()\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let baseline = session.check(&[file.clone(), unrelated.clone()], "3.11");
        assert!(baseline.coverage.status == "incomplete", "{baseline:#?}");
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let make_catalog = |cancelled| {
            let mut catalog = crate::model::FlowModelCatalog::default();
            catalog
                .insert(crate::model::ResourceEffectModel {
                    id: "fixture-async-cancelled-outcome".into(),
                    qualified_symbol: "model.Connection".into(),
                    close_method: "aclose".into(),
                    source_revision: format!("cancelled-{cancelled:?}"),
                    source_url: "fixture://model.Connection.aclose".into(),
                    supported_python_versions: BTreeSet::from(["3.11".into()]),
                    platform_assumptions: vec!["platform-independent fixture".into()],
                    argument_aliases: Vec::new(),
                    result_alias: None,
                    duty_handoff: "none".into(),
                    suppression: "not-a-context-manager".into(),
                    asynchronous: true,
                    normal: Some(crate::model::LifecyclePostcondition::Closed),
                    exceptional: None,
                    cancelled: Some(cancelled),
                    repeat_close: crate::model::RepeatClosePolicy::Unspecified,
                    post_close_methods: BTreeSet::new(),
                    trust: "reviewed-fixture".into(),
                    reviewer: "fixture-test".into(),
                    valid_cases: vec!["cancelled postcondition applies".into()],
                    invalid_cases: vec!["open cancellation retains a duty".into()],
                    unknown_cases: vec!["unknown cancellation retains a boundary".into()],
                })
                .unwrap();
            catalog
        };

        for (postcondition, expected_certainty, expected_coverage) in [
            (
                crate::model::LifecyclePostcondition::Closed,
                None,
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Open,
                Some("definite"),
                "complete-for-supported-subset",
            ),
            (
                crate::model::LifecyclePostcondition::Unknown,
                Some("possible"),
                "incomplete",
            ),
        ] {
            let catalog = make_catalog(postcondition);
            executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            assert!(session.set_flow_model_catalog(catalog.clone()));
            let result = session.check_report(&[file.clone(), unrelated.clone()], "3.11");
            let leaks = result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1002")
                .collect::<Vec<_>>();
            match expected_certainty {
                Some(certainty) => {
                    assert_eq!(leaks.len(), 1, "{postcondition:?}: {result:#?}");
                    assert_eq!(leaks[0].certainty, certainty, "{postcondition:?}");
                    assert_eq!(leaks[0].completion.as_deref(), Some("cancelled"));
                }
                None => assert!(leaks.is_empty(), "{postcondition:?}: {result:#?}"),
            }
            assert_eq!(
                result.coverage.status, expected_coverage,
                "{postcondition:?}: {result:#?}"
            );
            let flow_queries = executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|key| key.contains("check_flow_file_query"))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                flow_queries.len(),
                1,
                "{postcondition:?} edit should recheck only the modeled file: {flow_queries:#?}"
            );

            let mut fresh = AnalysisSession::default();
            fresh.set_flow_model_catalog(catalog);
            assert_eq!(
                result,
                fresh.check_report(&[file.clone(), unrelated.clone()], "3.11"),
                "{postcondition:?}"
            );
        }
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let consumer = SourceFile::new(
            "consumer.py",
            "consumer",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nvalue = C()\nvalue.close()\nvalue.status()\n",
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\n",
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
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    pass\n",
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
                "from codepact import resource\n@resource(closed_by='{lifecycle}')\nclass C:\n    def close(self): pass\n    def finish(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n{effect}"
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
            "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\n",
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
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\n\ndef shutdown(value):\n    value.close()\n",
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
    fn moving_a_conditional_cleanup_refreshes_its_related_span() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let source = |blank: bool| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup(flag):\n    resource = C()\n    if flag:\n{}        resource.close()\n",
                    if blank { "\n" } else { "" }
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first_source = source(false);
        let first = session.check(&[first_source.clone(), unrelated.clone()], "3.11");
        let first_leak = first
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "TP1002")
            .expect("a conditional cleanup leaves a possible duty");
        assert_eq!(first_leak.certainty, "possible", "{first:#?}");
        let first_close = first_leak
            .related
            .iter()
            .find(|span| span.line == 8)
            .expect("the leak retains the conditional close origin")
            .clone();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_source = source(true);
        let updated = session.check(&[edited_source.clone(), unrelated.clone()], "3.11");
        let moved_leak = updated
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "TP1002")
            .expect("the edited conditional cleanup still leaves a possible duty");
        let moved_close = moved_leak
            .related
            .iter()
            .find(|span| span.line == first_close.line + 1)
            .expect("the related cleanup span moves with the edited body");
        assert_eq!(moved_leak.certainty, first_leak.certainty);
        assert_eq!(moved_close.column, first_close.column);
        assert_eq!(updated, crate::check(&[edited_source, unrelated], "3.11"));
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "only the edited cleanup file should refresh: {flow_queries:#?}"
        );
    }

    #[test]
    fn moving_acquisition_and_owned_annotation_spans_matches_fresh_cleanup_results() {
        let resource_header = "from codepact import resource\nfrom codepact.rust import Owned\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n";
        let source = format!("{resource_header}def cleanup(owner: Owned[C]):\n    local = C()\n");
        let file = SourceFile::new("phase03.py", "phase03", source.clone());
        let mut session = AnalysisSession::default();
        let initial = session.check(std::slice::from_ref(&file), "3.11");
        let initial_leaks = initial
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(initial_leaks.len(), 2, "{initial:#?}");
        let initial_related_lines = initial_leaks
            .iter()
            .map(|diagnostic| diagnostic.related[0].line)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            initial_related_lines,
            BTreeSet::from([6, 7]),
            "{initial:#?}"
        );

        let moved_source = format!(
            "{resource_header}# moved cleanup declarations\n{}",
            &source[resource_header.len()..]
        );
        let moved_file = SourceFile::new("phase03.py", "phase03", moved_source);
        let updated = session.check(std::slice::from_ref(&moved_file), "3.11");
        let updated_leaks = updated
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(updated_leaks.len(), 2, "{updated:#?}");
        let updated_related_lines = updated_leaks
            .iter()
            .map(|diagnostic| diagnostic.related[0].line)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            updated_related_lines,
            BTreeSet::from([7, 8]),
            "{updated:#?}"
        );
        assert_eq!(
            updated,
            crate::check(std::slice::from_ref(&moved_file), "3.11"),
            "the reused-session origin spans and result must match a fresh check"
        );
    }

    #[test]
    fn lifecycle_callee_edits_match_a_fresh_check() {
        let mut session = AnalysisSession::default();
        let before = SourceFile::new(
            "module.py",
            "module",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    value.close()\nconnection = C()\nshutdown(connection)\nconnection.status()\n",
        );
        let first = session.check(std::slice::from_ref(&before), "3.11");
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));

        let after = SourceFile::new(
            "module.py",
            "module",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef shutdown(value):\n    pass\nconnection = C()\nshutdown(connection)\nconnection.status()\n",
        );
        let updated = session.check(std::slice::from_ref(&after), "3.11");

        assert!(!updated
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1001"));
        assert_eq!(updated, crate::check(&[after], "3.11"));
    }

    #[test]
    fn owned_return_contract_edits_refresh_callers_and_match_fresh() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let phase = |owned: bool| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\nfrom codepact.rust import Owned\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef make(){}:\n    return C()\ndef leak():\n    value = make()\n",
                    if owned { " -> Owned[C]" } else { "" },
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first_phase = phase(true);
        let first = session.check(&[first_phase.clone(), unrelated.clone()], "3.11");
        assert!(
            first
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"
                    && diagnostic.primary.file == "phase03.py"),
            "{first:#?}"
        );
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_phase = phase(false);
        let updated = session.check(&[edited_phase.clone(), unrelated.clone()], "3.11");
        assert!(
            updated.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP9001"
                    && diagnostic.primary.file == "phase03.py"
                    && diagnostic.message.contains("return")
            }),
            "{updated:#?}"
        );
        assert_eq!(updated.coverage.status, "incomplete", "{updated:#?}");
        assert_eq!(updated, crate::check(&[edited_phase, unrelated], "3.11"));
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "the edited flow file should refresh while the unrelated file reuses its query: {flow_queries:#?}"
        );
    }

    #[test]
    fn async_callee_suspension_edits_refresh_callers_and_match_fresh() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let phase = |body: &str| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nasync def use(resource: Borrowed[C]):\n    {body}\nasync def cleanup():\n    resource = C()\n    try:\n        await use(resource)\n    finally:\n        resource.close()\n"
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first_phase = phase("pass");
        let first = session.check(&[first_phase.clone(), unrelated.clone()], "3.11");
        assert!(
            !first
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "a successful synchronous close in finally resolves the captured duty: {first:#?}"
        );
        assert!(first.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("cancellation")
                && diagnostic.primary.file == "phase03.py"
        }));
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_phase = phase("await resource.pause()");
        let updated = session.check(&[edited_phase.clone(), unrelated.clone()], "3.11");
        assert!(
            !updated
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "a successful synchronous close after suspension resolves the captured duty: {updated:#?}"
        );
        assert!(updated.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("cancellation")
                && diagnostic.primary.file == "phase03.py"
        }));
        assert_eq!(updated.coverage.status, "incomplete", "{updated:#?}");
        assert_ne!(
            updated, first,
            "the callee's new suspension must affect the result"
        );
        assert_eq!(updated, crate::check(&[edited_phase, unrelated], "3.11"));
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "the edited flow file should refresh while the unrelated file reuses its query: {flow_queries:#?}"
        );
    }

    #[test]
    fn async_coroutine_capture_edits_preserve_unrelated_duties_and_match_fresh() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let phase = |captured: &str| {
            SourceFile::new(
                "phase03.py",
                "phase03",
                format!(
                    "from codepact import resource\nfrom codepact.rust import Owned\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nasync def close_owned(resource: Owned[C]):\n    resource.close()\nasync def cleanup():\n    first = C()\n    second = C()\n    await close_owned({captured})\n"
                ),
            )
        };
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let first_phase = phase("first");
        let first = session.check(&[first_phase.clone(), unrelated.clone()], "3.11");
        let first_leaks = first
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(first_leaks.len(), 3, "{first:#?}");
        assert!(first_leaks.iter().any(|diagnostic| {
            diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
                && diagnostic.related.iter().any(|span| span.line == 9)
        }), "the captured first resource remains caller-owned if cancellation precedes entry: {first:#?}");
        assert!(
            first_leaks.iter().any(|diagnostic| {
                diagnostic.certainty == "definite"
                    && diagnostic.related.iter().any(|span| span.line == 10)
            }),
            "the unrelated second resource remains a definite duty: {first:#?}"
        );
        assert!(
            first_leaks.iter().any(|diagnostic| {
                diagnostic.certainty == "definite"
                    && diagnostic.message.ends_with("on cancelled")
                    && diagnostic.related.iter().any(|span| span.line == 10)
            }),
            "the unrelated second resource stays definite on cancellation: {first:#?}"
        );
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let edited_phase = phase("second");
        let updated = session.check(&[edited_phase.clone(), unrelated.clone()], "3.11");
        let updated_leaks = updated
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(updated_leaks.len(), 3, "{updated:#?}");
        assert!(
            updated_leaks.iter().any(|diagnostic| {
                diagnostic.certainty == "definite"
                    && diagnostic.related.iter().any(|span| span.line == 9)
            }),
            "the uncaptured first resource remains definitely open: {updated:#?}"
        );
        assert!(
            updated_leaks.iter().any(|diagnostic| {
                diagnostic.certainty == "possible"
                    && diagnostic.message.ends_with("on cancelled")
                    && diagnostic.related.iter().any(|span| span.line == 10)
            }),
            "only the captured second resource is uncertain before coroutine entry: {updated:#?}"
        );
        assert_eq!(updated, crate::check(&[edited_phase, unrelated], "3.11"));
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "the edited phase file should refresh while the unrelated file reuses its query: {flow_queries:#?}"
        );
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
    fn session_source_byte_count_tracks_mutations_resets_and_storage_rebuilds() {
        let mut session = AnalysisSession::default();
        session
            .set_file("first.py".into(), "first".into(), "x = 1\n".into())
            .unwrap();
        assert_eq!(session.total_source_bytes, 6);

        session
            .set_file("first.py".into(), "first".into(), "x = 123\n".into())
            .unwrap();
        assert_eq!(session.total_source_bytes, 8);

        session
            .set_file("second.py".into(), "second".into(), "y\n".into())
            .unwrap();
        assert_eq!(session.total_source_bytes, 10);

        session.mutations_since_reset = crate::DEFAULT_MAX_SESSION_MUTATIONS;
        session
            .set_file("third.py".into(), "third".into(), "z\n".into())
            .unwrap();
        assert_eq!(session.total_source_bytes, 12);
        assert_eq!(session.mutations_since_reset, 1);

        session.remove_file("first.py");
        assert_eq!(session.total_source_bytes, 4);

        session.reset();
        assert_eq!(session.total_source_bytes, 0);
        assert!(session.files.is_empty());
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
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
            "from codepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
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
            "from codepact import resource\n@resource(closed_by=\"dispose\")\nclass C: pass\n",
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            "from codepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
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
            "# shifted\nfrom codepact import resource\n@resource(closed_by=\"close\")\nclass C: pass\n",
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let token = session.cancellation_token();
        token.cancel();
        let leaking = SourceFile::new(
            "module.py",
            "module",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n",
        );
        assert!(matches!(
            session.check_report_cancellable(std::slice::from_ref(&leaking), "3.11"),
            Err(AnalysisCancelled)
        ));
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let cleaned = SourceFile::new(
            "module.py",
            "module",
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    resource.close()\n",
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
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            flow_queries.len(),
            1,
            "recovery must recompute the cancelled file query exactly once: {flow_queries:#?}"
        );
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
    fn cleanup_root_removal_and_readdition_match_fresh_results() {
        let resource = SourceFile::new(
            "resource.py",
            "resource",
            "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\ndef leak():\n    connection = Connection()\n",
        );
        let unrelated = SourceFile::new("unrelated.py", "unrelated", "value = 1\n");
        let mut session = AnalysisSession::default();

        let initial = session
            .check_cancellable(&[resource.clone(), unrelated.clone()], "3.11")
            .unwrap();
        assert!(initial
            .inspection
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"));

        let removed = session
            .check_cancellable(std::slice::from_ref(&unrelated), "3.11")
            .unwrap();
        assert!(removed.revision > initial.revision);
        assert!(!removed
            .inspection
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"));
        assert_eq!(
            removed.inspection,
            crate::check(std::slice::from_ref(&unrelated), "3.11"),
            "dropping a cleanup root must not retain its prior findings"
        );

        let restored = session
            .check_cancellable(&[resource.clone(), unrelated.clone()], "3.11")
            .unwrap();
        assert!(restored.revision > removed.revision);
        assert_eq!(restored.inspection, initial.inspection);
        assert_eq!(
            restored.inspection,
            crate::check(&[resource.clone(), unrelated.clone()], "3.11"),
            "readding a cleanup root must match a fresh analysis"
        );

        let unchanged = session
            .check_cancellable(&[resource, unrelated], "3.11")
            .unwrap();
        assert_eq!(unchanged.revision, restored.revision);
        assert_eq!(unchanged.inspection, restored.inspection);
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
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
            "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\nc = C()\nc.close()\nc.status()\n",
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

    #[test]
    fn adoption_policy_and_baseline_edits_reuse_canonical_session_queries() {
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
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let root = std::env::temp_dir().join(format!(
            "codepact-phase04-adoption-session-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let baseline_path = root.join("baseline.json");
        let source = |suppressed: bool| {
            SourceFile::new(
                "main.py",
                "main",
                if suppressed {
                    "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run():\n    fetch()  # codepact: ignore[TP3001] -- reviewed intentional discard\n"
                } else {
                    "from codepact import must_use\n@must_use\ndef fetch(): return 1\ndef run():\n    fetch()\n"
                },
            )
        };
        let files = [source(false)];
        let first = session.check_with_must_use(&files, "3.11");
        let first = crate::CheckResult::from_inspection(first, "3.11");
        assert!(first
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP3001"));
        let mut config = crate::adoption::resolve_config(
            &root,
            &crate::adoption::Overrides {
                no_config: true,
                ..crate::adoption::Overrides::default()
            },
        )
        .unwrap();
        let error_threshold =
            crate::adoption::build_report(first.clone(), &files, &config, None, None).unwrap();
        assert_eq!(error_threshold.exit_code, 0);

        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let reused = session.check_with_must_use(&files, "3.11");
        let reused = crate::CheckResult::from_inspection(reused, "3.11");
        let flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .count();
        assert_eq!(
            flow_queries, 0,
            "policy changes must not execute semantic flow queries"
        );
        let fresh =
            crate::CheckResult::from_inspection(crate::check_with_must_use(&files, "3.11"), "3.11");
        assert_eq!(reused, fresh);
        config.fail_on = "warning".into();
        let warning_threshold =
            crate::adoption::build_report(reused.clone(), &files, &config, None, None).unwrap();
        assert_eq!(warning_threshold.exit_code, 1);
        assert_eq!(
            error_threshold.semantic_fingerprint,
            warning_threshold.semantic_fingerprint
        );
        assert_ne!(
            error_threshold.policy_fingerprint,
            warning_threshold.policy_fingerprint
        );
        assert_eq!(
            error_threshold.diagnostics[0].fingerprint,
            warning_threshold.diagnostics[0].fingerprint
        );

        crate::adoption::write_baseline(
            &baseline_path,
            &reused,
            &files,
            &config,
            false,
            "capture existing finding",
        )
        .unwrap();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let suppressed_files = [source(true)];
        let suppressed_result = session.check_with_must_use(&suppressed_files, "3.11");
        let suppressed_result = crate::CheckResult::from_inspection(suppressed_result, "3.11");
        let suppressed_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .count();
        assert_eq!(
            suppressed_flow_queries, 0,
            "a suppression comment must reuse unchanged semantic flow facts"
        );
        let inline = crate::adoption::build_report(
            suppressed_result.clone(),
            &suppressed_files,
            &config,
            Some(&baseline_path),
            None,
        )
        .unwrap();
        assert!(inline
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.diagnostic.code == "TP3001"
                && diagnostic.disposition == "suppressed-inline"));
        let fresh = crate::CheckResult::from_inspection(
            crate::check_with_must_use(&suppressed_files, "3.11"),
            "3.11",
        );
        assert_eq!(suppressed_result, fresh);

        let mut baseline: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&baseline_path).unwrap()).unwrap();
        baseline["update_reason"] = serde_json::json!("baseline policy edit");
        std::fs::write(&baseline_path, serde_json::to_vec(&baseline).unwrap()).unwrap();
        executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let unchanged = session.check_with_must_use(&suppressed_files, "3.11");
        let unchanged = crate::CheckResult::from_inspection(unchanged, "3.11");
        let baseline_flow_queries = executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|key| key.contains("check_flow_file_query"))
            .count();
        assert_eq!(
            baseline_flow_queries, 0,
            "baseline bytes are report-policy inputs only"
        );
        let updated_baseline = crate::adoption::build_report(
            unchanged,
            &suppressed_files,
            &config,
            Some(&baseline_path),
            None,
        )
        .unwrap();
        assert_eq!(
            updated_baseline.baseline.update_reason.as_deref(),
            Some("baseline policy edit")
        );
        assert_eq!(
            inline.semantic_fingerprint,
            updated_baseline.semantic_fingerprint
        );
        assert_ne!(
            inline.policy_fingerprint,
            updated_baseline.policy_fingerprint
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn v04_20_21_policy_measurements_and_semantic_mutations_match_fresh_results() {
        use std::time::Instant;
        let executions = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = Arc::clone(&executions);
        let storage = salsa::Storage::new(Some(Box::new(move |event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                observed.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        let mut session = AnalysisSession {
            db: Database { storage },
            files: BTreeMap::new(),
            total_source_bytes: 0,
            revision: 0,
            target_version: None,
            ownership: crate::Ownership::Disabled,
            finite_matches: false,
            model_catalog: crate::model::FlowModelCatalog::default(),
            mutations_since_reset: 0,
            cancellation: Arc::new(Mutex::new(salsa::CancellationToken::default())),
        };
        let root = std::env::temp_dir().join(format!(
            "codepact-session-qualification-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let provider="from codepact import resource, must_use\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 1\n@must_use\ndef fetch() -> int: return 1\ndef shutdown(conn):\n    conn.close()\n";
        let client="from provider import Connection, fetch, shutdown\ndef run():\n    fetch()\n    conn = Connection()\n    shutdown(conn)\n    conn.status()\n";
        let original = vec![
            SourceFile::new("provider.py", "provider", provider),
            SourceFile::new("main.py", "main", client),
            SourceFile::new("unrelated.py", "unrelated", "def value(): return 1\n"),
        ];
        let mut config = crate::adoption::resolve_config(
            &root,
            &crate::adoption::Overrides {
                no_config: true,
                ..Default::default()
            },
        )
        .unwrap();
        let initial = session.check_with_must_use(&original, "3.11");
        assert!(initial.diagnostics.iter().any(|d| d.code == "TP1001"));
        let baseline_path = root.join("baseline.json");
        crate::adoption::write_baseline(
            &baseline_path,
            &crate::CheckResult::from_inspection(initial.clone(), "3.11"),
            &original,
            &config,
            false,
            "session benchmark",
        )
        .unwrap();
        let mut measurements = Vec::new();
        for name in [
            "noop",
            "strict",
            "threshold",
            "severity",
            "selection",
            "suppression",
            "baseline",
        ] {
            config.mode = if name == "strict" {
                "strict"
            } else {
                "advisory"
            }
            .into();
            config.fail_on = if name == "threshold" {
                "warning"
            } else {
                "error"
            }
            .into();
            config.severity.clear();
            if name == "severity" {
                config.severity.insert("TP3001".into(), "error".into());
            }
            config.enabled_rules = crate::adoption::DEFAULT_RULES
                .iter()
                .map(|s| s.to_string())
                .collect();
            if name == "selection" {
                config.enabled_rules.remove("TP3001");
            }
            let scenario_files = if name == "suppression" {
                let mut files = original.clone();
                files[1].source = client.replace(
                    "    fetch()",
                    "    fetch() # codepact: ignore[TP3001] -- benchmark acknowledgement",
                );
                files
            } else {
                original.clone()
            };
            let baseline = if name == "baseline" {
                Some(baseline_path.as_path())
            } else {
                None
            };
            let mut semantic_ms = Vec::new();
            let mut reporting_ms = Vec::new();
            for _ in 0..7 {
                executions.lock().unwrap().clear();
                let start = Instant::now();
                let result = session.check_with_must_use(&scenario_files, "3.11");
                semantic_ms.push(start.elapsed().as_secs_f64() * 1000.);
                let events = executions
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|k| k.contains("check_flow_file_query"))
                    .count();
                assert_eq!(events, 0, "{name}");
                let fresh = crate::check_with_must_use(&scenario_files, "3.11");
                assert_eq!(result, fresh);
                let start = Instant::now();
                let actual = crate::adoption::build_report(
                    crate::CheckResult::from_inspection(result, "3.11"),
                    &scenario_files,
                    &config,
                    baseline,
                    None,
                )
                .unwrap();
                reporting_ms.push(start.elapsed().as_secs_f64() * 1000.);
                let expected = crate::adoption::build_report(
                    crate::CheckResult::from_inspection(fresh, "3.11"),
                    &scenario_files,
                    &config,
                    baseline,
                    None,
                )
                .unwrap();
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    serde_json::to_value(expected).unwrap()
                );
            }
            measurements.push(serde_json::json!({"scenario":name,"semantic_ms":semantic_ms,"reporting_ms":reporting_ms,"flow_query_executions":0}));
        }
        println!(
            "SESSION_QUALIFICATION {}",
            serde_json::to_string(&measurements).unwrap()
        );
        let cases = [
            (
                "mustuse-decorator",
                provider.replace("@must_use\n", ""),
                client.to_string(),
                "3.11",
                true,
            ),
            (
                "cleanup-effect",
                provider.replace("    conn.close()", "    pass"),
                client.to_string(),
                "3.11",
                true,
            ),
            (
                "locations",
                format!("\n# moved\n{provider}"),
                format!("\n{client}"),
                "3.11",
                true,
            ),
            (
                "target-312",
                provider.to_string(),
                client.to_string(),
                "3.12",
                false,
            ),
            (
                "target-313",
                provider.to_string(),
                client.to_string(),
                "3.13",
                false,
            ),
            (
                "failed-parse",
                "def invalid(:\n".to_string(),
                client.to_string(),
                "3.11",
                true,
            ),
            (
                "recovery",
                provider.to_string(),
                client.to_string(),
                "3.11",
                true,
            ),
        ];
        for (name, body, consumer, target, expect_unrelated_reuse) in cases {
            let files = vec![
                SourceFile::new("provider.py", "provider", body),
                SourceFile::new("main.py", "main", consumer),
                original[2].clone(),
            ];
            executions.lock().unwrap().clear();
            let reused = session.check_with_must_use(&files, target);
            let events = executions
                .lock()
                .unwrap()
                .iter()
                .filter(|k| k.contains("check_flow_file_query"))
                .count();
            let fresh_session = AnalysisSession::default().check_with_must_use(&files, target);
            let one_shot = crate::check_with_must_use(&files, target);
            assert_eq!(reused, fresh_session, "{name}");
            assert_eq!(reused, one_shot, "{name}");
            config.python_version = target.into();
            let report = crate::adoption::build_report(
                crate::CheckResult::from_inspection(reused.clone(), target),
                &files,
                &config,
                Some(&baseline_path),
                None,
            )
            .unwrap();
            let fresh_report = crate::adoption::build_report(
                crate::CheckResult::from_inspection(fresh_session.clone(), target),
                &files,
                &config,
                Some(&baseline_path),
                None,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(report).unwrap(),
                serde_json::to_value(fresh_report).unwrap(),
                "{name}: report/baseline parity"
            );

            // A target change intentionally refreshes all files. Other edits
            // must leave the unrelated module's flow query reusable.
            if expect_unrelated_reuse && name != "failed-parse" && name != "recovery" {
                assert!(events < files.len(), "{name}: {events} flow executions");
            }
            println!(
                "SESSION_MUTATION {name}: flow_queries={events}, diagnostics={}",
                reused.diagnostics.len()
            );
        }
        let renamed = vec![
            SourceFile::new("renamed_provider.py", "renamed_provider", provider),
            SourceFile::new(
                "main.py",
                "main",
                client.replace("from provider", "from renamed_provider"),
            ),
            original[2].clone(),
        ];
        assert_eq!(
            session.check_with_must_use(&renamed, "3.11"),
            crate::check_with_must_use(&renamed, "3.11")
        );
        let deleted = vec![renamed[1].clone(), renamed[2].clone()];
        assert_eq!(
            session.check_with_must_use(&deleted, "3.11"),
            crate::check_with_must_use(&deleted, "3.11")
        );
        let recovered = session.check_with_must_use(&original, "3.11");
        assert_eq!(recovered, initial);
        std::fs::remove_dir_all(root).unwrap();
    }
}

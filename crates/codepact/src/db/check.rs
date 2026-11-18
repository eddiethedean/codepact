use super::{
    queries::{check_flow_query, select_provider_query},
    AnalysisCancelled, AnalysisSession, SessionCheckResult, SessionInspection,
};
use crate::{
    foundation::{
        self, ComparisonDescriptor, FoundationEvidence, FoundationInputs, ModelUse,
        PreparedFoundations, ProviderIssue, ScopedSource,
    },
    model, CheckOptions, CheckResult, Inspection, Ownership, SourceFile,
};
use std::{collections::BTreeSet, sync::Arc};

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

pub(super) struct CheckFeatures {
    pub(super) must_use: bool,
    pub(super) finite_matches: bool,
    pub(super) review: bool,
}

impl AnalysisSession {
    pub(super) fn run_check(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        options: CheckOptions,
        inputs: Option<&FoundationInputs>,
        capture_review: bool,
    ) -> Result<(SessionCheckResult, crate::review::operations::Journal), AnalysisCancelled> {
        let mut analysis_files = files.to_vec();
        analysis_files.extend(
            options
                .provider_sources
                .into_iter()
                .map(crate::ProviderSource::into_source_file),
        );
        let prepared = inputs
            .map(|inputs| self.prepare_foundations(&mut analysis_files, python_version, inputs))
            .transpose()?;
        let (checked, model_uses, review_operations) = self.check_cancellable_with_options(
            &analysis_files,
            python_version,
            options.ownership,
            prepared.clone(),
            CheckFeatures {
                must_use: true,
                finite_matches: options.experimental_finite_matches,
                review: capture_review,
            },
        )?;
        let mut result = build_check_result(
            checked.inspection,
            python_version,
            &self.model_catalog,
            options.ownership,
        );
        if let Some(prepared) = prepared {
            for issue in prepared
                .issues
                .iter()
                .filter(|issue| issue.import_scope == "<async-guards>")
            {
                result.coverage.status = "incomplete".into();
                result.coverage.reasons.push(issue.reason.clone());
            }
            let scope = self.foundation_scope(&analysis_files, python_version, &prepared)?;
            add_foundations_evidence(
                &mut result,
                python_version,
                options.ownership,
                options.experimental_finite_matches,
                &prepared,
                scope,
                model_uses,
            );
        }
        Ok((
            SessionCheckResult {
                revision: checked.revision,
                result,
            },
            review_operations,
        ))
    }

    fn prepare_foundations(
        &self,
        files: &mut Vec<SourceFile>,
        python_version: &str,
        inputs: &FoundationInputs,
    ) -> Result<Arc<PreparedFoundations>, AnalysisCancelled> {
        let (prepared, provider_files) = salsa::Cancelled::catch(|| {
            let mut prepared = PreparedFoundations::new(inputs);
            if let Err(reason) =
                foundation::validate_async_guards(&prepared.async_guards, python_version)
            {
                prepared.issues.push(ProviderIssue {
                    import_scope: "<async-guards>".into(),
                    reason,
                });
                prepared.async_guards.clear();
            }
            prepared.async_guards.retain(|guard| {
                if files.iter().any(|file| {
                    file.module == guard.module
                        && !file.path.starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
                }) {
                    true
                } else {
                    prepared.issues.push(ProviderIssue {
                        import_scope: "<async-guards>".into(),
                        reason: format!(
                            "reviewed asyncio guard module {} is outside the selected source scope",
                            guard.module
                        ),
                    });
                    false
                }
            });
            let mut provider_files = Vec::new();
            let mut session_bytes = files.iter().map(|file| file.source.len()).sum::<usize>();
            let mut reviewed_bytes = 0usize;
            let mut top_levels = files
                .iter()
                .filter_map(|file| file.module.split('.').next().map(str::to_owned))
                .collect::<BTreeSet<_>>();
            for provider in &inputs.providers {
                let top = provider.import_scope.split('.').next().unwrap_or_default();
                if !top_levels.insert(top.to_owned()) {
                    continue;
                }
                let bytes = provider
                    .sources
                    .iter()
                    .map(|file| file.source.len())
                    .sum::<usize>();
                if provider.reviewed_library_limit().is_some()
                    && !inputs.project_models.iter().any(|model| {
                        model
                            .factory
                            .strip_prefix(&provider.import_scope)
                            .is_some_and(|tail| tail.starts_with('.'))
                    })
                {
                    reviewed_bytes = reviewed_bytes.saturating_add(bytes);
                    let validation = if reviewed_bytes > 16_000_000 {
                        Err("reviewed library source byte budget exceeded".into())
                    } else {
                        provider.validate(python_version)
                    };
                    if let Err(reason) = validation {
                        prepared.issues.push(ProviderIssue {
                            import_scope: provider.import_scope.clone(),
                            reason,
                        });
                    } else {
                        prepared.load_reviewed_library(provider, inputs);
                    }
                    continue;
                }
                let file_count = files
                    .len()
                    .saturating_add(provider_files.len())
                    .saturating_add(provider.sources.len());
                let valid = if file_count > crate::DEFAULT_MAX_INPUT_FILES
                    || session_bytes.saturating_add(bytes) > crate::DEFAULT_MAX_SOURCE_BYTES
                {
                    Err("selected dependency exceeds the shared source file or byte budget".into())
                } else {
                    provider.validate(python_version)
                };
                if let Err(reason) = valid {
                    prepared.issues.push(ProviderIssue {
                        import_scope: provider.import_scope.clone(),
                        reason,
                    });
                    continue;
                }
                let selected = select_provider_query(
                    &self.db,
                    Arc::new(provider.clone()),
                    python_version.to_owned(),
                );
                prepared.load(selected, inputs, python_version);
                provider_files.extend(provider.sources_for_analysis());
                session_bytes += bytes;
            }
            prepared.issues.sort();
            prepared.issues.dedup();
            (prepared, provider_files)
        })
        .map_err(|_| AnalysisCancelled)?;
        files.extend(provider_files);
        Ok(Arc::new(prepared))
    }

    fn foundation_scope(
        &self,
        files: &[SourceFile],
        python_version: &str,
        prepared: &PreparedFoundations,
    ) -> Result<Vec<ScopedSource>, AnalysisCancelled> {
        let modules = files.iter().map(|file| file.module.clone()).collect();
        let mut scope = salsa::Cancelled::catch(|| {
            files
                .iter()
                .map(|file| {
                    let imports = self
                        .files
                        .get(&file.path)
                        .map(|input| {
                            let summary =
                                super::parse_file(&self.db, *input, python_version.to_owned());
                            let mut imports = summary.absolute_imports.clone();
                            imports.extend(prepared.submodule_imports(summary));
                            imports.sort();
                            imports.dedup();
                            imports
                        })
                        .unwrap_or_default();
                    let mut entry = foundation::scope_entry(file, imports, &modules);
                    entry.unresolved_imports.retain(|module| {
                        if prepared.providers.get(module).is_some_and(|provider| {
                            provider.origin == "reviewed-library" && provider.reason.is_none()
                        }) {
                            entry.recognized_imports.push(module.clone());
                            false
                        } else {
                            true
                        }
                    });
                    entry.recognized_imports.sort();
                    entry
                })
                .collect::<Vec<_>>()
        })
        .map_err(|_| AnalysisCancelled)?;
        scope.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(scope)
    }

    pub(super) fn check_cancellable_with_options(
        &mut self,
        files: &[SourceFile],
        python_version: &str,
        ownership: crate::Ownership,
        inputs: Option<Arc<crate::foundation::PreparedFoundations>>,
        features: CheckFeatures,
    ) -> Result<
        (
            SessionInspection,
            Vec<ModelUse>,
            crate::review::operations::Journal,
        ),
        AnalysisCancelled,
    > {
        let CheckFeatures {
            must_use: run_must_use,
            finite_matches,
            review: capture_review,
        } = features;
        if let Some(mut inspection) = rejected_check_input(files) {
            inspection.assurance = if ownership.includes_advanced_contracts() {
                crate::ADVANCED_ASSURANCE
            } else if ownership.is_enabled() {
                crate::OWNERSHIP_ASSURANCE
            } else if run_must_use {
                crate::ADOPTION_ASSURANCE
            } else {
                crate::LIFECYCLE_ASSURANCE
            }
            .into();
            return Ok((
                SessionInspection {
                    revision: self.revision,
                    inspection,
                },
                Vec::new(),
                crate::review::operations::Journal::default(),
            ));
        }
        if self.model_catalog.foundations != inputs {
            self.model_catalog.foundations = inputs.clone();
            self.revision = self.revision.saturating_add(1);
        }
        if self.ownership != ownership {
            self.ownership = ownership;
            self.revision = self.revision.saturating_add(1);
        }
        if self.finite_matches != finite_matches {
            self.finite_matches = finite_matches;
            self.revision = self.revision.saturating_add(1);
        }
        let inspected = self.inspect_cancellable_with_must_use(
            files,
            python_version,
            run_must_use,
            finite_matches,
            inputs.clone(),
        )?;
        let revision = inspected.revision;
        let mut inspection = inspected.inspection;
        if inputs.is_none() {
            inspection.contracts.retain(|contract| {
                !contract
                    .source
                    .file
                    .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
            });
        }
        inspection.diagnostics.retain(|diagnostic| {
            !diagnostic
                .primary
                .file
                .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
        });
        inspection
            .coverage
            .reasons
            .retain(|reason| !reason.starts_with(crate::DEPENDENCY_SOURCE_PREFIX));
        if inspection.coverage.reasons.is_empty() {
            inspection.coverage.status = "complete-for-supported-subset".into();
        }
        let models = self.model_catalog.clone();
        let checked = salsa::Cancelled::catch(|| {
            check_flow_query(
                &self.db,
                self.files.values().copied().collect(),
                python_version.to_string(),
                models,
                inspection,
                ownership,
                capture_review,
            )
        });
        let (mut result, model_uses, review_operations) =
            checked.map_err(|_| AnalysisCancelled)?.clone();
        result.contracts.retain(|contract| {
            !contract
                .source
                .file
                .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
        });
        result.diagnostics.retain(|diagnostic| {
            !diagnostic
                .primary
                .file
                .starts_with(crate::DEPENDENCY_SOURCE_PREFIX)
        });
        result
            .coverage
            .reasons
            .retain(|reason| !reason.starts_with(crate::DEPENDENCY_SOURCE_PREFIX));
        if result.coverage.reasons.is_empty() {
            result.coverage.status = "complete-for-supported-subset".into();
        }
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
        result.assurance = if ownership.includes_advanced_contracts() {
            crate::ADVANCED_ASSURANCE
        } else if ownership.is_enabled() {
            crate::OWNERSHIP_ASSURANCE
        } else if run_must_use {
            crate::ADOPTION_ASSURANCE
        } else {
            "lifecycle-checking-for-supported-exit-flow"
        }
        .into();
        Ok((
            SessionInspection {
                revision,
                inspection: result,
            },
            model_uses,
            review_operations,
        ))
    }
}

fn build_check_result(
    inspection: Inspection,
    python_version: &str,
    catalog: &model::FlowModelCatalog,
    ownership: Ownership,
) -> CheckResult {
    let mut result = CheckResult::from_inspection_with_models(inspection, python_version, catalog);
    if let Some(metadata) = model::task_group_metadata(python_version)
        .filter(|_| ownership.assumes_default_task_factory())
    {
        result.model_catalog_version = model::TASK_GROUP_CATALOG_VERSION;
        result.available_models.push(metadata);
    }
    if ownership.includes_advanced_contracts() {
        result.model_catalog_version = model::ADVANCED_MODEL_CATALOG_VERSION;
        result
            .available_models
            .extend(model::thread_pool_executor_metadata(python_version));
    }
    result
        .available_models
        .sort_by(|left, right| left.id.cmp(&right.id));
    result
}

fn add_foundations_evidence(
    result: &mut CheckResult,
    python_version: &str,
    ownership: Ownership,
    finite_matches: bool,
    prepared: &PreparedFoundations,
    scope: Vec<ScopedSource>,
    model_uses: Vec<ModelUse>,
) {
    result.check_schema_version = foundation::CHECK_SCHEMA_VERSION;
    result.model_catalog_version = foundation::MODEL_CATALOG_VERSION;
    if let Some(runtime) = &prepared.runtime {
        result.available_models.extend(foundation::stdlib_catalog(
            runtime,
            python_version,
            ownership,
        ));
        result
            .available_models
            .sort_by(|left, right| left.id.cmp(&right.id));
    }
    result.foundations = Some(FoundationEvidence {
        runtime: prepared.runtime.clone(),
        catalog_revision: foundation::CATALOG_REVISION.into(),
        model_uses,
        providers: prepared
            .providers
            .values()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        provider_issues: prepared.issues.clone(),
        scope,
        exhausted_limits: result
            .coverage
            .reasons
            .iter()
            .filter(|reason| reason.contains("limit") || reason.contains("budget"))
            .cloned()
            .collect(),
        lock_sha256: prepared.lock_digest.clone(),
        project_models_sha256: prepared.project_models_digest.clone(),
        comparison: ComparisonDescriptor {
            schema_version: 1,
            engine_version: env!("CARGO_PKG_VERSION").into(),
            rule_semantic_revision: crate::adoption::RULE_SEMANTIC_VERSION,
            finding_identity_revision: crate::adoption::FINGERPRINT_ALGORITHM_VERSION,
            source_target: python_version.into(),
            ownership,
            finite_matches,
            catalog_revision: foundation::CATALOG_REVISION.into(),
            normalization_revision: 1,
            preservation_policy_revision: 1,
        },
    });
}

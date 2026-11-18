//! Source snapshots and bounded preservation review shared by Rust and the CLI.

mod compare;
pub(crate) mod operations;

pub use compare::{compare, FindingChange, ReviewReason, ReviewReport, ReviewVerdict};

use crate::{
    adoption::{self, EffectiveConfig, ReviewAnchorCache},
    foundation::{sha256, FoundationEvidence, FoundationInputs},
    AnalysisSession, CheckOptions, Contract, SourceFile, Span,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SNAPSHOT_SCHEMA_VERSION: u8 = 1;
pub const REVIEW_SCHEMA_VERSION: u8 = 1;
pub const MAX_REVIEW_BYTES: usize = 32 * 1024 * 1024;
const COMPARISON_ALGORITHM_VERSION: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub engine_version: String,
    pub check_schema_version: u8,
    pub rule_semantics: u8,
    pub finding_identity: u8,
    pub comparison_algorithm: u8,
    pub python_target: String,
    pub ownership: crate::Ownership,
    pub finite_matches: bool,
    pub assurance: String,
    pub model_catalog: u8,
    pub reviewed_providers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewFile {
    pub logical_path: String,
    pub module: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewFinding {
    pub fingerprint: String,
    pub code: String,
    pub primary: Span,
    pub certainty: String,
    pub severity: String,
    pub actionable: bool,
    pub disposition: String,
    pub disposition_reason: Option<String>,
    pub message: String,
    pub operation: Option<String>,
    pub condition: Option<String>,
    pub completion: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewOperation {
    pub identity: String,
    pub origin: Span,
    pub completion: String,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicy {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub enabled_rules: BTreeSet<String>,
    pub mode: String,
    pub fail_on: String,
    pub severity: BTreeMap<String, String>,
    pub declaration_policy: Vec<adoption::DeclarationRequirement>,
    pub required_providers: Vec<String>,
    pub baseline_sha256: Option<String>,
    pub suppressions: BTreeMap<String, Vec<(String, String)>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSnapshot {
    pub review_snapshot_schema_version: u8,
    pub snapshot_id: String,
    pub descriptor: Descriptor,
    pub selected_files: Vec<ReviewFile>,
    pub dependency_roots: Vec<String>,
    pub policy: ReviewPolicy,
    pub settings_origins: BTreeMap<String, String>,
    pub contracts: BTreeSet<String>,
    pub models: Vec<crate::CheckModelMetadata>,
    pub foundations: Option<FoundationEvidence>,
    pub findings: Vec<ReviewFinding>,
    pub operations: Vec<ReviewOperation>,
    pub coverage: crate::Coverage,
    pub details_truncated: bool,
    pub check_exit: u8,
}

fn normalized_contract(owner: &str, contract: &Contract) -> String {
    serde_json::to_string(&(
        contract.schema_version,
        owner,
        &contract.base_symbol,
        &contract.display_name,
        &contract.lifecycle,
        &contract.access,
        &contract.provenance,
    ))
    .expect("contract representation serializes")
}

fn source_outcome_agreements(
    files: &[SourceFile],
    target: &str,
) -> Result<BTreeSet<String>, String> {
    use crate::frontend::Declaration;

    let publication = crate::semantic::publication_inspection(files, target)?;
    let mut agreements = publication
        .must_use
        .keys()
        .map(|symbol| {
            serde_json::to_string(&(symbol, "must-use")).expect("result use agreement serializes")
        })
        .collect::<BTreeSet<_>>();
    let modules = publication.declarations.keys().cloned().collect();
    for (module, declarations) in publication.declarations {
        let outcomes = crate::flow::outcomes::scan_declarations(&declarations, &modules);
        let mut add = |name: String, start| {
            if let Some(contract) = outcomes.returns.get(&start) {
                agreements.insert(
                    serde_json::to_string(&(name, "must-handle-return", contract.capability()))
                        .expect("outcome agreement serializes"),
                );
            }
        };
        for declaration in declarations {
            match declaration {
                Declaration::Function { name, range, .. } => {
                    add(format!("{module}.{name}"), range.start);
                }
                Declaration::Class { name, methods, .. } => {
                    for method in methods {
                        add(
                            format!("{module}.{name}.{}", method.name),
                            method.range.start,
                        );
                    }
                }
                _ => {}
            }
        }
    }
    Ok(agreements)
}

fn canonical_digest(value: &impl Serialize) -> String {
    sha256(&serde_json::to_vec(value).expect("canonical review fields serialize"))
}

fn operation_identity(
    source: &SourceFile,
    span: &Span,
    contract: &[String],
    kind: &str,
    anchors: &mut ReviewAnchorCache,
) -> String {
    canonical_digest(&(
        &source.module,
        kind,
        adoption::review_site_anchor(&source.source, span, anchors),
        contract,
    ))
}

fn source_suppressions(files: &[SourceFile]) -> BTreeMap<String, Vec<(String, String)>> {
    let mut suppressions = BTreeMap::new();
    let mut anchors = ReviewAnchorCache::default();
    for file in files {
        let starts = crate::source::line_starts(&file.source);
        let mut comments = Vec::new();
        for (line, comment) in adoption::review_source_comments(&file.source) {
            if !comment
                .strip_prefix('#')
                .is_some_and(|text| text.trim_start().starts_with("codepact:"))
            {
                continue;
            }
            let start = starts[line - 1];
            let end = starts.get(line).copied().unwrap_or(file.source.len());
            let end_column = crate::source::line_content(&file.source, start, end).len() + 1;
            let span = Span {
                file: file.path.clone(),
                line,
                column: 1,
                end_line: line,
                end_column,
            };
            comments.push((
                adoption::review_site_anchor(&file.source, &span, &mut anchors),
                comment.to_owned(),
            ));
        }
        if !comments.is_empty() {
            comments.sort();
            suppressions.insert(file.path.clone(), comments);
        }
    }
    suppressions
}

/// Analyze immutable source inputs and construct a portable review capture.
/// Hosts may reuse the same session across captures. The comparable context is
/// independent of source content and the ordinary adoption-report schema.
pub fn capture(
    session: &mut AnalysisSession,
    files: &[SourceFile],
    config: &EffectiveConfig,
    options: CheckOptions,
    foundations: Option<&FoundationInputs>,
    max_diagnostics: Option<usize>,
) -> Result<ReviewSnapshot, String> {
    if options.ownership != config.ownership
        || options.experimental_finite_matches != config.finite_matches
        || !options.provider_sources.is_empty()
    {
        return Err(
            "review capture requires matching analysis settings and reviewed provider inputs"
                .into(),
        );
    }
    let (mut checked, journal) = session
        .check_with_review_data(files, &config.python_version, options, foundations)
        .map_err(|_| "analysis was cancelled; no review snapshot was produced")?;
    adoption::require_provider_publications(&mut checked.result, files, &config.required_providers);
    let (adoption, baseline_digest) = adoption::build_report_with_baseline_digest(
        checked.result,
        files,
        config,
        config.baseline.as_deref(),
        max_diagnostics,
    )?;
    let by_path = files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut anchors = ReviewAnchorCache::default();
    let operations = journal
        .sites
        .iter()
        .filter_map(|site| {
            let source = by_path.get(site.origin.file.as_str())?;
            let identity = operation_identity(
                source,
                &site.origin,
                &site.contract,
                site.kind,
                &mut anchors,
            );
            Some(ReviewOperation {
                identity,
                origin: site.origin.clone(),
                completion: site.completion.clone(),
                closed: site.closed,
            })
        })
        .collect();
    let mut sites_by_origin = BTreeMap::<_, Vec<_>>::new();
    for site in &journal.sites {
        sites_by_origin.entry(&site.origin).or_default().push(site);
    }
    let mut findings = Vec::with_capacity(adoption.diagnostics.len());
    for item in adoption.diagnostics {
        let diagnostic = item.diagnostic;
        let origin_candidates = diagnostic
            .related
            .iter()
            .filter_map(|span| sites_by_origin.get(span))
            .flatten()
            .filter(|site| {
                site.kind == diagnostic.code
                    && site.contract == diagnostic.semantic_identity
                    && diagnostic
                        .completion
                        .as_ref()
                        .is_none_or(|completion| completion == &site.completion)
            })
            .map(|site| (&site.origin, &site.contract))
            .collect::<BTreeSet<_>>();
        let operation = if let [(origin, contract)] =
            origin_candidates.into_iter().collect::<Vec<_>>().as_slice()
        {
            by_path.get(origin.file.as_str()).map(|source| {
                operation_identity(source, origin, contract, &diagnostic.code, &mut anchors)
            })
        } else {
            None
        };
        let condition = by_path
            .get(diagnostic.primary.file.as_str())
            .and_then(|file| {
                if diagnostic.semantic_identity.is_empty() {
                    return None;
                }
                Some(canonical_digest(&(
                    &diagnostic.code,
                    &diagnostic.message,
                    &file.module,
                    adoption::review_site_anchor(&file.source, &diagnostic.primary, &mut anchors),
                    &diagnostic.semantic_identity,
                    &diagnostic.completion,
                    &diagnostic.trust,
                )))
            });
        findings.push(ReviewFinding {
            fingerprint: item.fingerprint,
            code: diagnostic.code,
            primary: diagnostic.primary,
            certainty: diagnostic.certainty,
            severity: item.effective_severity,
            actionable: item.actionable,
            disposition: item.disposition,
            disposition_reason: item.disposition_reason,
            message: diagnostic.message,
            operation,
            condition,
            completion: diagnostic.completion,
        });
    }
    let selected_files = by_path
        .values()
        .map(|file| ReviewFile {
            logical_path: file.path.clone(),
            module: file.module.clone(),
            sha256: sha256(file.source.as_bytes()),
        })
        .collect();
    let policy = ReviewPolicy {
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        enabled_rules: config.enabled_rules.clone(),
        mode: config.mode.clone(),
        fail_on: config.fail_on.clone(),
        severity: config.severity.clone(),
        declaration_policy: config.declaration_policy.clone(),
        required_providers: config.required_providers.clone(),
        baseline_sha256: (!baseline_digest.is_empty()).then_some(baseline_digest),
        suppressions: source_suppressions(files),
    };
    let dependency_roots = config
        .dependency_roots
        .iter()
        .map(|path| {
            path.strip_prefix(&config.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let mut contracts = adoption
        .contracts
        .iter()
        .map(|contract| {
            let path = contract.source.file.as_str();
            let owner = by_path.get(path).map_or(path, |file| file.module.as_str());
            normalized_contract(owner, contract)
        })
        .collect::<BTreeSet<_>>();
    let mut details_truncated = journal.truncated || adoption.details_truncated;
    match source_outcome_agreements(files, &config.python_version) {
        Ok(agreements) => contracts.extend(agreements),
        Err(_) => details_truncated = true,
    }
    if let (Some(input), Some(evidence)) = (foundations, adoption.foundations.as_ref()) {
        for package in &input.providers {
            if evidence.providers.iter().any(|selection| {
                selection.import_scope == package.import_scope
                    && selection.reason.is_none()
                    && selection.artifact_sha256 == package.digest()
            }) {
                contracts.extend(crate::foundation::normalized_provider_agreements(
                    package,
                    &config.python_version,
                ));
            }
        }
    }
    let mut captured = ReviewSnapshot {
        review_snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: String::new(),
        descriptor: Descriptor {
            engine_version: adoption.engine_version,
            check_schema_version: adoption.check_schema_version,
            rule_semantics: adoption::RULE_SEMANTIC_VERSION,
            finding_identity: adoption::FINGERPRINT_ALGORITHM_VERSION,
            comparison_algorithm: COMPARISON_ALGORITHM_VERSION,
            python_target: adoption.target_python_version,
            ownership: config.ownership,
            finite_matches: config.finite_matches,
            assurance: adoption.assurance,
            model_catalog: adoption.model_catalog_version,
            reviewed_providers: adoption.foundations.is_some(),
        },
        selected_files,
        dependency_roots,
        policy,
        settings_origins: adoption.settings_origins,
        contracts,
        models: adoption.available_models,
        foundations: adoption.foundations,
        findings,
        operations,
        coverage: adoption.coverage,
        details_truncated,
        check_exit: adoption.exit_code,
    };
    captured.snapshot_id = canonical_digest(&captured);
    Ok(captured)
}

impl ReviewSnapshot {
    pub fn read(text: &str) -> Result<Self, String> {
        if text.len() > MAX_REVIEW_BYTES {
            return Err("review snapshot exceeds input byte limit".into());
        }
        let mut capture: Self =
            serde_json::from_str(text).map_err(|e| format!("invalid review snapshot: {e}"))?;
        if capture.review_snapshot_schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported review snapshot schema {}",
                capture.review_snapshot_schema_version
            ));
        }
        let snapshot_id = std::mem::take(&mut capture.snapshot_id);
        if canonical_digest(&capture) != snapshot_id {
            return Err("review snapshot content does not match its snapshot identity".into());
        }
        capture.snapshot_id = snapshot_id;
        Ok(capture)
    }
}

pub fn serialize(value: &impl Serialize) -> Result<String, String> {
    String::from_utf8(adoption::serialize_limited(
        value,
        MAX_REVIEW_BYTES,
        "review output",
    )?)
    .map_err(|error| error.to_string())
}

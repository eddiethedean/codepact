use super::{ReviewFinding, ReviewOperation, ReviewSnapshot, REVIEW_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Passed,
    Failed,
    ReviewRequired,
    Inconclusive,
}

impl ReviewVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::ReviewRequired => "review-required",
            Self::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewReason {
    pub category: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingChange {
    pub classification: String,
    pub before: Option<ReviewFinding>,
    pub after: Option<ReviewFinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReport {
    pub review_schema_version: u8,
    pub before_snapshot: String,
    pub after_snapshot: String,
    pub claim: String,
    pub after_check_exit: u8,
    pub verdict: ReviewVerdict,
    pub exit_code: u8,
    pub reasons: Vec<ReviewReason>,
    pub changed_sections: BTreeSet<String>,
    pub findings: Vec<FindingChange>,
    pub added_files: Vec<String>,
    pub removed_files: Vec<String>,
    pub added_contracts: Vec<String>,
    pub removed_contracts: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Blocking {
    Review,
    Incomplete,
    Failed,
}

fn reason(
    reasons: &mut BTreeMap<ReviewReason, Blocking>,
    level: Blocking,
    category: &str,
    detail: impl Into<String>,
) {
    let reason = ReviewReason {
        category: category.into(),
        detail: detail.into(),
    };
    reasons
        .entry(reason)
        .and_modify(|current| *current = (*current).max(level))
        .or_insert(level);
}

fn complete(snapshot: &ReviewSnapshot) -> bool {
    snapshot.coverage.status == "complete-for-supported-subset"
        && !snapshot.details_truncated
        && snapshot.foundations.as_ref().is_none_or(|evidence| {
            evidence.exhausted_limits.is_empty()
                && evidence.provider_issues.is_empty()
                && evidence
                    .scope
                    .iter()
                    .all(|file| file.unresolved_imports.is_empty())
        })
}

struct MissingEvidence<'a> {
    all: bool,
    files: BTreeSet<&'a str>,
}

impl<'a> MissingEvidence<'a> {
    fn from_capture(snapshot: &'a ReviewSnapshot) -> Self {
        let mut missing = Self {
            all: snapshot.details_truncated,
            files: BTreeSet::new(),
        };
        for reason in &snapshot.coverage.reasons {
            if let Some((file, _)) = reason.split_once(':') {
                missing.files.insert(file);
            } else {
                missing.all = true;
            }
        }
        if let Some(evidence) = &snapshot.foundations {
            missing.all |=
                !evidence.exhausted_limits.is_empty() || !evidence.provider_issues.is_empty();
            missing
                .files
                .extend(evidence.scope.iter().filter_map(|scope| {
                    (!scope.unresolved_imports.is_empty()).then_some(scope.path.as_str())
                }));
        }
        missing
    }

    fn affects(&self, file: &str) -> bool {
        self.all || self.files.contains(file)
    }
}

type Index<'a> = BTreeMap<&'a str, Vec<usize>>;

fn index<'a>(
    findings: &'a [ReviewFinding],
    key: impl Fn(&'a ReviewFinding) -> Option<&'a str>,
) -> Index<'a> {
    let mut result = BTreeMap::<_, Vec<_>>::new();
    for (i, finding) in findings.iter().enumerate() {
        if let Some(key) = key(finding) {
            result.entry(key).or_default().push(i);
        }
    }
    result
}

enum Match {
    Unique(usize),
    Ambiguous,
    Absent,
}

fn find_site(key: &str, before: &Index<'_>, after: &Index<'_>) -> Match {
    match (
        before.get(key).map_or(&[][..], Vec::as_slice),
        after.get(key).map_or(&[][..], Vec::as_slice),
    ) {
        ([i], [_]) => Match::Unique(*i),
        ([], _) => Match::Absent,
        _ => Match::Ambiguous,
    }
}

fn operation_index(sites: &[ReviewOperation]) -> BTreeMap<&str, BTreeMap<&crate::Span, bool>> {
    let mut indexed = BTreeMap::<_, BTreeMap<_, _>>::new();
    for site in sites {
        indexed
            .entry(site.identity.as_str())
            .or_default()
            .entry(&site.origin)
            .and_modify(|closed| *closed &= site.closed)
            .or_insert(site.closed);
    }
    indexed
}

fn stronger(after: &ReviewFinding, before: &ReviewFinding) -> bool {
    after.actionable
        && (!before.actionable
            || (after.certainty == "definite" && before.certainty != "definite")
            || (after.severity == "error" && before.severity != "error"))
}

/// Compare captures. A supplied current capture was generated from the
/// workspace by the trusted host. A hash in an input file grants no trust.
pub fn compare(
    before: &ReviewSnapshot,
    after: &ReviewSnapshot,
    repair_targets: &BTreeSet<String>,
    current: Option<&ReviewSnapshot>,
) -> Result<ReviewReport, String> {
    let mut reasons = BTreeMap::new();
    let mut changed_sections = BTreeSet::new();
    let comparable = before.descriptor.comparison_algorithm == super::COMPARISON_ALGORITHM_VERSION
        && before.descriptor == after.descriptor
        && before
            .foundations
            .as_ref()
            .map(|evidence| &evidence.comparison)
            == after
                .foundations
                .as_ref()
                .map(|evidence| &evidence.comparison);
    if !comparable {
        changed_sections.insert("semantics".into());
        reason(
            &mut reasons,
            Blocking::Incomplete,
            "semantics",
            "checker or target semantics are unsupported or differ; recapture both sides with one qualified checker and target",
        );
    }
    let complete_evidence = complete(before) && complete(after);
    for (label, capture) in [("before", before), ("after", after)] {
        if !complete(capture) {
            changed_sections.insert("coverage".into());
            reason(
                &mut reasons,
                Blocking::Incomplete,
                "coverage",
                format!("{label} capture has incomplete or truncated required evidence"),
            );
            if capture.details_truncated {
                reason(
                    &mut reasons,
                    Blocking::Incomplete,
                    "capture-limit",
                    format!("{label} capture omitted required review details"),
                );
            }
            for detail in &capture.coverage.reasons {
                reason(
                    &mut reasons,
                    Blocking::Incomplete,
                    "coverage",
                    format!("{label}: {detail}"),
                );
            }
            if let Some(evidence) = &capture.foundations {
                for limit in &evidence.exhausted_limits {
                    changed_sections.insert("budgets".into());
                    reason(
                        &mut reasons,
                        Blocking::Incomplete,
                        "budget",
                        format!("{label}: {limit}"),
                    );
                }
                for issue in &evidence.provider_issues {
                    reason(
                        &mut reasons,
                        Blocking::Incomplete,
                        "provider",
                        format!("{label} {}: {}", issue.import_scope, issue.reason),
                    );
                }
                for file in &evidence.scope {
                    if !file.unresolved_imports.is_empty() {
                        reason(
                            &mut reasons,
                            Blocking::Incomplete,
                            "scope",
                            format!(
                                "{label} {} has unresolved imports: {}",
                                file.path,
                                file.unresolved_imports.join(", ")
                            ),
                        );
                    }
                }
            }
        }
    }
    if before.coverage.status != after.coverage.status
        || before.coverage.reasons != after.coverage.reasons
    {
        changed_sections.insert("coverage".into());
    }
    if current.is_some_and(|current| current.snapshot_id != after.snapshot_id) {
        reason(
            &mut reasons,
            Blocking::Incomplete,
            "current-workspace",
            "after capture does not match the newly captured workspace inputs",
        );
    }

    let before_files = before
        .selected_files
        .iter()
        .map(|file| (file.logical_path.as_str(), file.module.as_str()))
        .collect::<BTreeSet<_>>();
    let after_files = after
        .selected_files
        .iter()
        .map(|file| (file.logical_path.as_str(), file.module.as_str()))
        .collect::<BTreeSet<_>>();
    let removed_files = before_files
        .difference(&after_files)
        .map(|(path, _)| (*path).to_owned())
        .collect::<Vec<_>>();
    let added_files = after_files
        .difference(&before_files)
        .map(|(path, _)| (*path).to_owned())
        .collect::<Vec<_>>();
    if !removed_files.is_empty() || !added_files.is_empty() {
        changed_sections.insert("scope".into());
    }
    if !removed_files.is_empty() {
        reason(
            &mut reasons,
            Blocking::Incomplete,
            "scope",
            format!("selected source scope lost: {}", removed_files.join(", ")),
        );
    }
    if before.dependency_roots != after.dependency_roots {
        changed_sections.insert("scope".into());
        reason(
            &mut reasons,
            Blocking::Review,
            "scope",
            "ordered dependency roots changed",
        );
    }
    if before.policy != after.policy {
        changed_sections.insert("policy".into());
        reason(
            &mut reasons,
            Blocking::Review,
            "policy",
            "effective settings, baseline, or inline suppression changed",
        );
    }
    let added_contracts = after
        .contracts
        .difference(&before.contracts)
        .cloned()
        .collect::<Vec<_>>();
    let removed_contracts = before
        .contracts
        .difference(&after.contracts)
        .cloned()
        .collect::<Vec<_>>();
    if !added_contracts.is_empty() || !removed_contracts.is_empty() {
        changed_sections.insert("contracts".into());
        reason(
            &mut reasons,
            Blocking::Review,
            "contracts",
            "normalized source contracts changed",
        );
    }
    let assumptions_changed = before.models != after.models
        || match (&before.foundations, &after.foundations) {
            (Some(before), Some(after)) => {
                before.runtime != after.runtime
                    || before.catalog_revision != after.catalog_revision
                    || before.providers != after.providers
                    || before.lock_sha256 != after.lock_sha256
                    || before.project_models_sha256 != after.project_models_sha256
            }
            (None, None) => false,
            _ => true,
        };
    if assumptions_changed {
        changed_sections.insert("assumptions".into());
        reason(
            &mut reasons,
            Blocking::Review,
            "assumptions",
            "reviewed provider artifacts, runtime, model assumptions, or lock inputs changed",
        );
    }
    let preserved = comparable
        && complete_evidence
        && reasons.is_empty()
        && before.contracts == after.contracts;

    let before_exact = index(&before.findings, |finding| Some(&finding.fingerprint));
    let after_exact = index(&after.findings, |finding| Some(&finding.fingerprint));
    let before_condition = index(&before.findings, |finding| finding.condition.as_deref());
    let after_condition = index(&after.findings, |finding| finding.condition.as_deref());
    let before_sites = operation_index(&before.operations);
    let after_sites = operation_index(&after.operations);
    let outstanding_after = after
        .findings
        .iter()
        .filter_map(|finding| finding.operation.as_deref())
        .collect::<BTreeSet<_>>();
    let unobserved_before = MissingEvidence::from_capture(before);
    let mut changes = Vec::new();
    let mut matched = BTreeSet::new();
    let mut repaired = BTreeSet::new();
    let mut old_waivers = BTreeMap::new();
    for waiver in before
        .findings
        .iter()
        .filter(|finding| finding.code == "TP5005")
    {
        *old_waivers
            .entry((&waiver.fingerprint, &waiver.message))
            .or_insert(0usize) += 1;
    }
    for finding in &after.findings {
        if finding.code == "TP5005" {
            let remaining = old_waivers
                .entry((&finding.fingerprint, &finding.message))
                .or_default();
            if *remaining > 0 {
                *remaining -= 1;
            } else {
                changed_sections.insert("waivers".into());
                reason(
                    &mut reasons,
                    Blocking::Review,
                    "waiver",
                    format!(
                        "new explicit outcome waiver at {}:{}",
                        finding.primary.file, finding.primary.line
                    ),
                );
            }
        }
        let corresponding = match find_site(&finding.fingerprint, &before_exact, &after_exact) {
            Match::Absent => finding
                .condition
                .as_ref()
                .map_or(Match::Absent, |condition| {
                    find_site(condition, &before_condition, &after_condition)
                }),
            found => found,
        };
        let (classification, old) = match corresponding {
            Match::Unique(i) if matched.insert(i) => {
                let old = &before.findings[i];
                if comparable && stronger(finding, old) {
                    reason(
                        &mut reasons,
                        Blocking::Failed,
                        "regression",
                        format!("worsened finding {} {}", finding.code, finding.fingerprint),
                    );
                    ("worsened", Some(old.clone()))
                } else {
                    ("inherited", Some(old.clone()))
                }
            }
            Match::Unique(_) | Match::Ambiguous => {
                if finding.actionable {
                    reason(
                        &mut reasons,
                        Blocking::Incomplete,
                        "correspondence",
                        format!("ambiguous finding {} {}", finding.code, finding.fingerprint),
                    );
                }
                ("ambiguous", None)
            }
            Match::Absent if comparable && !unobserved_before.affects(&finding.primary.file) => {
                if finding.actionable {
                    reason(
                        &mut reasons,
                        Blocking::Failed,
                        "regression",
                        format!("new finding {} {}", finding.code, finding.fingerprint),
                    );
                }
                ("new", None)
            }
            Match::Absent => ("unmatched", None),
        };
        changes.push(FindingChange {
            classification: classification.into(),
            before: old,
            after: Some(finding.clone()),
        });
    }
    for (i, old) in before.findings.iter().enumerate() {
        if matched.contains(&i) {
            continue;
        }
        let resolved = preserved
            && old.operation.as_ref().is_some_and(|operation| {
                let unique_before = before_sites
                    .get(operation.as_str())
                    .is_some_and(|sites| sites.len() == 1);
                let unique_clean_after = after_sites
                    .get(operation.as_str())
                    .is_some_and(|sites| sites.len() == 1 && sites.values().all(|closed| *closed));
                unique_before
                    && unique_clean_after
                    && !outstanding_after.contains(operation.as_str())
            });
        if resolved {
            repaired.insert(old.fingerprint.as_str());
        }
        let classification = if resolved {
            "resolved-under-preserved-contracts"
        } else if removed_files.contains(&old.primary.file) {
            "code-removed"
        } else {
            "disappeared"
        };
        changes.push(FindingChange {
            classification: classification.into(),
            before: Some(old.clone()),
            after: None,
        });
    }

    for fingerprint in repair_targets {
        if !before_exact.contains_key(fingerprint.as_str()) {
            return Err(format!(
                "repair target {fingerprint} is absent from the before capture"
            ));
        }
        if !repaired.contains(fingerprint.as_str())
            || before_exact
                .get(fingerprint.as_str())
                .is_some_and(|entries| entries.len() != 1)
        {
            reason(
                &mut reasons,
                Blocking::Incomplete,
                "repair",
                format!("requested finding {fingerprint} has no unambiguous preserved repair"),
            );
        }
    }
    if changes
        .iter()
        .any(|change| change.classification != "inherited")
    {
        changed_sections.insert("findings".into());
    }
    let verdict = match reasons.values().max() {
        Some(Blocking::Failed) => ReviewVerdict::Failed,
        Some(Blocking::Incomplete) => ReviewVerdict::Inconclusive,
        Some(Blocking::Review) => ReviewVerdict::ReviewRequired,
        None => ReviewVerdict::Passed,
    };
    Ok(ReviewReport {
        review_schema_version: REVIEW_SCHEMA_VERSION,
        before_snapshot: before.snapshot_id.clone(),
        after_snapshot: after.snapshot_id.clone(),
        claim: if current.is_some() {
            "current-workspace".into()
        } else {
            "historical".into()
        },
        after_check_exit: after.check_exit,
        verdict,
        exit_code: u8::from(verdict != ReviewVerdict::Passed),
        reasons: reasons.into_keys().collect(),
        changed_sections,
        findings: changes,
        added_files,
        removed_files,
        added_contracts,
        removed_contracts,
    })
}

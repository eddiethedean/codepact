use super::{
    baseline::{Baseline, BaselineKey},
    config::{eligible_code, validate_rule},
    fingerprint::{code_before_comment, fingerprint, source_comments, AnchorCache},
    scope::ownership_model_revision,
    suppression_diagnostic, BaselineSummary, EffectiveConfig, RULE_SEMANTIC_VERSION,
};
use crate::{CheckDiagnostic, CheckResult, Coverage, SourceFile};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Suppression {
    pub(super) codes: Vec<String>,
    reason: String,
}

pub(super) struct BaselinePolicy {
    entries: BTreeSet<BaselineKey>,
    compatible: bool,
    reason: String,
}

impl BaselinePolicy {
    pub(super) fn new(loaded: Baseline, result: &CheckResult, config: &EffectiveConfig) -> Self {
        let compatible = loaded.target_python_version == result.target_python_version
            && loaded.model_catalog_version == result.model_catalog_version
            && loaded.rule_semantic_version == RULE_SEMANTIC_VERSION
            && loaded.ownership == config.ownership
            && loaded.ownership_model_revision.as_deref()
                == ownership_model_revision(result).as_deref()
            && loaded.engine_version == env!("CARGO_PKG_VERSION");
        Self {
            entries: loaded.entries.iter().map(BaselineKey::from).collect(),
            compatible,
            reason: loaded.update_reason,
        }
    }
}

pub(super) enum Disposition {
    Active,
    RuleDisabled,
    InlineSuppressed(String),
    BaselineSuppressed,
}

impl Disposition {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RuleDisabled => "rule-disabled",
            Self::InlineSuppressed(_) => "suppressed-inline",
            Self::BaselineSuppressed => "suppressed-baseline",
        }
    }

    pub(super) fn into_reason(self) -> Option<String> {
        match self {
            Self::Active => None,
            Self::RuleDisabled => Some("rule disabled by effective configuration".into()),
            Self::InlineSuppressed(reason) => Some(reason),
            Self::BaselineSuppressed => Some("matching finding in configured baseline".into()),
        }
    }
}

pub(super) struct PolicyDiagnostic {
    pub(super) diagnostic: CheckDiagnostic,
    pub(super) fingerprint: String,
    pub(super) base_severity: String,
    pub(super) effective_severity: String,
    pub(super) disposition: Disposition,
    pub(super) actionable: bool,
}

pub(super) struct PolicyResult {
    pub(super) diagnostics: Vec<PolicyDiagnostic>,
    pub(super) counts: BTreeMap<String, usize>,
    pub(super) counts_by_certainty: BTreeMap<String, usize>,
    pub(super) counts_by_disposition: BTreeMap<String, usize>,
    pub(super) actionable_count: usize,
    pub(super) failures: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) unused_suppressions: Vec<String>,
    pub(super) baseline: BaselineSummary,
}

pub(super) fn eligible(d: &CheckDiagnostic) -> bool {
    eligible_code(&d.code)
}

pub(super) fn parse_suppression(line: &str) -> Result<Option<Suppression>, String> {
    let comment = code_before_comment(line);
    let Some(hash) = line[comment.len()..]
        .find('#')
        .map(|i| &line[comment.len() + i + 1..])
    else {
        return Ok(None);
    };
    let directive = hash.trim_start();
    let Some(directive) = directive.strip_prefix("codepact: ignore[") else {
        return if directive.starts_with("codepact:") {
            Err("unsupported CodePact comment; use `codepact: ignore[CODE] -- reason`".into())
        } else {
            Ok(None)
        };
    };
    let Some(close) = directive.find(']') else {
        return Err("malformed CodePact suppression; expected a closing ]".into());
    };
    let codes = directive[..close]
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if codes.is_empty() || codes.iter().any(String::is_empty) {
        return Err("CodePact suppression requires at least one rule code".into());
    }
    let rest = directive[close + 1..].trim();
    let Some(reason) = rest.strip_prefix("--").map(str::trim) else {
        return Err("CodePact suppression requires `-- reason`".into());
    };
    if reason.is_empty() {
        return Err("CodePact suppression reason must not be empty".into());
    }
    let mut seen = BTreeSet::new();
    for code in &codes {
        validate_rule(code).map_err(|_| format!("unknown suppression rule {code}"))?;
        if !eligible_code(code) {
            return Err(format!("rule {code} cannot be suppressed"));
        }
        if !seen.insert(code) {
            return Err(format!("duplicate suppression rule {code}"));
        }
    }
    Ok(Some(Suppression {
        codes,
        reason: reason.to_owned(),
    }))
}

pub(super) fn apply(
    mut diagnostics: Vec<CheckDiagnostic>,
    coverage: &Coverage,
    files: &[SourceFile],
    config: &EffectiveConfig,
    baseline: Option<&BaselinePolicy>,
) -> PolicyResult {
    let mut contents = BTreeMap::new();
    for file in files {
        contents.insert(file.path.as_str(), file.source.as_str());
    }
    let mut anchors = AnchorCache::default();
    let mut candidates = BTreeMap::<String, usize>::new();
    for diagnostic in &diagnostics {
        *candidates
            .entry(fingerprint(diagnostic, &contents, &mut anchors))
            .or_default() += 1;
    }

    let mut policy_failures = Vec::<String>::new();
    let mut suppression_records = BTreeMap::<(String, usize), Suppression>::new();
    for file in files {
        for (line_number, comment) in source_comments(&file.source) {
            if !comment
                .strip_prefix('#')
                .is_some_and(|text| text.trim_start().starts_with("codepact:"))
            {
                continue;
            }
            match parse_suppression(comment) {
                Err(message) => {
                    policy_failures.push(format!("TP4002 {}:{line_number}: {message}", file.path));
                    diagnostics.push(suppression_diagnostic(&file.path, line_number, 1, message));
                }
                Ok(Some(suppression)) => {
                    suppression_records.insert((file.path.clone(), line_number), suppression);
                }
                Ok(None) => {}
            }
        }
    }
    let mut unused_suppressions = Vec::new();
    for ((path, line), suppression) in &suppression_records {
        if !diagnostics.iter().any(|diagnostic| {
            diagnostic.primary.file == *path
                && diagnostic.primary.line == *line
                && suppression.codes.contains(&diagnostic.code)
        }) {
            unused_suppressions.push(format!("{path}:{line}"));
        }
    }
    unused_suppressions.sort();

    let mut counts = BTreeMap::new();
    let mut counts_by_certainty = BTreeMap::new();
    let mut counts_by_disposition = BTreeMap::new();
    let mut matched_entries = BTreeSet::new();
    let mut actionable_count = 0usize;
    let mut raw = Vec::new();
    for diagnostic in diagnostics {
        let fp = fingerprint(&diagnostic, &contents, &mut anchors);
        let is_eligible = eligible(&diagnostic);
        let disposition = if is_eligible && !config.enabled_rules.contains(&diagnostic.code) {
            Disposition::RuleDisabled
        } else if let Some(suppression) = suppression_records
            .get(&(diagnostic.primary.file.clone(), diagnostic.primary.line))
            .filter(|suppression| is_eligible && suppression.codes.contains(&diagnostic.code))
        {
            Disposition::InlineSuppressed(suppression.reason.clone())
        } else if is_eligible
            && !diagnostic.semantic_identity.is_empty()
            && candidates.get(&fp) == Some(&1)
            && baseline.is_some_and(|baseline| baseline.compatible)
        {
            let key = BaselineKey::for_diagnostic(&diagnostic, fp.clone());
            if baseline.is_some_and(|baseline| baseline.entries.contains(&key)) {
                matched_entries.insert(key);
                Disposition::BaselineSuppressed
            } else {
                Disposition::Active
            }
        } else {
            Disposition::Active
        };
        let base = if diagnostic.code == "TP3001" {
            "warning".to_string()
        } else {
            diagnostic.severity.clone()
        };
        let effective = config
            .severity
            .get(&diagnostic.code)
            .map(String::as_str)
            .unwrap_or(&base)
            .to_string();
        let threshold = if config.fail_on == "error" {
            effective == "error"
        } else {
            matches!(effective.as_str(), "error" | "warning")
        };
        let actionable = matches!(disposition, Disposition::Active)
            && ((threshold && diagnostic.certainty != "unknown")
                || (config.mode == "strict" && diagnostic.certainty == "possible")
                || (!is_eligible && diagnostic.severity == "error"));
        if actionable {
            actionable_count += 1;
            policy_failures.push(format!("{} {}", diagnostic.code, diagnostic.message));
        }
        *counts.entry(diagnostic.code.clone()).or_insert(0) += 1;
        *counts_by_certainty
            .entry(diagnostic.certainty.clone())
            .or_insert(0) += 1;
        *counts_by_disposition
            .entry(disposition.label().to_owned())
            .or_insert(0) += 1;
        raw.push(PolicyDiagnostic {
            diagnostic,
            fingerprint: fp,
            base_severity: base,
            effective_severity: effective,
            disposition,
            actionable,
        });
    }
    if config.mode == "strict" && coverage.status != "complete-for-supported-subset" {
        policy_failures.push("analysis coverage is incomplete".into());
    }

    let mut policy_warnings = Vec::new();
    let baseline_summary = baseline.map_or_else(BaselineSummary::default, |baseline| {
        let unmatched = baseline.entries.len().saturating_sub(matched_entries.len());
        if !baseline.compatible {
            policy_warnings
                .push("baseline is stale or incompatible; entries were not applied".into());
        } else if unmatched > 0 {
            policy_warnings.push(format!(
                "{unmatched} baseline entries no longer match current findings"
            ));
        }
        BaselineSummary {
            configured: true,
            compatible: Some(baseline.compatible),
            entries: baseline.entries.len(),
            matched: matched_entries.len(),
            unmatched,
            update_reason: Some(baseline.reason.clone()),
        }
    });
    PolicyResult {
        diagnostics: raw,
        counts,
        counts_by_certainty,
        counts_by_disposition,
        actionable_count,
        failures: policy_failures,
        warnings: policy_warnings,
        unused_suppressions,
        baseline: baseline_summary,
    }
}

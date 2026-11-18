use super::{
    policy::{Disposition, PolicyDiagnostic},
    AdoptionDiagnostic, AdoptionReport,
};
use serde::Serialize;
use std::io::{self, Write};

fn bound_diagnostic_text(text: &mut String, maximum_bytes: usize) -> usize {
    if text.len() <= maximum_bytes {
        return 0;
    }
    let original_length = text.len();
    let ellipsis_length = '…'.len_utf8();
    let mut end = if maximum_bytes >= ellipsis_length {
        maximum_bytes - ellipsis_length
    } else {
        maximum_bytes
    };
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    if maximum_bytes >= ellipsis_length {
        text.push('…');
    }
    original_length.saturating_sub(end)
}

struct LimitedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for LimitedVecWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other(
                "serialized document exceeds its byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialize_pretty_limited<T: Serialize>(
    value: &T,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut writer = LimitedVecWriter {
        bytes: Vec::with_capacity(8192),
        limit,
        exceeded: false,
    };
    serde_json::to_writer_pretty(&mut writer, value).map_err(|error| {
        if writer.exceeded {
            format!("{label} exceeds the {} byte limit", writer.limit)
        } else {
            format!("cannot serialize {label}: {error}")
        }
    })?;
    Ok(writer.bytes)
}

pub fn serialize_report(report: &AdoptionReport) -> Result<String, String> {
    String::from_utf8(serialize_pretty_limited(
        report,
        32 * 1024 * 1024,
        "checker report",
    )?)
    .map_err(|error| format!("checker report is not UTF-8: {error}"))
}

pub(super) struct ReportDetails {
    pub(super) diagnostics: Vec<AdoptionDiagnostic>,
    pub(super) total_diagnostics: usize,
    pub(super) truncated: bool,
}

fn present(item: PolicyDiagnostic) -> AdoptionDiagnostic {
    let PolicyDiagnostic {
        mut diagnostic,
        fingerprint,
        base_severity,
        effective_severity,
        disposition,
        actionable,
    } = item;
    let related_sites_omitted = diagnostic.related.len().saturating_sub(32);
    diagnostic.related.sort_by(|a, b| {
        (&a.file, a.line, a.column, a.end_line, a.end_column).cmp(&(
            &b.file,
            b.line,
            b.column,
            b.end_line,
            b.end_column,
        ))
    });
    if related_sites_omitted > 0 {
        diagnostic.related.truncate(32);
    }
    let mut explanation_budget = 8 * 1024;
    let message_omitted_bytes = bound_diagnostic_text(&mut diagnostic.message, explanation_budget);
    explanation_budget = explanation_budget.saturating_sub(diagnostic.message.len());
    let repair_omitted_bytes = diagnostic.repair.as_mut().map_or(0, |repair| {
        bound_diagnostic_text(repair, explanation_budget)
    });
    let explanation_id = format!("{}.R001", diagnostic.code);
    AdoptionDiagnostic {
        diagnostic,
        fingerprint,
        base_severity,
        effective_severity,
        disposition: disposition.label().to_owned(),
        disposition_reason: disposition.into_reason(),
        explanation_id,
        actionable,
        message_omitted_bytes,
        repair_omitted_bytes,
        related_sites_omitted,
    }
}

pub(super) fn present_details(
    mut items: Vec<PolicyDiagnostic>,
    max: Option<usize>,
) -> Result<ReportDetails, String> {
    if max == Some(0) {
        return Err("--max-diagnostics must be positive".into());
    }
    items.sort_by(|a, b| {
        let location_order = (
            &a.diagnostic.primary.file,
            a.diagnostic.primary.line,
            a.diagnostic.primary.column,
            a.diagnostic.primary.end_line,
            a.diagnostic.primary.end_column,
            &a.diagnostic.code,
            &a.diagnostic.completion,
            &a.fingerprint,
        )
            .cmp(&(
                &b.diagnostic.primary.file,
                b.diagnostic.primary.line,
                b.diagnostic.primary.column,
                b.diagnostic.primary.end_line,
                b.diagnostic.primary.end_column,
                &b.diagnostic.code,
                &b.diagnostic.completion,
                &b.fingerprint,
            ));
        if max.is_some() {
            let rank = |item: &PolicyDiagnostic| {
                if item.actionable {
                    0
                } else {
                    match item.disposition {
                        Disposition::Active => 1,
                        Disposition::RuleDisabled => 2,
                        Disposition::InlineSuppressed(_) | Disposition::BaselineSuppressed => 3,
                    }
                }
            };
            rank(a).cmp(&rank(b)).then(location_order)
        } else {
            location_order
        }
    });
    let max = max.unwrap_or(usize::MAX);
    let total_diagnostics = items.len();
    let truncated = total_diagnostics > max;
    let diagnostics = items.into_iter().take(max).map(present).collect::<Vec<_>>();
    let details_truncated = truncated
        || diagnostics.iter().any(|diagnostic| {
            diagnostic.related_sites_omitted > 0
                || diagnostic.message_omitted_bytes > 0
                || diagnostic.repair_omitted_bytes > 0
        });
    Ok(ReportDetails {
        diagnostics,
        total_diagnostics,
        truncated: details_truncated,
    })
}

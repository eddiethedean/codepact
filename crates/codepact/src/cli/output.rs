use codepact::adoption::{self, AdoptionReport};
pub(super) const MAX_REPORT_BYTES: usize = 32 * 1024 * 1024;
pub(super) struct RenderedOutput {
    pub(super) stdout_text: BoundedText,
    pub(super) stderr_text: BoundedText,
}

pub(super) struct BoundedText {
    pub(super) value: String,
    maximum: usize,
    pub(super) overflow: bool,
}
impl BoundedText {
    pub(super) fn new(maximum: usize) -> Self {
        Self {
            value: String::new(),
            maximum,
            overflow: false,
        }
    }
    pub(super) fn push_str(&mut self, text: &str) {
        if self.overflow || text.len() > self.maximum.saturating_sub(self.value.len()) {
            self.overflow = true;
        } else {
            self.value.push_str(text);
        }
    }
    pub(super) fn push(&mut self, character: char) {
        let mut bytes = [0; 4];
        self.push_str(character.encode_utf8(&mut bytes));
    }
}
impl std::ops::Deref for BoundedText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.value
    }
}
impl std::fmt::Display for BoundedText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.value, formatter)
    }
}
impl std::fmt::Write for BoundedText {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.push_str(text);
        if self.overflow {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}
pub(super) fn concise_record(
    span: &codepact::Span,
    severity: &str,
    code: &str,
    role: &str,
    message: &str,
) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        serde_json::to_string(&span.file).unwrap(),
        span.line,
        span.column,
        severity,
        code,
        role,
        serde_json::to_string(message).unwrap()
    )
}

pub(super) fn framed_stdout_bytes(value: &str) -> usize {
    value
        .len()
        .saturating_add(usize::from(!value.is_empty() && !value.ends_with('\n')))
}

pub(super) fn render_report(
    report: &AdoptionReport,
    output_format: &str,
) -> Result<RenderedOutput, String> {
    let mut stdout_text = BoundedText::new(MAX_REPORT_BYTES);
    let mut stderr_text = BoundedText::new(MAX_REPORT_BYTES);
    match output_format {
        "json" => {
            // The JSON serializer enforces the same byte cap before this move.
            stdout_text.value = match adoption::serialize_report(report) {
                Ok(value) => value,
                Err(error) => return Err(format!("serialization failure: {error}")),
            };
        }
        "concise" => {
            for diagnostic in report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.actionable)
            {
                stdout_text.push_str(&concise_record(
                    &diagnostic.diagnostic.primary,
                    &diagnostic.effective_severity,
                    &diagnostic.diagnostic.code,
                    "primary",
                    &diagnostic.diagnostic.message,
                ));
                for related in &diagnostic.diagnostic.related {
                    let message = format!(
                        "related evidence for {} at {}:{}:{}",
                        diagnostic.diagnostic.code,
                        diagnostic.diagnostic.primary.file,
                        diagnostic.diagnostic.primary.line,
                        diagnostic.diagnostic.primary.column
                    );
                    stdout_text.push_str(&concise_record(
                        related,
                        &diagnostic.effective_severity,
                        &diagnostic.diagnostic.code,
                        "related",
                        &message,
                    ));
                }
            }
            stderr_text.push_str(&format!(
                "{} diagnostics; {} actionable; {} suppressed; {} rule-disabled; {} returned",
                report.total_diagnostics,
                report.actionable_diagnostics,
                report
                    .counts_by_disposition
                    .get("suppressed-inline")
                    .unwrap_or(&0)
                    + report
                        .counts_by_disposition
                        .get("suppressed-baseline")
                        .unwrap_or(&0),
                report
                    .counts_by_disposition
                    .get("rule-disabled")
                    .unwrap_or(&0),
                report.returned_diagnostics,
            ));
            if report.details_truncated {
                stderr_text.push_str("; details truncated");
            }
            if report.coverage.status != "complete-for-supported-subset" {
                stderr_text.push_str(&format!("; coverage {}", report.coverage.status));
            }
            stderr_text.push('\n');
            for warning in &report.policy_warnings {
                stderr_text.push_str(&format!("policy warning: {warning}\n"));
            }
            for unused in &report.unused_suppressions {
                stderr_text.push_str(&format!("unused suppression: {unused}\n"));
            }
        }
        _ => {
            for diagnostic in report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.disposition == "active")
            {
                use std::fmt::Write as FmtWrite;
                let _ = writeln!(
                    &mut stdout_text,
                    "{}:{}:{}: {} {}: {}",
                    diagnostic.diagnostic.primary.file,
                    diagnostic.diagnostic.primary.line,
                    diagnostic.diagnostic.primary.column,
                    diagnostic.effective_severity,
                    diagnostic.diagnostic.code,
                    diagnostic.diagnostic.message
                );
                let _ = writeln!(
                    &mut stdout_text,
                    "  certainty: {}",
                    diagnostic.diagnostic.certainty
                );
                let _ = writeln!(&mut stdout_text, "  trust: {}", diagnostic.diagnostic.trust);
                if let Some(completion) = &diagnostic.diagnostic.completion {
                    let _ = writeln!(&mut stdout_text, "  completion: {completion}");
                }
                if let Some(reason) = &diagnostic.diagnostic.coverage_reason {
                    let _ = writeln!(&mut stdout_text, "  coverage reason: {reason}");
                }
                if let Some(repair) = &diagnostic.diagnostic.repair {
                    let _ = writeln!(&mut stdout_text, "  repair: {repair}");
                }
                for related in &diagnostic.diagnostic.related {
                    let _ = writeln!(
                        &mut stdout_text,
                        "  related: {}:{}:{}",
                        related.file, related.line, related.column
                    );
                }
            }
            use std::fmt::Write as FmtWrite;
            let has_summary = report.total_diagnostics > 0
                || report.coverage.status != "complete-for-supported-subset"
                || !report.policy_warnings.is_empty()
                || !report.unused_suppressions.is_empty();
            if has_summary {
                let _ = writeln!(
                    &mut stdout_text,
                    "{} diagnostics; {} actionable; {} suppressed; {} rule-disabled; {} returned",
                    report.total_diagnostics,
                    report.actionable_diagnostics,
                    report
                        .counts_by_disposition
                        .get("suppressed-inline")
                        .unwrap_or(&0)
                        + report
                            .counts_by_disposition
                            .get("suppressed-baseline")
                            .unwrap_or(&0),
                    report
                        .counts_by_disposition
                        .get("rule-disabled")
                        .unwrap_or(&0),
                    report.returned_diagnostics,
                );
            }
            if report.details_truncated {
                stdout_text.push_str("diagnostic details truncated by --max-diagnostics\n");
            }
            if report.coverage.status != "complete-for-supported-subset" {
                let _ = writeln!(
                    &mut stdout_text,
                    "analysis incomplete: {}",
                    report.coverage.reasons.join("; ")
                );
            }
            for warning in &report.policy_warnings {
                let _ = writeln!(&mut stdout_text, "policy warning: {warning}");
            }
            for unused in &report.unused_suppressions {
                let _ = writeln!(&mut stdout_text, "unused suppression: {unused}");
            }
        }
    }
    if stdout_text.overflow
        || stderr_text.overflow
        || framed_stdout_bytes(&stdout_text).saturating_add(stderr_text.len()) > MAX_REPORT_BYTES
    {
        return Err("rendered report exceeds the 32 MiB byte limit".into());
    }
    Ok(RenderedOutput {
        stdout_text,
        stderr_text,
    })
}

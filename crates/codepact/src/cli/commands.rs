use super::{args::ParsedArgs, input, metadata, output, CommandOutput};
use codepact::{
    adoption::{self, EffectiveConfig},
    CheckDiagnostic, CheckOptions, CheckResult, Inspection, SourceFile, Span,
};
use std::path::Path;

mod baseline;

pub(super) use baseline::update_baseline;

pub(super) fn inspect(parsed: ParsedArgs, cwd: &Path) -> Result<Inspection, String> {
    let target = parsed.overrides.python_version.as_deref().unwrap_or("3.11");
    let files = input::load_inspection(parsed.paths, parsed.overrides.root, target, cwd)?;
    Ok(codepact::inspect(&files, target))
}

pub(super) fn render_inspection(inspection: &Inspection) -> Result<CommandOutput, String> {
    let output = serde_json::to_string_pretty(inspection)
        .map_err(|error| format!("serialization failure: {error}"))?;
    let failed = inspection.diagnostics.iter().any(|d| d.severity == "error")
        || inspection.coverage.status != "complete-for-supported-subset";
    Ok(CommandOutput::stdout(output, u8::from(failed)))
}

pub(super) fn check(parsed: ParsedArgs, cwd: &Path) -> Result<CommandOutput, String> {
    let config = adoption::resolve_config(cwd, &parsed.overrides)?;
    if parsed.show_settings {
        return metadata::settings_json(&config).map(|output| CommandOutput::stdout(output, 0));
    }
    let baseline = config.baseline.as_deref();
    if let Some(baseline) = baseline {
        adoption::validate_baseline_file(baseline)?;
    }
    let loaded = input::load_project(parsed.paths, &config, cwd)?;
    let (files, result) = analyze_project(loaded, &config);
    let report = adoption::build_report(result, &files, &config, baseline, parsed.max_diagnostics)?;
    let rendered = output::render_report(&report, &parsed.output_format)?;
    Ok(CommandOutput {
        stdout: rendered.stdout_text.value,
        stderr: rendered.stderr_text.value,
        status: report.exit_code,
    })
}

fn analyze_project(
    loaded: input::LoadedInputs,
    config: &EffectiveConfig,
) -> (Vec<SourceFile>, CheckResult) {
    let options = CheckOptions {
        ownership: config.ownership,
        experimental_finite_matches: config.finite_matches,
        ..CheckOptions::default()
    };
    let files = loaded.files;
    let mut result = if let Some(foundations) = loaded.foundations {
        codepact::check_with_foundations(&files, &config.python_version, options, foundations)
    } else {
        codepact::check_with_options(&files, &config.python_version, options)
    };
    require_provider_publications(&mut result, &files, &config.required_providers);
    (files, result)
}

fn require_provider_publications(
    result: &mut CheckResult,
    files: &[SourceFile],
    required_providers: &[String],
) {
    for provider in required_providers {
        let expected = codepact::foundation::normalized_distribution(provider).unwrap_or_default();
        if result.foundations.as_ref().is_some_and(|evidence| {
            evidence.providers.iter().any(|selected| {
                selected.distribution == expected
                    && selected.reason.is_none()
                    && matches!(selected.origin.as_str(), "inline-source" | "manifest")
            })
        }) {
            continue;
        }
        let message = format!("required CodePact provider {provider} was not resolved from the selected dependency roots");
        let path = files.first().map_or("<input>", |file| file.path.as_str());
        result.coverage.status = "incomplete".into();
        result.coverage.reasons.push(format!("{path}:1: {message}"));
        result.diagnostics.push(CheckDiagnostic {
            semantic_identity: vec![provider.clone()],
            provider_evidence: None,
            code: "TP9001".into(),
            severity: "warning".into(),
            message: message.clone(),
            primary: Span {
                file: path.into(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
            },
            related: Vec::new(),
            certainty: "unknown".into(),
            completion: None,
            trust: "unresolved-provider".into(),
            coverage_reason: Some(message),
            repair: Some(
                "Add a dependency root containing the marked installed distribution.".into(),
            ),
        });
    }
}

use super::{args::ParsedArgs, input, metadata, output, CommandOutput};
use codepact::{
    adoption::{self, EffectiveConfig},
    CheckOptions, CheckResult, Inspection, SourceFile,
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
    adoption::require_provider_publications(&mut result, &files, &config.required_providers);
    (files, result)
}

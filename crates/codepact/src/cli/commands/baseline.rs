use super::{analyze_project, input, CommandOutput, ParsedArgs};
use codepact::adoption::{self, BaselineUpdateSummary};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub(in crate::cli) fn update_baseline(
    parsed: ParsedArgs,
    cwd: &Path,
) -> Result<CommandOutput, String> {
    let config = adoption::resolve_config(cwd, &parsed.overrides)?;
    let destination = resolve_destination(
        cwd,
        parsed
            .output_baseline
            .as_deref()
            .expect("baseline output was parsed"),
        parsed.replace_baseline,
    )?;
    let loaded = input::load_project(parsed.paths, &config, cwd)?;
    reject_input_replacement(
        &destination,
        &loaded.physical_files,
        config.source.as_deref(),
    )?;
    let (files, result) = analyze_project(loaded, &config);
    let summary = adoption::write_baseline(
        &destination,
        &result,
        &files,
        &config,
        parsed.replace_baseline,
        &parsed.update_reason.expect("baseline reason was parsed"),
    )?;
    Ok(render_summary(&summary, &parsed.output_format))
}

fn resolve_destination(cwd: &Path, requested: &Path, replace: bool) -> Result<PathBuf, String> {
    let destination = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        cwd.join(requested)
    };
    adoption::reject_symlink_components(&destination, "baseline destination")?;
    match fs::symlink_metadata(&destination) {
        Ok(_) if !replace => {
            return Err("baseline destination exists; pass --replace to replace it".into());
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err("baseline destination must be a regular file".into());
        }
        Ok(_) => adoption::validate_baseline_file(&destination)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "cannot inspect baseline destination {}: {error}",
                destination.display()
            ));
        }
    }
    Ok(destination)
}

fn reject_input_replacement(
    destination: &Path,
    physical_files: &BTreeSet<PathBuf>,
    config: Option<&Path>,
) -> Result<(), String> {
    let canonical = fs::canonicalize(destination).or_else(|_| {
        let parent = destination.parent().unwrap_or(Path::new("."));
        fs::canonicalize(parent)
            .map(|parent| parent.join(destination.file_name().unwrap_or_default()))
    });
    if canonical.is_ok_and(|path| {
        physical_files.contains(&path)
            || config
                .and_then(|config| fs::canonicalize(config).ok())
                .is_some_and(|config| config == path)
    }) {
        return Err(
            "baseline output must not replace a selected source or configuration file".into(),
        );
    }
    Ok(())
}

fn render_summary(summary: &BaselineUpdateSummary, format: &str) -> CommandOutput {
    if format == "json" {
        return CommandOutput::stdout(serde_json::to_string_pretty(summary).unwrap(), 0);
    }
    let stdout = format!(
        "baseline updated: {} entries ({} added, {} removed, {} unchanged, {} ambiguous skipped); snapshot {}",
        summary.entries,
        summary.added,
        summary.removed,
        summary.unchanged,
        summary.skipped_ambiguous,
        summary.snapshot_id,
    );
    let mut output = CommandOutput::stdout(stdout, 0);
    if summary.coverage_status != "complete-for-supported-subset" {
        output.stderr = format!(
            "baseline records incomplete coverage: {}\n",
            summary.coverage_status
        );
    }
    output
}

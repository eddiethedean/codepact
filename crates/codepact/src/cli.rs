use codepact::inspect;
#[cfg(test)]
use std::path::PathBuf;
use std::{env, fs, io::Write, path::Path, process::ExitCode};

macro_rules! stderr_line {
    ($($arguments:tt)*) => {{
        let _ = write_stderr(&format!("{}\n", format_args!($($arguments)*)));
    }};
}
fn write_stderr(value: &str) -> Result<(), String> {
    std::io::stderr()
        .lock()
        .write_all(value.as_bytes())
        .map_err(|error| format!("cannot write stderr: {error}"))
}

fn usage() {
    stderr_line!("Usage: codepact inspect-contracts [--root DIR] [--python-version VERSION] [--json] FILE...\n       codepact check [--root DIR] [--config PYPROJECT|--no-config] [--python-version VERSION] [--experimental-ownership|--no-experimental-ownership] [--mode advisory|strict] [--fail-on error|warning] [--rules CODE[,CODE...]] [--severity CODE=LEVEL] [--json|--output-format text|json|concise] [--baseline FILE] [--max-diagnostics N] [FILE...]\n       codepact baseline update --output FILE --reason TEXT [--replace] [--json] [check options]\n       codepact capabilities --json\n       codepact explain RULE --json\n       codepact --version");
}

fn emit_failure(format: &str, message: String) -> ExitCode {
    if format == "json" {
        let value = serde_json::to_string_pretty(&codepact::adoption::failure(message))
            .unwrap_or_else(|_| {
                format!(
                    "{{\"execution_status\":\"failed\",\"engine_version\":\"{}\",\"report_schema_version\":2,\"check_schema_version\":2,\"error\":{{\"category\":\"configuration-or-execution\",\"message\":\"error details could not be serialized\"}}}}",
                    env!("CARGO_PKG_VERSION")
                )
            });
        if let Err(error) = write_stdout(&value) {
            stderr_line!("codepact: {error}");
        }
    } else {
        stderr_line!("codepact: {message}");
    }
    ExitCode::from(2)
}

fn write_stdout(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Ok(());
    }
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(value.as_bytes())
        .and_then(|()| {
            if value.ends_with('\n') {
                Ok(())
            } else {
                stdout.write_all(b"\n")
            }
        })
        .map_err(|error| format!("cannot write output: {error}"))
}

mod args;
mod discovery;
mod input;
mod metadata;
mod output;
use discovery::resolve_project_root;
#[cfg(test)]
use discovery::{discover_python_files, discover_python_files_with_limits, DiscoveryLimits};
#[cfg(test)]
use output::{concise_record, framed_stdout_bytes, BoundedText};

pub(super) fn run() -> ExitCode {
    use codepact::adoption::{self, EffectiveConfig};
    let raw_args: Vec<_> = env::args_os().skip(1).collect();
    if let Some(index) = raw_args.iter().position(|arg| arg.to_str().is_none()) {
        return emit_failure(
            args::invalid_argument_format(&raw_args),
            format!("argument {} is not valid UTF-8", index + 1),
        );
    }
    let mut args = raw_args.into_iter().map(|arg| {
        arg.into_string()
            .expect("argument encoding was validated before parsing")
    });
    let Some(command) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if command == "--version" || command == "-V" {
        if let Err(error) = write_stdout(concat!("codepact ", env!("CARGO_PKG_VERSION"))) {
            return emit_failure("text", error);
        }
        return ExitCode::SUCCESS;
    }
    if command == "--help" || command == "-h" {
        usage();
        return ExitCode::SUCCESS;
    }
    if command == "capabilities" {
        if args.next().as_deref() != Some("--json") || args.next().is_some() {
            return emit_failure("json", "capabilities requires exactly --json".into());
        }
        if let Err(error) = write_stdout(&metadata::capabilities_json()) {
            stderr_line!("codepact: {error}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    if command == "explain" {
        let Some(rule) = args.next() else {
            return emit_failure("json", "explain requires a rule code".into());
        };
        if args.next().as_deref() != Some("--json") || args.next().is_some() {
            return emit_failure("json", "explain requires RULE --json".into());
        }
        let Some(output) = metadata::explanation_json(&rule) else {
            return emit_failure("json", format!("unknown rule identifier {rule}"));
        };
        if let Err(error) = write_stdout(&output) {
            stderr_line!("codepact: {error}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    let inspecting = command == "inspect-contracts";
    let baseline_update = command == "baseline";
    let checking = command == "check" || baseline_update;
    if !inspecting && !checking {
        usage();
        return emit_failure("text", format!("unknown command: {command}"));
    }
    if baseline_update && args.next().as_deref() != Some("update") {
        return emit_failure("text", "baseline supports only the update operation".into());
    }

    let parsed = match args::parse(args, inspecting, baseline_update) {
        Ok(args::ParseOutcome::Parsed(parsed)) => *parsed,
        Ok(args::ParseOutcome::Help) => {
            usage();
            return ExitCode::SUCCESS;
        }
        Err(error) => return emit_failure(&error.format, error.message),
    };
    let args::ParsedArgs {
        overrides,
        explicit_root,
        paths,
        output_format,
        max_diagnostics,
        output_baseline,
        update_reason,
        replace_baseline,
        show_settings,
    } = parsed;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            return emit_failure(
                &output_format,
                format!("cannot determine current directory: {e}"),
            )
        }
    };
    let effective = if checking {
        match adoption::resolve_config(&cwd, &overrides) {
            Ok(c) => Some(c),
            Err(e) => return emit_failure(&output_format, e),
        }
    } else {
        None
    };
    if show_settings {
        let Some(c) = effective.as_ref() else {
            return emit_failure(
                &output_format,
                "--show-settings applies only to check".into(),
            );
        };
        let root = match resolve_project_root(&c.root) {
            Ok(root) => root,
            Err(error) => return emit_failure(&output_format, error),
        };
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "execution_status": "settings-resolved",
            "root": root,
            "config_file": c.source,
            "python_version": c.python_version,
            "include": c.include,
            "exclude": c.exclude,
            "enabled_rules": c.enabled_rules,
            "mode": c.mode,
            "experimental_ownership": c.ownership.is_enabled(),
            "assume_default_task_factory": c.ownership == codepact::Ownership::AssumeDefaultTaskFactory,
            "fail_on": c.fail_on,
            "baseline": c.baseline,
            "severity": c.severity,
            "settings_origins": c.settings_origins
        }))
        .unwrap();
        if let Err(error) = write_stdout(&output) {
            stderr_line!("codepact: {error}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    let baseline_destination = if baseline_update {
        let requested = output_baseline.as_deref().unwrap();
        let destination = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            cwd.join(requested)
        };
        if let Err(error) =
            adoption::reject_symlink_components(&destination, "baseline destination")
        {
            return emit_failure(&output_format, error);
        }
        match fs::symlink_metadata(&destination) {
            Ok(_) if !replace_baseline => {
                return emit_failure(
                    &output_format,
                    "baseline destination exists; pass --replace to replace it".into(),
                );
            }
            Ok(metadata) if !metadata.file_type().is_file() => {
                return emit_failure(
                    &output_format,
                    "baseline destination must be a regular file".into(),
                );
            }
            Ok(_) => {
                if let Err(error) = adoption::validate_baseline_file(&destination) {
                    return emit_failure(&output_format, error);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return emit_failure(
                    &output_format,
                    format!(
                        "cannot inspect baseline destination {}: {error}",
                        destination.display()
                    ),
                )
            }
        }
        Some(destination)
    } else {
        if checking {
            if let Some(baseline) = effective
                .as_ref()
                .and_then(|config| config.baseline.as_deref())
            {
                if let Err(error) = adoption::validate_baseline_file(baseline) {
                    return emit_failure(&output_format, error);
                }
            }
        }
        None
    };
    let root = match input::resolve_root(effective.as_ref(), explicit_root, inspecting, &cwd) {
        Ok(root) => root,
        Err(error) => return emit_failure(&output_format, error),
    };
    let python_version = effective
        .as_ref()
        .map(|c| c.python_version.as_str())
        .or(overrides.python_version.as_deref())
        .unwrap_or("3.11")
        .to_string();
    let loaded = match input::collect_files(
        paths,
        root.as_deref(),
        &python_version,
        checking,
        inspecting,
        effective.as_ref(),
        &cwd,
    ) {
        Ok(loaded) => loaded,
        Err(error) => return emit_failure(&output_format, error),
    };
    let files = loaded.files;
    let physical_files = loaded.physical_files;
    if let Some(destination) = baseline_destination.as_deref() {
        let canonical_destination = fs::canonicalize(destination).or_else(|_| {
            let parent = destination.parent().unwrap_or(Path::new("."));
            fs::canonicalize(parent)
                .map(|parent| parent.join(destination.file_name().unwrap_or_default()))
        });
        if let Ok(canonical_destination) = canonical_destination {
            if physical_files.contains(&canonical_destination)
                || effective
                    .as_ref()
                    .and_then(|config| config.source.as_ref())
                    .and_then(|path| fs::canonicalize(path).ok())
                    .is_some_and(|config_path| config_path == canonical_destination)
            {
                return emit_failure(
                    &output_format,
                    "baseline output must not replace a selected source or configuration file"
                        .into(),
                );
            }
        }
    }
    if inspecting {
        let inspection = inspect(&files, &python_version);
        let value = serde_json::to_string_pretty(&inspection);
        match value {
            Ok(v) => {
                if let Err(error) = write_stdout(&v) {
                    stderr_line!("codepact: {error}");
                    return ExitCode::from(2);
                }
            }
            Err(e) => return emit_failure("text", format!("serialization failure: {e}")),
        }
        return if inspection.diagnostics.iter().any(|d| d.severity == "error")
            || inspection.coverage.status != "complete-for-supported-subset"
        {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }
    let config: EffectiveConfig = effective.expect("checking resolves effective configuration");
    let result = codepact::check_with_ownership(&files, &python_version, config.ownership);
    if baseline_update {
        let destination = baseline_destination
            .as_deref()
            .expect("baseline update destination was preflighted");
        let summary = match adoption::write_baseline(
            destination,
            &result,
            &files,
            &config,
            replace_baseline,
            &update_reason.unwrap(),
        ) {
            Ok(summary) => summary,
            Err(error) => return emit_failure(&output_format, error),
        };
        if output_format == "json" {
            let output = serde_json::to_string_pretty(&summary).unwrap();
            if let Err(error) = write_stdout(&output) {
                stderr_line!("codepact: {error}");
                return ExitCode::from(2);
            }
        } else {
            let output = format!(
                "baseline updated: {} entries ({} added, {} removed, {} unchanged, {} ambiguous skipped); snapshot {}",
                summary.entries,
                summary.added,
                summary.removed,
                summary.unchanged,
                summary.skipped_ambiguous,
                summary.snapshot_id,
            );
            if let Err(error) = write_stdout(&output) {
                stderr_line!("codepact: {error}");
                return ExitCode::from(2);
            }
            if summary.coverage_status != "complete-for-supported-subset"
                && write_stderr(&format!(
                    "baseline records incomplete coverage: {}\n",
                    summary.coverage_status
                ))
                .is_err()
            {
                return ExitCode::from(2);
            }
        }
        return ExitCode::SUCCESS;
    }
    let baseline = config.baseline.as_deref();
    let report = match adoption::build_report(result, &files, &config, baseline, max_diagnostics) {
        Ok(r) => r,
        Err(e) => return emit_failure(&output_format, e),
    };
    let rendered = match output::render_report(&report, &output_format) {
        Ok(rendered) => rendered,
        Err(error) => return emit_failure(&output_format, error),
    };
    let stdout_text = rendered.stdout_text;
    let stderr_text = rendered.stderr_text;
    if let Err(error) = write_stdout(&stdout_text) {
        stderr_line!("codepact: {error}");
        return ExitCode::from(2);
    }
    if !stderr_text.is_empty() && write_stderr(&stderr_text).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::from(report.exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovery_project(name: &str) -> PathBuf {
        let root =
            env::temp_dir().join(format!("codepact-discovery-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn discovery_counts_only_files_selected_by_project_policy() {
        let root = discovery_project("policy-limit");
        fs::create_dir(root.join("generated")).unwrap();
        fs::write(root.join("generated/first.py"), "").unwrap();
        fs::write(root.join("generated/second.py"), "").unwrap();
        fs::write(root.join("main.py"), "").unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[tool.codepact]\nconfig-version=1\nexclude=['generated/**']\n",
        )
        .unwrap();
        let config = codepact::adoption::resolve_config(&root, &Default::default()).unwrap();
        let mut files = Vec::new();
        discover_python_files(&root, &mut files, 1, &|path| {
            codepact::adoption::included(&config, &root, path)
        })
        .unwrap();
        assert_eq!(files, vec![root.join("main.py")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_accepts_exact_file_limit_with_later_empty_directories() {
        let root = discovery_project("exact-limit");
        fs::write(root.join("a.py"), "").unwrap();
        fs::write(root.join("b.py"), "").unwrap();
        fs::create_dir_all(root.join("z/empty")).unwrap();
        let mut files = Vec::new();
        discover_python_files(&root, &mut files, 2, &|_| true).unwrap();
        assert_eq!(files, vec![root.join("a.py"), root.join("b.py")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_rejects_the_next_selected_file_without_exceeding_storage_limit() {
        let root = discovery_project("over-limit");
        for name in ["a.py", "b.py", "c.py"] {
            fs::write(root.join(name), "").unwrap();
        }
        let mut files = Vec::new();
        let error = discover_python_files(&root, &mut files, 2, &|_| true).unwrap_err();
        assert!(error.contains("2 selected Python file limit"));
        assert_eq!(files, vec![root.join("a.py"), root.join("b.py")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_bounds_total_entries_across_nested_directories() {
        let root = discovery_project("traversal-limit");
        fs::write(root.join("a.py"), "").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/b.py"), "").unwrap();
        fs::write(root.join("z.txt"), "").unwrap();
        let mut files = Vec::new();
        let error = discover_python_files_with_limits(
            &root,
            &mut files,
            10,
            &|_| true,
            DiscoveryLimits {
                entries: 3,
                entries_per_directory: 10,
                depth: 10,
            },
        )
        .unwrap_err();
        assert!(error.contains("3 directory-entry traversal limit"));
        assert!(files.len() <= 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_bounds_entries_buffered_for_each_directory() {
        let root = discovery_project("directory-limit");
        fs::write(root.join("a.py"), "").unwrap();
        fs::write(root.join("b.txt"), "").unwrap();
        fs::write(root.join("c.txt"), "").unwrap();
        let mut files = Vec::new();
        let error = discover_python_files_with_limits(
            &root,
            &mut files,
            10,
            &|_| true,
            DiscoveryLimits {
                entries: 10,
                entries_per_directory: 2,
                depth: 10,
            },
        )
        .unwrap_err();
        assert!(error.contains("exceeds the 2 entry limit"));
        assert!(files.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_bounds_directory_depth_before_recursing_further() {
        let root = discovery_project("depth-limit");
        let nested = root.join("one/two/three");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("main.py"), "").unwrap();
        let mut files = Vec::new();
        let error = discover_python_files_with_limits(
            &root,
            &mut files,
            10,
            &|_| true,
            DiscoveryLimits {
                entries: 10,
                entries_per_directory: 10,
                depth: 2,
            },
        )
        .unwrap_err();
        assert!(error.contains("2 directory-depth limit"));
        assert!(files.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_human_reports_accept_exact_bytes_and_reject_overflow_before_allocation() {
        use std::fmt::Write;
        let mut text = super::BoundedText::new(3);
        text.push_str("é");
        text.push('x');
        assert_eq!(&*text, "éx");
        assert!(!text.overflow);
        assert!(write!(&mut text, "y").is_err());
        assert!(text.overflow);
        assert_eq!(&*text, "éx");
        text.push_str("further output must not be allocated");
        assert_eq!(text.len(), 3);
    }

    #[test]
    fn concise_records_round_trip_adversarial_path_message_and_both_roles() {
        let path = "space colon: tab\t newline\nemoji🙂";
        let message = "message: \"quoted\"\tline\ncombining e\u{301}";
        let span = codepact::Span {
            file: path.into(),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 6,
        };
        for role in ["primary", "related"] {
            let record = super::concise_record(&span, "error", "TP1001", role, message);
            let fields = record
                .trim_end_matches('\n')
                .split('\t')
                .collect::<Vec<_>>();
            assert_eq!(fields.len(), 7);
            assert_eq!(serde_json::from_str::<String>(fields[0]).unwrap(), path);
            assert_eq!(fields[1..6], ["3", "5", "error", "TP1001", role]);
            assert_eq!(serde_json::from_str::<String>(fields[6]).unwrap(), message);
            assert_eq!(record.lines().count(), 1);
        }
    }
    #[test]
    fn rendered_report_accounting_includes_exactly_one_transport_newline() {
        assert_eq!(super::framed_stdout_bytes(""), 0);
        assert_eq!(super::framed_stdout_bytes("é"), 3);
        assert_eq!(super::framed_stdout_bytes("é\n"), 3);
        assert_eq!(super::framed_stdout_bytes("a").saturating_add("b".len()), 3);
    }
}

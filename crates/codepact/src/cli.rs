#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::PathBuf;
use std::{env, io::Write, process::ExitCode};

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
    stderr_line!("Usage: codepact inspect-contracts [--root DIR] [--python-version VERSION] [--json] FILE...\n       codepact check [--root DIR] [--dependency-root DIR] [--config PYPROJECT|--no-config] [--python-version VERSION] [--experimental-ownership|--no-experimental-ownership] [--experimental-advanced-contracts|--no-experimental-advanced-contracts] [--experimental-finite-matches|--no-experimental-finite-matches] [--mode advisory|strict] [--fail-on error|warning] [--rules CODE[,CODE...]] [--severity CODE=LEVEL] [--json|--output-format text|json|concise] [--baseline FILE] [--max-diagnostics N] [FILE...]\n       codepact baseline update --output FILE --reason TEXT [--replace] [--json] [check options]\n       codepact capabilities --json\n       codepact explain RULE --json\n       codepact --version");
    stderr_line!("       codepact contracts export --root DIR --distribution NAME --version VERSION --import-scope PACKAGE [--python-version VERSION]\n       codepact contracts verify WHEEL [--python-version VERSION]\n       codepact contracts diff BEFORE_WHEEL AFTER_WHEEL [--python-version VERSION] [--json]");
    stderr_line!("       codepact review capture [check options]\n       codepact review BEFORE AFTER [--repair FINGERPRINT] [--current DIR] [--json]");
}

fn emit_failure(format: &str, message: String) -> ExitCode {
    if format == "json" {
        let value = serde_json::to_string_pretty(&codepact::adoption::failure(message))
            .unwrap_or_else(|_| {
                format!(
                    "{{\"execution_status\":\"failed\",\"engine_version\":\"{}\",\"report_schema_version\":3,\"check_schema_version\":1,\"error\":{{\"category\":\"configuration-or-execution\",\"message\":\"error details could not be serialized\"}}}}",
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
mod artifacts;
mod commands;
mod discovery;
mod input;
mod metadata;
mod output;
mod providers;
mod publications;
mod review;
#[cfg(test)]
use discovery::{discover_python_files, discover_python_files_with_limits, DiscoveryLimits};
#[cfg(test)]
use output::{concise_record, framed_stdout_bytes, BoundedText};

struct CommandOutput {
    stdout: String,
    stderr: String,
    status: u8,
}

impl CommandOutput {
    fn stdout(stdout: String, status: u8) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            status,
        }
    }
}

fn emit(output: Result<CommandOutput, String>, failure_format: &str) -> ExitCode {
    let output = match output {
        Ok(output) => output,
        Err(message) => return emit_failure(failure_format, message),
    };
    if let Err(error) = write_stdout(&output.stdout) {
        return emit_failure("text", error);
    }
    if !output.stderr.is_empty() && write_stderr(&output.stderr).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::from(output.status)
}

pub(super) fn run() -> ExitCode {
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
    match command.as_str() {
        "--version" | "-V" => emit(
            Ok(CommandOutput::stdout(
                concat!("codepact ", env!("CARGO_PKG_VERSION")).into(),
                0,
            )),
            "text",
        ),
        "--help" | "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        "capabilities" => {
            if args.next().as_deref() != Some("--json") || args.next().is_some() {
                return emit_failure("json", "capabilities requires exactly --json".into());
            }
            emit(
                Ok(CommandOutput::stdout(metadata::capabilities_json(), 0)),
                "json",
            )
        }
        "explain" => {
            let Some(rule) = args.next() else {
                return emit_failure("json", "explain requires a rule code".into());
            };
            if args.next().as_deref() != Some("--json") || args.next().is_some() {
                return emit_failure("json", "explain requires RULE --json".into());
            }
            let result = metadata::explanation_json(&rule)
                .map(|output| CommandOutput::stdout(output, 0))
                .ok_or_else(|| format!("unknown rule identifier {rule}"));
            emit(result, "json")
        }
        "contracts" => {
            let args = args.collect::<Vec<_>>();
            let format = if args.first().is_some_and(|arg| arg == "diff")
                && args.iter().any(|arg| arg == "--json")
            {
                "json"
            } else {
                "text"
            };
            emit(publications::run(args.into_iter()), format)
        }
        "review" => {
            let args = args.collect::<Vec<_>>();
            let format = review::failure_format(&args);
            let output = env::current_dir()
                .map_err(|error| format!("cannot determine current directory: {error}"))
                .and_then(|cwd| review::run(args, &cwd));
            emit(output, format)
        }
        "inspect-contracts" => run_project(args, ProjectCommand::Inspect),
        "check" => run_project(args, ProjectCommand::Check),
        "baseline" if args.next().as_deref() == Some("update") => {
            run_project(args, ProjectCommand::UpdateBaseline)
        }
        "baseline" => emit_failure("text", "baseline supports only the update operation".into()),
        _ => {
            usage();
            emit_failure("text", format!("unknown command: {command}"))
        }
    }
}

enum ProjectCommand {
    Inspect,
    Check,
    UpdateBaseline,
}

fn run_project(args: impl Iterator<Item = String>, command: ProjectCommand) -> ExitCode {
    let parsed = match args::parse(
        args,
        matches!(command, ProjectCommand::Inspect),
        matches!(command, ProjectCommand::UpdateBaseline),
    ) {
        Ok(args::ParseOutcome::Parsed(parsed)) => *parsed,
        Ok(args::ParseOutcome::Help) => {
            usage();
            return ExitCode::SUCCESS;
        }
        Err(error) => return emit_failure(&error.format, error.message),
    };
    let cwd = match env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            return emit_failure(
                &parsed.output_format,
                format!("cannot determine current directory: {error}"),
            )
        }
    };
    let failure_format = parsed.output_format.clone();
    match command {
        ProjectCommand::Inspect => match commands::inspect(parsed, &cwd) {
            Ok(inspection) => emit(commands::render_inspection(&inspection), "text"),
            Err(error) => emit_failure(&failure_format, error),
        },
        ProjectCommand::Check => emit(commands::check(parsed, &cwd), &failure_format),
        ProjectCommand::UpdateBaseline => {
            emit(commands::update_baseline(parsed, &cwd), &failure_format)
        }
    }
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

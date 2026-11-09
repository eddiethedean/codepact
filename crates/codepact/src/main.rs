use codepact::{inspect, is_python_identifier, SourceFile, DEFAULT_MAX_SOURCE_BYTES};
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

const MAX_DISCOVERY_ENTRIES: usize = 100_000;
const MAX_DIRECTORY_ENTRIES: usize = 20_000;
const MAX_DISCOVERY_DEPTH: usize = 256;

#[derive(Clone, Copy)]
struct DiscoveryLimits {
    entries: usize,
    entries_per_directory: usize,
    depth: usize,
}

const DEFAULT_DISCOVERY_LIMITS: DiscoveryLimits = DiscoveryLimits {
    entries: MAX_DISCOVERY_ENTRIES,
    entries_per_directory: MAX_DIRECTORY_ENTRIES,
    depth: MAX_DISCOVERY_DEPTH,
};

fn discover_python_files(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
) -> Result<(), String> {
    discover_python_files_with_limits(root, output, budget, selected, DEFAULT_DISCOVERY_LIMITS)
}

fn discover_python_files_with_limits(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
    limits: DiscoveryLimits,
) -> Result<(), String> {
    let mut visited_entries = 0;
    discover_python_files_inner(
        root,
        output,
        budget,
        selected,
        limits,
        &mut visited_entries,
        0,
    )
}

fn discover_python_files_inner(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
    limits: DiscoveryLimits,
    visited_entries: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > limits.depth {
        return Err(format!(
            "project discovery exceeds the {} directory-depth limit",
            limits.depth
        ));
    }
    let entries = fs::read_dir(root)
        .map_err(|error| format!("cannot read project directory {}: {error}", root.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        if *visited_entries >= limits.entries {
            return Err(format!(
                "project discovery exceeds the {} directory-entry traversal limit",
                limits.entries
            ));
        }
        if paths.len() >= limits.entries_per_directory {
            return Err(format!(
                "project directory {} exceeds the {} entry limit",
                root.display(),
                limits.entries_per_directory
            ));
        }
        let entry = entry.map_err(|error| error.to_string())?;
        *visited_entries += 1;
        paths.push(entry.path());
    }
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir()
            && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | ".venv" | "venv" | "target" | "build" | "dist" | "__pycache__")
            )
        {
            discover_python_files_inner(
                &path,
                output,
                budget,
                selected,
                limits,
                visited_entries,
                depth + 1,
            )?;
        } else if metadata.is_file()
            && path.extension().is_some_and(|extension| extension == "py")
            && selected(&path)
        {
            if output.len() >= budget {
                return Err(format!(
                    "project discovery exceeds the {budget} selected Python file limit"
                ));
            }
            output.push(path);
        }
    }
    Ok(())
}

fn usage() {
    eprintln!("Usage: codepact inspect-contracts [--root DIR] [--python-version VERSION] [--json] FILE...\n       codepact check [--root DIR] [--config PYPROJECT|--no-config] [--python-version VERSION] [--mode advisory|strict] [--fail-on error|warning] [--rules CODE[,CODE...]] [--severity CODE=LEVEL] [--json|--output-format text|json|concise] [--baseline FILE] [--max-diagnostics N] [FILE...]\n       codepact baseline update --output FILE --reason TEXT [--replace] [--json] [check options]\n       codepact capabilities --json\n       codepact explain RULE --json\n       codepact --version");
}

fn module_from_relative_path(path: &Path, python_version: &str) -> Result<String, String> {
    let mut components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let Some(last) = components.last_mut() else {
        return Err(format!(
            "cannot derive a Python module name from {}",
            path.display()
        ));
    };
    if last == "__init__.py" {
        components.pop();
        if components.is_empty() {
            return Err(format!(
                "root-level __init__.py needs a parent package name: {}",
                path.display()
            ));
        }
    } else if let Some(stem) = last.strip_suffix(".py") {
        *last = stem.to_string();
    } else {
        return Err(format!("expected a .py file: {}", path.display()));
    }
    if components
        .iter()
        .any(|part| !is_python_identifier(part, python_version))
    {
        return Err(format!(
            "path does not map to valid Python module components: {}",
            path.display()
        ));
    }
    Ok(components.join("."))
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
            eprintln!("codepact: {error}");
        }
    } else {
        eprintln!("codepact: {message}");
    }
    ExitCode::from(2)
}

fn resolve_project_root(root: &Path) -> Result<PathBuf, String> {
    codepact::adoption::reject_symlink_components(root, "project root")?;
    fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve project root {}: {error}", root.display()))
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

fn main() -> ExitCode {
    use codepact::adoption::{self, EffectiveConfig, Overrides};
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if command == "--version" || command == "-V" {
        println!("codepact {}", env!("CARGO_PKG_VERSION"));
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
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "capability_schema_version": 1,
            "engine_version": env!("CARGO_PKG_VERSION"),
            "checker_schema_version": 2,
            "baseline_schema_version": 1,
            "inspection_schema_version": 0,
            "explanation_schema_version": 1,
            "python_targets": ["3.11", "3.12", "3.13"],
            "rules": [
                {"code":"TP1001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1001.R001"},
                {"code":"TP1002","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1002.R001"},
                {"code":"TP1003","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1003.R001"},
                {"code":"TP2001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP2001.R001"},
                {"code":"TP3001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP3001.R001"},
                {"code":"TP4001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4001.R001"},
                {"code":"TP4002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4002.R001"},
                {"code":"TP9001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9001.R001"},
                {"code":"TP9002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9002.R001"}
            ],
            "formats": ["text", "json", "concise"],
            "settings": ["root", "python-version", "include", "exclude", "enabled-rules", "mode", "fail-on", "baseline", "severity"],
            "limits": {
                "config_bytes": 65536,
                "baseline_bytes": 16777216,
                "baseline_entries": 100000,
                "report_bytes": 33554432,
                "related_sites_per_finding": 32,
                "source_snippet_bytes_per_finding": 2048,
                "explanation_bytes_per_finding": 8192,
                "max_discovery_entries": MAX_DISCOVERY_ENTRIES,
                "max_directory_entries": MAX_DIRECTORY_ENTRIES,
                "max_discovery_depth": MAX_DISCOVERY_DEPTH,
                "max_input_files": codepact::DEFAULT_MAX_INPUT_FILES,
                "max_source_bytes": DEFAULT_MAX_SOURCE_BYTES
            },
            "services": {"pagination": false,"mcp": false,"language_server": false,"persistent_cache": false}
        })).unwrap();
        if let Err(error) = write_stdout(&output) {
            eprintln!("codepact: {error}");
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
        let explanation = match rule.as_str() {
            "TP1001" => ("Use after close", "A modeled resource use occurs after its close transition.", "Keep the use before close or guard it with a lifecycle check.", "A matching resource close transition is resolved in the supported local flow model.", "The model does not prove arbitrary runtime dispatch or external library behavior.", "a.close(); a.read()", "a.read(); a.close()", "resource acquisition and close summary is unresolved", "Move the use before close or establish a supported lifecycle guard."),
            "TP1002" => ("Open cleanup duty", "An owned resource remains open on a modeled function exit.", "Close it on every reported exit or return it through a resolved Owned[T] boundary.", "A supported owned resource is created or transferred into the function.", "Unknown exceptions, dynamic escapes and unsupported concurrency remain coverage boundaries.", "def f():\n    r = Resource()\n    r.close()", "def f():\n    r = Resource()", "the ownership or exit path is not modeled", "Close on each feasible exit or return ownership through the declared contract."),
            "TP1003" => ("Invalid owned return", "An Owned[T] result violates the modeled handoff contract.", "Return a valid open owned resource after cleanup paths have completed.", "The return annotation resolves to the supported Owned[T] contract.", "This does not validate the runtime type or behavior of the returned object.", "return open_resource", "close_resource(); return resource", "the returned resource identity or handoff is unresolved", "Return the live owned resource after local cleanup."),
            "TP2001" => ("Close through borrow", "A borrowed resource is closed through a non-owning view.", "Leave cleanup with the owning caller.", "The borrow and close operation resolve to the same modeled resource identity.", "Borrow escape and dynamic dispatch beyond the supported flow subset are not proven.", "borrow.read()", "borrow.close()", "the receiver's ownership identity is uncertain", "Remove the close through the borrow and return cleanup responsibility to its owner."),
            "TP3001" => ("Discarded must-use result", "A declared must-use result is discarded as an expression statement.", "Handle the value, assign it for immediate handling, or pass it to codepact.discard when abandonment is intentional.", "The synchronous callable resolves to a supported must-use declaration.", "Assignment is immediate handling only; later use or eventual consumption is not proven. Async and dynamic consumption are unqualified.", "value = fetch()", "fetch()", "the callee is dynamically selected or the result is nested in an unsupported expression", "Handle the result, or use codepact.discard(call()) when intentional discard is explicit."),
            "TP4001" => ("Python syntax error", "The source could not be parsed for its selected Python target.", "Correct the syntax or select the target that matches the source.", "The parser receives source bytes and a supported Python target.", "This diagnostic does not claim analysis of syntax the selected parser could not build.", "def f(): pass", "def f(: pass", "the selected Python target may not match the source grammar", "Correct the source syntax or change the declared Python target."),
            "TP4002" => ("Invalid declaration or policy", "A declaration or inline policy directive is invalid.", "Correct the exact declaration or directive shown in the report.", "A known CodePact declaration or policy form is recognized.", "Ordinary Python type checking is outside this diagnostic.", "# codepact: ignore[TP3001] -- intentional discard", "# codepact: ignore[TP9001] -- hide boundary", "an unknown third-party annotation may not be interpreted", "Correct the metadata or use an exact eligible suppression code and reason."),
            "TP9001" => ("Analysis boundary", "A construct or effect is outside the qualified analysis subset.", "Review the unsupported construct; a clean supported-subset result is not a general safety proof.", "The source is valid enough for the frontend to identify the construct.", "This is not a confirmed violation; boundary reporting never proves safety or execution behavior.", "simple local open/use/close", "dynamic dispatch treated as a proven close", "task ownership or framework behavior outside the model", "Resolve the boundary or record it as a limitation; do not change policy to call it repaired."),
            "TP9002" => ("Analysis cancelled", "The requested analysis did not complete.", "Retry the check; do not treat this result as a clean report.", "The analyzer request was cancelled before a completed report was produced.", "Cancellation of analyzed Python code is distinct from cancellation of the analyzer request.", "completed check", "partial report treated as complete", "the caller cancels while work remains", "Retry and verify a completed report with the current snapshot."),
            _ => return emit_failure("json", format!("unknown rule identifier {rule}")),
        };
        let id = format!("{rule}.R001");
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "explanation_schema_version":1,"explanation_id":id,"code":rule,
            "title":explanation.0,"summary":explanation.1,
            "preconditions":[explanation.3],"limitations":[explanation.4],
            "examples":{"valid":explanation.5,"invalid":explanation.6,"unknown":explanation.7},
            "repair_guidance":{"id":format!("{rule}.repair.R001"),"text":explanation.2,"suggested_action":explanation.8},
            "automatic_fix":false
        })).unwrap();
        if let Err(error) = write_stdout(&output) {
            eprintln!("codepact: {error}");
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

    let mut overrides = Overrides::default();
    let mut explicit_root = None;
    let mut paths = Vec::new();
    let mut output_format = if inspecting {
        "json".to_string()
    } else {
        "text".to_string()
    };
    let mut requested_output_format: Option<String> = None;
    let mut show_settings = false;
    let mut max_diagnostics = None;
    let mut output_baseline = None;
    let mut update_reason = None;
    let mut replace_baseline = false;
    let mut it = args;
    while let Some(arg) = it.next() {
        let mut take = |label: &str| it.next().ok_or_else(|| format!("{label} requires a value"));
        match arg.as_str() {
            "--python-version" => match take("--python-version") {
                Ok(v) => overrides.python_version = Some(v),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--root" => match take("--root") {
                Ok(v) => {
                    let p = PathBuf::from(v);
                    explicit_root = Some(p.clone());
                    overrides.root = Some(p);
                }
                Err(e) => return emit_failure(&output_format, e),
            },
            "--config" => match take("--config") {
                Ok(v) => overrides.config = Some(PathBuf::from(v)),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--no-config" => overrides.no_config = true,
            "--mode" => match take("--mode") {
                Ok(v) => overrides.mode = Some(v),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--fail-on" => match take("--fail-on") {
                Ok(v) => overrides.fail_on = Some(v),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--rules" => match take("--rules") {
                Ok(value) => {
                    let rules = value
                        .split(',')
                        .map(str::trim)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if rules.iter().any(|rule| rule.trim().is_empty()) {
                        return emit_failure(
                            &output_format,
                            "--rules requires comma-separated rule codes".into(),
                        );
                    }
                    overrides.rules = Some(rules);
                }
                Err(error) => return emit_failure(&output_format, error),
            },
            "--severity" => match take("--severity") {
                Ok(value) => {
                    let Some((rule, severity)) = value.split_once('=') else {
                        return emit_failure(
                            &output_format,
                            "--severity requires CODE=LEVEL".into(),
                        );
                    };
                    let mut values = overrides.severity.take().unwrap_or_default();
                    if values
                        .insert(rule.trim().to_owned(), severity.trim().to_owned())
                        .is_some()
                    {
                        return emit_failure(
                            &output_format,
                            format!("duplicate --severity override for {rule}"),
                        );
                    }
                    overrides.severity = Some(values);
                }
                Err(error) => return emit_failure(&output_format, error),
            },
            "--baseline" => match take("--baseline") {
                Ok(v) => overrides.baseline = Some(PathBuf::from(v)),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--output-format" => match take("--output-format") {
                Ok(v) => {
                    if requested_output_format
                        .as_ref()
                        .is_some_and(|previous| previous != &v)
                    {
                        return emit_failure(
                            "text",
                            "--json conflicts with --output-format".into(),
                        );
                    }
                    requested_output_format = Some(v.clone());
                    output_format = v;
                }
                Err(e) => return emit_failure(&output_format, e),
            },
            "--json" => {
                if requested_output_format
                    .as_deref()
                    .is_some_and(|previous| previous != "json")
                {
                    return emit_failure("text", "--json conflicts with --output-format".into());
                }
                requested_output_format = Some("json".into());
                output_format = "json".into();
            }
            "--show-settings" => show_settings = true,
            "--max-diagnostics" => match take("--max-diagnostics").and_then(|v| {
                v.parse::<usize>()
                    .map_err(|_| "--max-diagnostics must be a positive integer".into())
            }) {
                Ok(0) => {
                    return emit_failure(
                        &output_format,
                        "--max-diagnostics must be a positive integer".into(),
                    )
                }
                Ok(v) => max_diagnostics = Some(v),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--output" if baseline_update => match take("--output") {
                Ok(v) => output_baseline = Some(PathBuf::from(v)),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--reason" if baseline_update => match take("--reason") {
                Ok(v) => update_reason = Some(v),
                Err(e) => return emit_failure(&output_format, e),
            },
            "--replace" if baseline_update => replace_baseline = true,
            "--help" | "-h" => {
                usage();
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') => {
                return emit_failure(&output_format, format!("unknown option: {arg}"))
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if !matches!(output_format.as_str(), "text" | "json" | "concise") {
        return emit_failure("text", format!("unknown output format {output_format}"));
    }
    if baseline_update && (output_baseline.is_none() || update_reason.is_none()) {
        return emit_failure(
            &output_format,
            "baseline update requires --output and --reason".into(),
        );
    }
    if baseline_update
        && (requested_output_format
            .as_deref()
            .is_some_and(|format| !matches!(format, "text" | "json"))
            || max_diagnostics.is_some()
            || show_settings)
    {
        return emit_failure(
            &output_format,
            "baseline update supports text or JSON output and does not accept --max-diagnostics or --show-settings".into(),
        );
    }
    if show_settings && (inspecting || output_format != "json") {
        return emit_failure(
            "json",
            "--show-settings is available only on check with JSON output".into(),
        );
    }
    if show_settings && (!paths.is_empty() || max_diagnostics.is_some()) {
        return emit_failure(
            "json",
            "--show-settings does not accept file operands or --max-diagnostics".into(),
        );
    }
    if inspecting
        && (overrides.config.is_some()
            || overrides.no_config
            || overrides.mode.is_some()
            || overrides.fail_on.is_some()
            || overrides.rules.is_some()
            || overrides.baseline.is_some()
            || overrides.severity.is_some()
            || max_diagnostics.is_some()
            || show_settings)
    {
        return emit_failure(
            "json",
            "inspect-contracts accepts only --root, --python-version, --json, and file operands"
                .into(),
        );
    }
    if inspecting && output_format != "json" {
        return emit_failure("json", "inspect-contracts emits JSON only".into());
    }

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
            "fail_on": c.fail_on,
            "baseline": c.baseline,
            "severity": c.severity,
            "settings_origins": c.settings_origins
        }))
        .unwrap();
        if let Err(error) = write_stdout(&output) {
            eprintln!("codepact: {error}");
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
    let root = if let Some(c) = effective.as_ref() {
        match resolve_project_root(&c.root) {
            Ok(root) => Some(root),
            Err(error) => return emit_failure(&output_format, error),
        }
    } else if let Some(p) = explicit_root {
        let p = if p.is_absolute() { p } else { cwd.join(p) };
        if let Err(error) = adoption::reject_symlink_components(&p, "project root") {
            return emit_failure(&output_format, error);
        }
        match fs::canonicalize(&p) {
            Ok(p) => Some(p),
            Err(e) => {
                return emit_failure(
                    &output_format,
                    format!("cannot resolve module root {}: {e}", p.display()),
                )
            }
        }
    } else if inspecting {
        None
    } else {
        Some(cwd.clone())
    };
    let python_version = effective
        .as_ref()
        .map(|c| c.python_version.as_str())
        .or(overrides.python_version.as_deref())
        .unwrap_or("3.11")
        .to_string();

    if inspecting && paths.is_empty() {
        return emit_failure(
            "json",
            "inspect-contracts requires at least one file".into(),
        );
    }
    if checking {
        let project_root = root.as_deref().unwrap();
        let mut discovered = Vec::new();
        if let Err(e) = discover_python_files(
            project_root,
            &mut discovered,
            codepact::DEFAULT_MAX_INPUT_FILES,
            &|path| {
                effective
                    .as_ref()
                    .is_none_or(|config| adoption::included(config, project_root, path))
            },
        ) {
            return emit_failure(&output_format, e);
        }
        if paths.is_empty() && discovered.is_empty() {
            return emit_failure(
                &output_format,
                format!(
                    "check found no Python files selected under {}",
                    project_root.display()
                ),
            );
        }
        paths.extend(discovered);
        paths.sort();
        paths.dedup();
        if paths.len() > codepact::DEFAULT_MAX_INPUT_FILES {
            return emit_failure(
                &output_format,
                "project input exceeds the 10,000 file limit".into(),
            );
        }
    }

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut modules = std::collections::BTreeSet::new();
    let mut physical_files = std::collections::BTreeSet::new();
    for path in paths {
        let physical_path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        if checking {
            if let Err(error) = adoption::reject_symlink_components(&physical_path, "source input")
            {
                return emit_failure(&output_format, error);
            }
        }
        let canonical_path = match fs::canonicalize(&physical_path) {
            Ok(p) => p,
            Err(e) => {
                return emit_failure(
                    &output_format,
                    format!("cannot read {}: {e}", physical_path.display()),
                )
            }
        };
        if !physical_files.insert(canonical_path.clone()) {
            continue;
        }
        let logical_path = match &root {
            Some(root) => match canonical_path.strip_prefix(root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => {
                    return emit_failure(
                        &output_format,
                        format!(
                            "file {} is outside module root {}",
                            canonical_path.display(),
                            root.display()
                        ),
                    )
                }
            },
            None => match canonical_path.file_name() {
                Some(n) => PathBuf::from(n),
                None => {
                    return emit_failure(
                        &output_format,
                        "cannot derive module name from source path".into(),
                    )
                }
            },
        };
        if checking
            && effective
                .as_ref()
                .is_some_and(|c| !adoption::included(c, root.as_ref().unwrap(), &canonical_path))
        {
            return emit_failure(
                &output_format,
                format!(
                    "explicit input {} is excluded or skipped by project configuration",
                    logical_path.display()
                ),
            );
        }
        let module = match if root.is_some() {
            module_from_relative_path(&logical_path, &python_version)
        } else {
            logical_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| is_python_identifier(s, &python_version))
                .ok_or_else(|| format!("cannot derive module name from {}", logical_path.display()))
        } {
            Ok(m) => m,
            Err(e) => return emit_failure(&output_format, e),
        };
        if !modules.insert(module.clone()) {
            return emit_failure(
                &output_format,
                format!("multiple files map to module {module}; provide a suitable --root"),
            );
        }
        let metadata = match fs::metadata(&canonical_path) {
            Ok(m) => m,
            Err(e) => {
                return emit_failure(
                    &output_format,
                    format!("cannot inspect {}: {e}", canonical_path.display()),
                )
            }
        };
        let remaining = DEFAULT_MAX_SOURCE_BYTES.saturating_sub(total_bytes);
        if metadata.len() > remaining as u64 {
            return emit_failure(
                &output_format,
                format!(
                    "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded before reading {}",
                    canonical_path.display()
                ),
            );
        }
        let mut source = String::new();
        let mut input = match File::open(&canonical_path) {
            Ok(f) => f.take(remaining as u64 + 1),
            Err(e) => {
                return emit_failure(
                    &output_format,
                    format!("cannot read {}: {e}", canonical_path.display()),
                )
            }
        };
        if let Err(e) = input.read_to_string(&mut source) {
            return emit_failure(
                &output_format,
                format!(
                    "cannot read UTF-8 Python source {}: {e}",
                    canonical_path.display()
                ),
            );
        }
        if source.len() > remaining {
            return emit_failure(
                &output_format,
                format!(
                    "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded while reading {}",
                    canonical_path.display()
                ),
            );
        }
        total_bytes += source.len();
        files.push(SourceFile::new(
            logical_path.to_string_lossy().replace('\\', "/"),
            module,
            source,
        ));
    }
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
    let mut inspection = if checking {
        codepact::check_with_must_use(&files, &python_version)
    } else {
        inspect(&files, &python_version)
    };
    if checking {
        inspection.diagnostics.retain(|d| {
            !d.message
                .contains("module-level control flow and executable statements are unsupported")
        });
    }
    if inspecting {
        let value = serde_json::to_string_pretty(&inspection);
        match value {
            Ok(v) => {
                if let Err(error) = write_stdout(&v) {
                    eprintln!("codepact: {error}");
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
    let result = codepact::CheckResult::from_inspection(inspection, &python_version);
    let config: EffectiveConfig = effective.expect("checking resolves effective configuration");
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
                eprintln!("codepact: {error}");
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
                eprintln!("codepact: {error}");
                return ExitCode::from(2);
            }
            if summary.coverage_status != "complete-for-supported-subset" {
                eprintln!(
                    "baseline records incomplete coverage: {}",
                    summary.coverage_status
                );
            }
        }
        return ExitCode::SUCCESS;
    }
    let baseline = config.baseline.as_deref();
    let report = match adoption::build_report(result, &files, &config, baseline, max_diagnostics) {
        Ok(r) => r,
        Err(e) => return emit_failure(&output_format, e),
    };
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    match output_format.as_str() {
        "json" => {
            stdout_text = match adoption::serialize_report(&report) {
                Ok(value) => value,
                Err(error) => {
                    return emit_failure("json", format!("serialization failure: {error}"))
                }
            };
        }
        "concise" => {
            for diagnostic in report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.actionable)
            {
                let path = serde_json::to_string(&diagnostic.diagnostic.primary.file).unwrap();
                let message = serde_json::to_string(&diagnostic.diagnostic.message).unwrap();
                use std::fmt::Write as FmtWrite;
                let _ = writeln!(
                    &mut stdout_text,
                    "{path}\t{}\t{}\t{}\t{}\tprimary\t{message}",
                    diagnostic.diagnostic.primary.line,
                    diagnostic.diagnostic.primary.column,
                    diagnostic.effective_severity,
                    diagnostic.diagnostic.code
                );
                for related in &diagnostic.diagnostic.related {
                    let path = serde_json::to_string(&related.file).unwrap();
                    let related_message = serde_json::to_string(&format!(
                        "related evidence for {} at {}:{}:{}",
                        diagnostic.diagnostic.code,
                        diagnostic.diagnostic.primary.file,
                        diagnostic.diagnostic.primary.line,
                        diagnostic.diagnostic.primary.column
                    ))
                    .unwrap();
                    let _ = writeln!(
                        &mut stdout_text,
                        "{path}\t{}\t{}\t{}\t{}\trelated\t{related_message}",
                        related.line,
                        related.column,
                        diagnostic.effective_severity,
                        diagnostic.diagnostic.code
                    );
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
    if let Err(error) = write_stdout(&stdout_text) {
        eprintln!("codepact: {error}");
        return ExitCode::from(2);
    }
    if !stderr_text.is_empty() {
        eprint!("{stderr_text}");
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
}

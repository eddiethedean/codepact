use codepact::adoption::Overrides;
use std::ffi::OsString;
use std::path::PathBuf;

pub(super) struct ParsedArgs {
    pub(super) overrides: Overrides,
    pub(super) explicit_root: Option<PathBuf>,
    pub(super) paths: Vec<PathBuf>,
    pub(super) output_format: String,
    pub(super) max_diagnostics: Option<usize>,
    pub(super) output_baseline: Option<PathBuf>,
    pub(super) update_reason: Option<String>,
    pub(super) replace_baseline: bool,
    pub(super) show_settings: bool,
}

pub(super) struct ParseError {
    pub(super) format: String,
    pub(super) message: String,
}

pub(super) enum ParseOutcome {
    Parsed(Box<ParsedArgs>),
    Help,
}

fn option_takes_value(option: &str, baseline_update: bool) -> bool {
    matches!(
        option,
        "--python-version"
            | "--root"
            | "--config"
            | "--mode"
            | "--fail-on"
            | "--rules"
            | "--severity"
            | "--baseline"
            | "--output-format"
            | "--max-diagnostics"
    ) || (baseline_update && matches!(option, "--output" | "--reason"))
}

pub(super) fn invalid_argument_format(args: &[OsString]) -> &'static str {
    let mut args = args.iter().map(|arg| arg.to_str());
    let (baseline_update, mut format) = match args.next().flatten() {
        Some("capabilities" | "explain") => return "json",
        Some("inspect-contracts") => (false, "json"),
        Some("check") => (false, "text"),
        Some("baseline") if args.next().flatten() == Some("update") => (true, "text"),
        _ => return "text",
    };
    while let Some(option) = args.next() {
        let Some(option) = option else { continue };
        if option == "--json" {
            return "json";
        }
        if option_takes_value(option, baseline_update) {
            let value = args.next().flatten();
            if option == "--output-format" {
                match value {
                    Some("json") => return "json",
                    Some("text" | "concise") => format = "text",
                    _ => {}
                }
            }
        }
    }
    format
}

fn failure(format: &str, message: impl Into<String>) -> ParseError {
    ParseError {
        format: format.to_owned(),
        message: message.into(),
    }
}

pub(super) fn parse(
    arguments: impl IntoIterator<Item = String>,
    inspecting: bool,
    baseline_update: bool,
) -> Result<ParseOutcome, ParseError> {
    let mut overrides = Overrides::default();
    let mut explicit_root = None;
    let mut paths = Vec::new();
    let mut output_format = if inspecting { "json" } else { "text" }.to_owned();
    let mut requested_output_format: Option<String> = None;
    let mut show_settings = false;
    let mut max_diagnostics = None;
    let mut output_baseline = None;
    let mut update_reason = None;
    let mut replace_baseline = false;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        let value = option_takes_value(&argument, baseline_update)
            .then(|| arguments.next())
            .flatten();
        let take = || {
            value
                .clone()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--python-version" => match take() {
                Ok(value) => overrides.python_version = Some(value),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--root" => match take() {
                Ok(value) => {
                    let path = PathBuf::from(value);
                    explicit_root = Some(path.clone());
                    overrides.root = Some(path);
                }
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--config" => match take() {
                Ok(value) => overrides.config = Some(PathBuf::from(value)),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--no-config" => overrides.no_config = true,
            "--experimental-ownership" | "--no-experimental-ownership" => {
                if overrides.experimental_ownership.is_some() {
                    return Err(failure(
                        &output_format,
                        "specify at most one ownership override",
                    ));
                }
                overrides.experimental_ownership = Some(argument == "--experimental-ownership");
            }
            "--mode" => match take() {
                Ok(value) => overrides.mode = Some(value),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--fail-on" => match take() {
                Ok(value) => overrides.fail_on = Some(value),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--rules" => match take() {
                Ok(value) => {
                    let rules = value
                        .split(',')
                        .map(str::trim)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if rules.iter().any(|rule| rule.trim().is_empty()) {
                        return Err(failure(
                            &output_format,
                            "--rules requires comma-separated rule codes",
                        ));
                    }
                    overrides.rules = Some(rules);
                }
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--severity" => match take() {
                Ok(value) => {
                    let Some((rule, severity)) = value.split_once('=') else {
                        return Err(failure(&output_format, "--severity requires CODE=LEVEL"));
                    };
                    let mut values = overrides.severity.take().unwrap_or_default();
                    if values
                        .insert(rule.trim().to_owned(), severity.trim().to_owned())
                        .is_some()
                    {
                        return Err(failure(
                            &output_format,
                            format!("duplicate --severity override for {rule}"),
                        ));
                    }
                    overrides.severity = Some(values);
                }
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--baseline" => match take() {
                Ok(value) => overrides.baseline = Some(PathBuf::from(value)),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--output-format" => match take() {
                Ok(value) => {
                    if requested_output_format
                        .as_ref()
                        .is_some_and(|previous| previous != &value)
                    {
                        return Err(failure("text", "--json conflicts with --output-format"));
                    }
                    requested_output_format = Some(value.clone());
                    output_format = value;
                }
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--json" => {
                if requested_output_format
                    .as_deref()
                    .is_some_and(|previous| previous != "json")
                {
                    return Err(failure("text", "--json conflicts with --output-format"));
                }
                requested_output_format = Some("json".into());
                output_format = "json".into();
            }
            "--show-settings" => show_settings = true,
            "--max-diagnostics" => match take().and_then(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| "--max-diagnostics must be a positive integer".to_owned())
            }) {
                Ok(0) => {
                    return Err(failure(
                        &output_format,
                        "--max-diagnostics must be a positive integer",
                    ));
                }
                Ok(value) => max_diagnostics = Some(value),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--output" if baseline_update => match take() {
                Ok(value) => output_baseline = Some(PathBuf::from(value)),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--reason" if baseline_update => match take() {
                Ok(value) => update_reason = Some(value),
                Err(error) => return Err(failure(&output_format, error)),
            },
            "--replace" if baseline_update => replace_baseline = true,
            "--help" | "-h" => return Ok(ParseOutcome::Help),
            _ if argument.starts_with('-') => {
                return Err(failure(
                    &output_format,
                    format!("unknown option: {argument}"),
                ));
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }

    if !matches!(output_format.as_str(), "text" | "json" | "concise") {
        return Err(failure(
            "text",
            format!("unknown output format {output_format}"),
        ));
    }
    if baseline_update && (output_baseline.is_none() || update_reason.is_none()) {
        return Err(failure(
            &output_format,
            "baseline update requires --output and --reason",
        ));
    }
    if baseline_update
        && (requested_output_format
            .as_deref()
            .is_some_and(|format| !matches!(format, "text" | "json"))
            || max_diagnostics.is_some()
            || show_settings)
    {
        return Err(failure(
            &output_format,
            "baseline update supports text or JSON output and does not accept --max-diagnostics or --show-settings",
        ));
    }
    if show_settings && (inspecting || output_format != "json") {
        return Err(failure(
            "json",
            "--show-settings is available only on check with JSON output",
        ));
    }
    if show_settings && (!paths.is_empty() || max_diagnostics.is_some()) {
        return Err(failure(
            "json",
            "--show-settings does not accept file operands or --max-diagnostics",
        ));
    }
    if inspecting
        && (overrides.config.is_some()
            || overrides.no_config
            || overrides.mode.is_some()
            || overrides.experimental_ownership.is_some()
            || overrides.fail_on.is_some()
            || overrides.rules.is_some()
            || overrides.baseline.is_some()
            || overrides.severity.is_some()
            || max_diagnostics.is_some()
            || show_settings)
    {
        return Err(failure(
            "json",
            "inspect-contracts accepts only --root, --python-version, --json, and file operands",
        ));
    }
    if inspecting && output_format != "json" {
        return Err(failure("json", "inspect-contracts emits JSON only"));
    }

    Ok(ParseOutcome::Parsed(Box::new(ParsedArgs {
        overrides,
        explicit_root,
        paths,
        output_format,
        max_diagnostics,
        output_baseline,
        update_reason,
        replace_baseline,
        show_settings,
    })))
}

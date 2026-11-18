use super::{args, input, CommandOutput};
use codepact::{
    adoption,
    review::{self, ReviewSnapshot},
    AnalysisSession, CheckOptions,
};
use std::{
    collections::BTreeSet,
    fmt::Write,
    path::{Path, PathBuf},
};

fn capture(cwd: &Path, args: impl IntoIterator<Item = String>) -> Result<ReviewSnapshot, String> {
    let args::ParseOutcome::Parsed(options) =
        args::parse(args, false, false).map_err(|error| error.message)?
    else {
        return Err("usage: codepact review capture [check options]".into());
    };
    if options.show_settings {
        return Err("review capture requires analysis evidence, not --show-settings".into());
    }
    let config = adoption::resolve_config(cwd, &options.overrides)?;
    let loaded = input::load_project(options.paths, &config, cwd)?;
    review::capture(
        &mut AnalysisSession::default(),
        &loaded.files,
        &config,
        CheckOptions {
            ownership: config.ownership,
            experimental_finite_matches: config.finite_matches,
            ..CheckOptions::default()
        },
        loaded.foundations.as_ref(),
        options.max_diagnostics,
    )
}

fn read(path: &Path) -> Result<ReviewSnapshot, String> {
    let text = super::artifacts::read_text(path, review::MAX_REVIEW_BYTES)?;
    ReviewSnapshot::read(&text)
}

pub(super) fn failure_format(args: &[String]) -> &'static str {
    if args.first().is_some_and(|arg| arg == "capture")
        || args
            .iter()
            .take_while(|arg| arg.as_str() != "--")
            .any(|arg| arg == "--json")
    {
        "json"
    } else {
        "text"
    }
}

pub(super) fn run(args: Vec<String>, cwd: &Path) -> Result<CommandOutput, String> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_some_and(|argument| argument == "capture") {
        args.next();
        let snapshot = capture(cwd, args)?;
        return Ok(CommandOutput::stdout(
            review::serialize(&snapshot)?,
            snapshot.check_exit,
        ));
    }
    let mut files = Vec::new();
    let mut current = None;
    let mut repairs = BTreeSet::new();
    let mut json = false;
    let mut replay_options = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--current" if current.is_none() => {
                let root = next_value(&mut args, "--current")?;
                current = Some(cwd.join(root));
            }
            "--repair" => {
                let fingerprint = next_value(&mut args, "--repair")?;
                if fingerprint.len() != 64
                    || !fingerprint
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                {
                    return Err("--repair expects a lowercase SHA-256 finding fingerprint".into());
                }
                repairs.insert(fingerprint);
            }
            "--" if current.is_some() => {
                replay_options.extend(args);
                break;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown review option {arg}")),
            _ => files.push(PathBuf::from(arg)),
        }
    }
    let [before_path, after_path] = files.as_slice() else {
        return Err(
            "usage: codepact review BEFORE AFTER [--repair FINGERPRINT] [--current DIR] [--json]"
                .into(),
        );
    };
    let before = read(&cwd.join(before_path))?;
    let after = read(&cwd.join(after_path))?;
    let current = current
        .as_deref()
        .map(|root| capture(root, replay_options))
        .transpose()?;
    let report = review::compare(&before, &after, &repairs, current.as_ref())?;
    let output = if json {
        review::serialize(&report)?
    } else {
        let mut output = format!("review {} ({})", report.verdict.as_str(), report.claim);
        for reason in &report.reasons {
            let _ = write!(output, "\n{}: {}", reason.category, reason.detail);
        }
        for change in &report.findings {
            if let Some(finding) = change.after.as_ref().or(change.before.as_ref()) {
                let _ = write!(
                    output,
                    "\n{} {} {}:{} {}",
                    change.classification,
                    finding.code,
                    finding.primary.file,
                    finding.primary.line,
                    finding.message
                );
            }
        }
        let _ = write!(
            output,
            "\nstandalone after check exit {}",
            report.after_check_exit
        );
        if output.len() >= review::MAX_REVIEW_BYTES {
            return Err("review text exceeds the output byte limit".into());
        }
        output
    };
    Ok(CommandOutput::stdout(output, report.exit_code))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .filter(|argument| !argument.starts_with("--"))
        .ok_or_else(|| format!("{flag} requires a value"))
}

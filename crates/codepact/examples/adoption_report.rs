use std::{env, process::ExitCode};

use codepact::{
    adoption::{build_report, resolve_config, serialize_report, Overrides},
    AnalysisSession, SourceFile,
};

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let config = resolve_config(
        &cwd,
        &Overrides {
            no_config: true,
            ..Overrides::default()
        },
    )?;
    let files = [SourceFile::new(
        "settings.py",
        "settings",
        "from codepact import must_use\n\n@must_use\ndef load() -> int:\n    return 1\n\nload()\n",
    )];

    let mut session = AnalysisSession::default();
    let checked = session.check_with_ownership(&files, &config.python_version, config.ownership);
    let report = build_report(checked, &files, &config, config.baseline.as_deref(), None)?;
    let json = serialize_report(&report)?;
    println!("{json}");
    Ok(report.exit_code)
}

fn main() -> ExitCode {
    match run() {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("Could not produce a CodePact report: {error}");
            ExitCode::from(2)
        }
    }
}

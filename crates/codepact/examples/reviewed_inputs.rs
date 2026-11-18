use codepact::{check_with_foundations, foundation::FoundationInputs, CheckOptions, SourceFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = serde_json::from_str(
        r#"{
            "implementation": "cpython",
            "python-version": "3.13.16",
            "os": "linux",
            "architecture": "x86_64"
        }"#,
    )?;
    let inputs = FoundationInputs {
        runtime: Some(runtime),
        ..FoundationInputs::default()
    };
    let files = [SourceFile::new(
        "settings.py",
        "settings",
        "def read_settings():\n    with open('settings.txt') as file:\n        file.read()\n    file.read()\n",
    )];

    let result = check_with_foundations(&files, "3.13", CheckOptions::default(), inputs);
    assert_eq!(result.check_schema_version, 2);
    let evidence = result.foundations.as_ref().expect("foundation evidence");
    assert!(evidence
        .model_uses
        .iter()
        .any(|site| site.symbol == "builtins.open" && site.applicable));
    assert!(result
        .diagnostics
        .iter()
        .any(|finding| finding.code == "TP1001"));
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

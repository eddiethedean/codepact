use codepact::{
    adoption::{resolve_config, Overrides},
    foundation::{
        capture_publication, diff_publications, export_manifest, ContractChangeKind,
        ProviderPackage,
    },
    review::{capture, compare, ReviewSnapshot, ReviewVerdict},
    AnalysisSession, CheckOptions, SourceFile,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const CLEANUP_SOURCE: &str = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n";
const FIX: &str = "    resource.close()\n";

fn analyze(session: &mut AnalysisSession, source: &str) -> ReviewSnapshot {
    let path = std::env::current_dir().unwrap();
    let config = resolve_config(
        &path,
        &Overrides {
            no_config: true,
            ..Overrides::default()
        },
    )
    .unwrap();
    capture(
        session,
        &[SourceFile::new("main.py", "main", source)],
        &config,
        CheckOptions::default(),
        None,
        None,
    )
    .unwrap()
}

#[test]
fn review_retains_inherited_findings_and_proves_same_operation_cleanup() {
    let mut reused = AnalysisSession::default();
    let before = analyze(&mut reused, CLEANUP_SOURCE);
    let shifted = analyze(&mut reused, &format!("\n{CLEANUP_SOURCE}"));
    assert_eq!(before.check_exit, 1);
    let regression_review = compare(&before, &shifted, &BTreeSet::new(), None).unwrap();
    assert_eq!(
        regression_review.verdict,
        ReviewVerdict::Passed,
        "{regression_review:?}"
    );
    assert_eq!(regression_review.findings[0].classification, "inherited");
    assert_eq!(regression_review.after_check_exit, 1);

    let fixed = format!("{CLEANUP_SOURCE}{FIX}");
    let after = analyze(&mut reused, &fixed);
    let fresh = analyze(&mut AnalysisSession::default(), &fixed);
    assert_eq!(fresh, after);
    assert!(after.operations.iter().any(|operation| operation.closed));
    let repair = compare(
        &before,
        &after,
        &BTreeSet::from([before.findings[0].fingerprint.clone()]),
        Some(&fresh),
    )
    .unwrap();
    assert_eq!(repair.verdict, ReviewVerdict::Passed, "{repair:?}");
    assert_eq!(repair.claim, "current-workspace");
    assert_eq!(
        repair.findings[0].classification,
        "resolved-under-preserved-contracts"
    );
    let stale = compare(&before, &after, &BTreeSet::new(), Some(&before)).unwrap();
    assert_eq!(stale.verdict, ReviewVerdict::Inconclusive);
    let new_leak = compare(&after, &before, &BTreeSet::new(), None).unwrap();
    assert_eq!(new_leak.verdict, ReviewVerdict::Failed);
    assert_eq!(new_leak.findings[0].classification, "new");

    let removed = analyze(
        &mut reused,
        &CLEANUP_SOURCE.replace("    resource = C()", "    pass"),
    );
    let result = compare(
        &before,
        &removed,
        &BTreeSet::from([before.findings[0].fingerprint.clone()]),
        None,
    )
    .unwrap();
    assert_eq!(result.verdict, ReviewVerdict::Inconclusive);
}

#[test]
fn review_requires_recapture_of_legacy_contract_keys() {
    let mut snapshot = analyze(&mut AnalysisSession::default(), CLEANUP_SOURCE);
    snapshot.descriptor.comparison_algorithm = 1;
    let report = compare(&snapshot, &snapshot, &BTreeSet::new(), None).unwrap();
    assert_eq!(report.verdict, ReviewVerdict::Inconclusive);
    assert!(report.changed_sections.contains("semantics"));
}

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codepact-phase09-review-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_codepact"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_review_detects_access_contracts_swapped_between_modules() {
    let sandbox = Sandbox::new();
    let write = |path: &str, source| fs::write(sandbox.0.join(path), source).unwrap();
    let write_api = |path, access| {
        write(
            path,
            format!("from codepact.rust import {access}\nfrom main import C\ndef use(value: {access}[C]): pass\n")
                .into_bytes(),
        );
    };
    write("main.py", CLEANUP_SOURCE.as_bytes().to_vec());
    write_api("alpha.py", "Borrowed");
    write_api("beta.py", "BorrowedMut");
    let before = sandbox.run(&["review", "capture", "--no-config"]);
    assert_eq!(before.status.code(), Some(1));
    let before_snapshot =
        ReviewSnapshot::read(std::str::from_utf8(&before.stdout).unwrap()).unwrap();
    write("before.json", before.stdout);

    write("main.py", format!("\n{CLEANUP_SOURCE}{FIX}").into_bytes());
    let preserved = sandbox.run(&["review", "capture", "--no-config"]);
    assert_eq!(preserved.status.code(), Some(0));
    let preserved = ReviewSnapshot::read(std::str::from_utf8(&preserved.stdout).unwrap()).unwrap();
    assert_eq!(before_snapshot.contracts, preserved.contracts);

    write_api("alpha.py", "BorrowedMut");
    write_api("beta.py", "Borrowed");
    let after = sandbox.run(&["review", "capture", "--no-config"]);
    assert_eq!(after.status.code(), Some(0));
    write("after.json", after.stdout);
    let compared = sandbox.run(&["review", "before.json", "after.json", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&compared.stdout).unwrap();
    assert_eq!(compared.status.code(), Some(1), "{report}");
    assert_eq!(report["verdict"], "review-required", "{report}");
    assert_eq!(report["added_contracts"].as_array().unwrap().len(), 2);
    assert_eq!(report["removed_contracts"].as_array().unwrap().len(), 2);

    let compared = sandbox.run(&[
        "review",
        "before.json",
        "after.json",
        "--json",
        "--repair",
        &before_snapshot.findings[0].fingerprint,
        "--current",
        ".",
        "--",
        "--no-config",
    ]);
    let report: serde_json::Value = serde_json::from_slice(&compared.stdout).unwrap();
    assert_eq!(compared.status.code(), Some(1), "{report}");
    assert_eq!(report["verdict"], "inconclusive", "{report}");
    assert_ne!(
        report["findings"][0]["classification"],
        "resolved-under-preserved-contracts"
    );
}

#[test]
fn cli_review_distinguishes_all_completed_verdicts_and_input_failure() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.0.join("main.py"), "x = 1\n").unwrap();
    let before = sandbox.run(&["review", "capture", "--no-config"]);
    assert_eq!(before.status.code(), Some(0));
    ReviewSnapshot::read(std::str::from_utf8(&before.stdout).unwrap()).unwrap();
    fs::write(sandbox.0.join("before.json"), before.stdout).unwrap();
    let after = sandbox.run(&["review", "capture", "--no-config", "--fail-on", "warning"]);
    assert_eq!(after.status.code(), Some(0));
    fs::write(sandbox.0.join("after.json"), after.stdout).unwrap();
    for (extra, verdict, status) in [
        (&["before.json", "before.json", "--json"][..], "passed", 0),
        (
            &["before.json", "after.json", "--json"],
            "review-required",
            1,
        ),
    ] {
        let argv = std::iter::once("review")
            .chain(extra.iter().copied())
            .collect::<Vec<_>>();
        let captured = sandbox.run(&argv);
        assert_eq!(captured.status.code(), Some(status), "{captured:?}");
        let value: serde_json::Value = serde_json::from_slice(&captured.stdout).unwrap();
        assert_eq!(value["verdict"], verdict);
    }
    let pass_text = sandbox.run(&["review", "before.json", "before.json"]);
    assert_eq!(pass_text.status.code(), Some(0));
    assert!(String::from_utf8(pass_text.stdout)
        .unwrap()
        .starts_with("review passed"));
    let text = sandbox.run(&["review", "before.json", "after.json"]);
    assert_eq!(text.status.code(), Some(1));
    assert!(String::from_utf8(text.stdout)
        .unwrap()
        .starts_with("review review-required"));

    let current = sandbox.run(&[
        "review",
        "before.json",
        "before.json",
        "--json",
        "--current",
        ".",
        "--",
        "--no-config",
    ]);
    assert_eq!(current.status.code(), Some(0), "{current:?}");
    fs::write(sandbox.0.join("main.py"), "x = 2\n").unwrap();
    let stale = sandbox.run(&[
        "review",
        "before.json",
        "before.json",
        "--json",
        "--current",
        ".",
        "--",
        "--no-config",
    ]);
    assert_eq!(stale.status.code(), Some(1), "{stale:?}");
    let report: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(report["verdict"], "inconclusive");
    let stale_text = sandbox.run(&[
        "review",
        "before.json",
        "before.json",
        "--current",
        ".",
        "--",
        "--no-config",
    ]);
    assert_eq!(stale_text.status.code(), Some(1));
    assert!(String::from_utf8(stale_text.stdout)
        .unwrap()
        .starts_with("review inconclusive"));

    fs::write(sandbox.0.join("main.py"), CLEANUP_SOURCE).unwrap();
    let regression = sandbox.run(&["review", "capture", "--no-config"]);
    assert_eq!(regression.status.code(), Some(1), "{regression:?}");
    fs::write(sandbox.0.join("regression.json"), regression.stdout).unwrap();
    let failed = sandbox.run(&["review", "before.json", "regression.json", "--json"]);
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    let report: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(report["verdict"], "failed");
    let failed_text = sandbox.run(&["review", "before.json", "regression.json"]);
    assert_eq!(failed_text.status.code(), Some(1));
    assert!(String::from_utf8(failed_text.stdout)
        .unwrap()
        .starts_with("review failed"));

    fs::write(sandbox.0.join("broken.json"), "{}").unwrap();
    let invalid = sandbox.run(&["review", "before.json", "broken.json", "--json"]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    let report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(report["execution_status"], "failed");
}

fn package(version: &str, source: &str) -> ProviderPackage {
    let mut package = ProviderPackage {
        distribution: "reviewed".into(),
        version: version.into(),
        import_scope: "reviewed".into(),
        sources: vec![SourceFile::new("reviewed/__init__.py", "reviewed", source)],
        marked: true,
        manifest: None,
    };
    let manifest = export_manifest(&package, "3.13").unwrap();
    package.manifest = Some(serde_json::to_string(&manifest).unwrap());
    package
}

fn wheel(root: &Path, package: &ProviderPackage) -> PathBuf {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    let dist = format!("reviewed-{}.dist-info", package.version);
    let record = format!("{dist}/RECORD");
    let mut members = package
        .sources
        .iter()
        .map(|file| (file.path.clone(), file.source.clone()))
        .collect::<Vec<_>>();
    members.extend([
        (package.marker_path(), String::new()),
        (package.manifest_path(), package.manifest.clone().unwrap()),
        (
            format!("{dist}/WHEEL"),
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n".into(),
        ),
        (
            format!("{dist}/METADATA"),
            format!(
                "Metadata-Version: 2.1\nName: reviewed\nVersion: {}\n",
                package.version
            ),
        ),
    ]);
    let mut records = members
        .iter()
        .map(|(name, data)| {
            let digest = URL_SAFE_NO_PAD.encode(Sha256::digest(data));
            format!("{name},sha256={digest},{}\n", data.len())
        })
        .collect::<String>();
    records.push_str(&format!("{record},,\n"));
    members.push((record, records));
    let filename = root.join(format!("reviewed-{}-py3-none-any.whl", package.version));
    let mut output = ZipWriter::new(fs::File::create(&filename).unwrap());
    for (name, data) in members {
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(data.as_bytes()).unwrap();
    }
    output.finish().unwrap();
    filename
}

#[test]
fn wheel_comparison_qualifies_caller_changes_and_evidence_boundaries() {
    let before = package("1.0", "def receipt(): return 1\n");
    let compatible = package("1.1", "\ndef receipt(): return 2\n");
    let marked = "from codepact import must_use\n@must_use\ndef receipt(): return 1\n";
    let breaking = package("1.2", marked);
    let mut unknown = package("1.3", marked);
    let mut manifest =
        codepact::foundation::decode_manifest(unknown.manifest.as_deref().unwrap()).unwrap();
    manifest.symbols[0].contracts[0]
        .required_capabilities
        .push("future-evidence".into());
    unknown.manifest = Some(serde_json::to_string(&manifest).unwrap());
    let sandbox = Sandbox::new();
    let before_path = wheel(&sandbox.0, &before);
    let before_api = capture_publication(&before, "3.13").unwrap();
    for (candidate, kind) in [
        (compatible, None),
        (breaking, Some(ContractChangeKind::NewCallerObligation)),
        (unknown, Some(ContractChangeKind::InsufficientEvidence)),
    ] {
        let path = wheel(&sandbox.0, &candidate);
        let candidate_api = capture_publication(&candidate, "3.13").unwrap();
        let verdict = diff_publications(&before_api, &candidate_api);
        assert_eq!(verdict.compatible, kind.is_none(), "{verdict:?}");
        assert_eq!(verdict.changes.first().map(|change| change.kind), kind);
        let args = [
            "contracts",
            "diff",
            before_path.to_str().unwrap(),
            path.to_str().unwrap(),
            "--json",
        ];
        let output = sandbox.run(&args);
        assert_eq!(
            output.status.code(),
            Some(i32::from(kind.is_some())),
            "{output:?}"
        );
        assert_eq!(output.stdout, sandbox.run(&args).stdout);
        let actual: codepact::foundation::ContractDiff =
            serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(verdict, actual);
    }

    let provider = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n";
    let old = package("2.0", provider);
    let after = package("2.1", "class C:\n    def close(self): pass\n");
    let result = diff_publications(
        &capture_publication(&old, "3.13").unwrap(),
        &capture_publication(&after, "3.13").unwrap(),
    );
    assert!(result
        .changes
        .iter()
        .any(|change| change.kind == ContractChangeKind::WeakenedGuarantee));
}

#[test]
fn outcome_review_retains_waivers_and_unresolved_helpers() {
    const PRODUCER: &str = "from codepact import Err, MustHandle, Ok, Result, discard_outcome\ndef produce() -> MustHandle[Result[int, str]]:\n    return Ok(1)\n";
    let mut session = AnalysisSession::default();
    let before = analyze(
        &mut session,
        &format!("{PRODUCER}\ndef client(callback):\n    result = produce()\n"),
    );
    let targets = BTreeSet::from([before
        .findings
        .iter()
        .find(|finding| finding.code == "TP5002")
        .expect("persistent duty is pending")
        .fingerprint
        .clone()]);
    let after = analyze(
        &mut session,
        &format!(
            "{PRODUCER}\ndef client(callback):\n    result = produce()\n    match result:\n        case Ok(_) | Err(_): pass\n"
        ),
    );
    let changed = compare(&before, &after, &targets, None).unwrap();
    assert_eq!(changed.verdict, ReviewVerdict::Passed, "{changed:?}");
    assert!(changed
        .findings
        .iter()
        .any(|finding| finding.classification == "resolved-under-preserved-contracts"));

    let unresolved = analyze(
        &mut session,
        &format!(
            "{PRODUCER}\ndef client(callback):\n    result = produce()\n    callback(result)\n"
        ),
    );
    let unknown = compare(&before, &unresolved, &targets, None).unwrap();
    assert_eq!(unknown.verdict, ReviewVerdict::Inconclusive, "{unknown:?}");
    assert!(unknown.changed_sections.contains("coverage"));

    let waived = analyze(
        &mut session,
        &format!("{PRODUCER}\ndef client(callback):\n    result = produce()\n    discard_outcome(result, reason='demonstrated fallback')\n"),
    );
    let review = compare(&before, &waived, &BTreeSet::new(), None).unwrap();
    assert_eq!(review.verdict, ReviewVerdict::ReviewRequired, "{review:?}");
    assert!(review.changed_sections.contains("waivers"));
}

#[test]
fn source_identical_provider_change_refreshes_reused_review_evidence() {
    let before_provider = package("3.0", "def receipt(): return 1\n");
    let after_provider = package(
        "3.1",
        "from codepact import must_use\n@must_use\ndef receipt(): return 1\n",
    );
    let before_inputs = codepact::foundation::FoundationInputs {
        providers: vec![before_provider],
        ..Default::default()
    };
    let after_inputs = codepact::foundation::FoundationInputs {
        providers: vec![after_provider],
        ..Default::default()
    };
    let config = resolve_config(
        &std::env::current_dir().unwrap(),
        &Overrides {
            no_config: true,
            python_version: Some("3.13".into()),
            fail_on: Some("warning".into()),
            ..Overrides::default()
        },
    )
    .unwrap();
    let source = [SourceFile::new(
        "caller.py",
        "caller",
        "from reviewed import receipt\nreceipt()\n",
    )];
    let run = |session: &mut AnalysisSession, inputs| {
        capture(
            session,
            &source,
            &config,
            CheckOptions::default(),
            Some(inputs),
            None,
        )
        .unwrap()
    };
    let mut reused = AnalysisSession::default();
    let before = run(&mut reused, &before_inputs);
    let after = run(&mut reused, &after_inputs);
    let fresh = run(&mut AnalysisSession::default(), &after_inputs);
    assert_eq!(after, fresh);
    assert_eq!(before.selected_files, after.selected_files);
    assert!(before
        .findings
        .iter()
        .all(|finding| finding.code != "TP3001"));
    assert!(after
        .findings
        .iter()
        .any(|finding| finding.code == "TP3001"));
    let result = compare(&before, &after, &BTreeSet::new(), Some(&fresh)).unwrap();
    assert_ne!(result.verdict, ReviewVerdict::Passed);
    assert!(result.changed_sections.contains("assumptions"));
    assert!(result.changed_sections.contains("contracts"));
    assert!(result
        .added_contracts
        .iter()
        .any(|fact| fact.contains("must-use")));
}

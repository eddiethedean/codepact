use codepact::foundation::{export_manifest, FoundationInputs, ProviderPackage};
use codepact::{
    check_with_foundations, check_with_options, CheckDiagnostic, CheckOptions, SourceFile,
};

const API: &str = r#"from codepact import Err, MustHandle, Ok, Result, discard_outcome, must_use
@must_use
def make() -> MustHandle[Result[int, ValueError]]:
    return Ok(1)
"#;

fn diagnostics(body: &str) -> Vec<CheckDiagnostic> {
    let source = format!("{API}{body}");
    check_with_options(
        &[SourceFile::new("example.py", "example", &source)],
        "3.13",
        CheckOptions::default(),
    )
    .diagnostics
    .into_iter()
    .filter(|d| d.code.starts_with("TP500") || d.code == "TP3001")
    .collect()
}

#[test]
fn aliases_and_equal_deliveries_retain_independent_outcome_identities() {
    let findings = diagnostics(
        r#"
def drop():
    first = make()
    alias = first
    second = make()
    del first
    discard_outcome(alias, reason="duplicate response")
    del second
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5005", "TP5002"],
        "{findings:#?}"
    );
    assert!(findings[0].message.contains("duplicate response"));
    assert_eq!(findings[1].certainty, "definite");
}

#[test]
fn acknowledgment_requires_an_entered_pattern_and_narrows_wildcard() {
    let accepted = diagnostics(
        r#"
def handle(flag):
    outcome = make()
    match outcome:
        case Ok(_) if False: pass
        case Err(_): pass
        case _: pass
"#,
    );
    assert!(accepted.is_empty(), "{accepted:#?}");
    let rejected = diagnostics(
        r#"
def handle():
    outcome = make()
    match outcome:
        case Ok(_) if False: pass
        case _: pass
"#,
    );
    assert_eq!(
        rejected.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5002"],
        "{rejected:#?}"
    );
}

#[test]
fn outcome_matching_keeps_paths_without_a_tracked_outcome() {
    let source = format!(
        r#"{API}
from codepact import resource
@resource(closed_by='close')
class Handle:
    def close(self): pass

def use(flag):
    if flag:
        value = make()
    else:
        value = None
    alias = value
    match alias:
        case Ok() | Err():
            return
    handle = Handle()
"#
    );
    let run = |source: &str| {
        check_with_options(
            &[SourceFile::new("example.py", "example", source)],
            "3.13",
            CheckOptions::default(),
        )
    };
    let report = run(&source);
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP1002"),
        "{report:#?}"
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "TP5002"),
        "{report:#?}"
    );

    // A direct match is still exhaustive when both paths carry marked outcomes,
    // even when each path produced a distinct identity.
    let report = run(&source.replace("value = None", "value = make()"));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| matches!(finding.code.as_str(), "TP1002" | "TP5002")),
        "{report:#?}"
    );
}

#[test]
fn helper_effects_and_qualified_returns_follow_actual_finally_exit() {
    let findings = diagnostics(
        r#"
def acknowledge(value):
    match value:
        case Ok(_) | Err(_): pass

def forward() -> MustHandle[Result[int, ValueError]]:
    outcome = make()
    try:
        return outcome
    finally:
        acknowledge(outcome)

def good():
    value = make()
    acknowledge(value)

def bad():
    value = forward()
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5002"],
        "{findings:#?}"
    );
}

#[test]
fn carrier_escape_remains_visible_without_definite_abandonment() {
    let source = format!(
        r#"{API}
def inspect(helper):
    outcome = make()
    helper(outcome)
    del outcome
"#
    );
    let inspection = check_with_options(
        &[SourceFile::new("example.py", "example", &source)],
        "3.13",
        CheckOptions::default(),
    );
    assert!(
        inspection
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("unresolved helper or carrier")),
        "{inspection:#?}"
    );
    assert!(!inspection.diagnostics.iter().any(|d| d.code == "TP5002"));
}

#[test]
fn published_manifest_and_inline_obligations_apply_to_independent_caller() {
    let mut package = ProviderPackage {
        distribution: "acme-outcomes".into(),
        version: "1.0".into(),
        import_scope: "vendor".into(),
        sources: vec![SourceFile::new("vendor/__init__.py", "vendor", API)],
        marked: true,
        manifest: None,
    };
    let caller = SourceFile::new(
        "caller.py",
        "caller",
        "from vendor import make\ndef caller():\n    value = make()\n",
    );
    let compare = |package| {
        check_with_foundations(
            std::slice::from_ref(&caller),
            "3.13",
            CheckOptions::default(),
            FoundationInputs {
                providers: vec![package],
                ..FoundationInputs::default()
            },
        )
    };
    let inline = compare(package.clone());
    assert!(
        inline.diagnostics.iter().any(|d| d.code == "TP5002"),
        "{inline:#?}"
    );
    let exported = export_manifest(&package, "3.13").unwrap();
    assert!(
        exported.symbols.iter().any(|symbol| symbol
            .contracts
            .iter()
            .any(|contract| contract.kind == "must-handle-return")),
        "{exported:#?}"
    );
    package.manifest = Some(serde_json::to_string(&exported).unwrap());
    let published = compare(package);
    assert!(
        published.diagnostics.iter().any(|d| d.code == "TP5002"),
        "{published:#?}"
    );
}

#[test]
fn failure_before_delivery_leaves_the_previous_binding_in_its_handler() {
    let findings = diagnostics(
        r#"
def fail() -> MustHandle[Result[int, ValueError]]:
    raise ValueError('broken provider')

def caller():
    value = make()
    try:
        value = fail()
    except ValueError:
        discard_outcome(value, reason='using cached result')
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5005"],
        "{findings:#?}"
    );
}

#[test]
fn direct_await_delivers_and_unawaited_coroutine_does_not() {
    let findings = diagnostics(
        r#"
async def make_async() -> MustHandle[Result[int, ValueError]]:
    return Ok(7)

async def awaited():
    value = await make_async()

async def created():
    pending = make_async()
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5002"],
        "{findings:#?}"
    );
    assert_eq!(findings[0].primary.line, 9);
}

#[test]
fn unmarked_new_acquisition_escape_differs_from_returning_an_argument_alias() {
    let findings = diagnostics(
        r#"
def escape() -> Result[int, ValueError]:
    return make()

def alias(value):
    return value

def caller():
    value = make()
    again = alias(value)
    match again:
        case Ok(_) | Err(_): pass
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5004"],
        "{findings:#?}"
    );
}

#[test]
fn invalid_positions_and_foreign_outcomes_do_not_create_a_duty() {
    let findings = diagnostics(
        r#"
def invalid_parameter(value: MustHandle[Result[int, ValueError]]): pass

def invalid_nested() -> list[MustHandle[Result[int, ValueError]]]: pass

class Foreign:
    pass

def invalid_base() -> MustHandle[Foreign]:
    return Foreign()

def unmarked():
    value = Ok(1)
"#,
    );
    assert_eq!(
        findings.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["TP5003", "TP5003", "TP5003"],
        "{findings:#?}"
    );
}

#[test]
fn singleton_wildcard_records_nonblocking_review_evidence() {
    use codepact::adoption::{resolve_config, Overrides};
    use codepact::review::{capture, compare, ReviewVerdict};
    use codepact::AnalysisSession;

    let source = format!(
        "{API}\n{}",
        r#"def caller():
    value = make()
    match value:
        case Err(_): pass
        case _: pass
"#
    );
    let file = SourceFile::new("example.py", "example", &source);
    let config = resolve_config(
        &std::env::current_dir().unwrap(),
        &Overrides {
            no_config: true,
            mode: Some("strict".into()),
            ..Overrides::default()
        },
    )
    .unwrap();
    let snapshot = capture(
        &mut AnalysisSession::default(),
        std::slice::from_ref(&file),
        &config,
        CheckOptions::default(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(snapshot.check_exit, 0, "{snapshot:#?}");
    let evidence = snapshot
        .findings
        .iter()
        .find(|finding| finding.code == "TP6003")
        .expect("the wildcard selected the sole remaining `Ok` variant");
    assert_eq!(evidence.severity, "info");
    assert!(!evidence.actionable);
    assert!(evidence.message.contains("only remaining outcome variant"));
    assert!(evidence.message.contains("Ok"));
    let check = check_with_options(&[file], "3.13", CheckOptions::default());
    let hint = check
        .diagnostics
        .iter()
        .find(|finding| finding.code == "TP6003")
        .unwrap();
    assert_eq!(hint.primary.line, 10);
    assert_eq!(hint.related[0].line, 7);

    let multiple = source.replace("        case Err(_): pass\n", "");
    let before = capture(
        &mut AnalysisSession::default(),
        &[SourceFile::new("example.py", "example", multiple)],
        &config,
        CheckOptions::default(),
        None,
        None,
    )
    .unwrap();
    let pending = before
        .findings
        .iter()
        .find(|finding| finding.code == "TP5002")
        .expect("the multi-variant wildcard leaves the duty pending");
    assert!(!before
        .findings
        .iter()
        .any(|finding| finding.code == "TP6003"));
    let repair = compare(
        &before,
        &snapshot,
        &std::collections::BTreeSet::from([pending.fingerprint.clone()]),
        None,
    )
    .unwrap();
    assert_eq!(repair.verdict, ReviewVerdict::Passed, "{repair:#?}");
}

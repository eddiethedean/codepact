use typepact::{check, AnalysisSession, Inspection, SourceFile};

const RESOURCE: &str = "from typepact import resource\nfrom typepact.rust import Borrowed, Owned\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\n";

fn check_source(source: &str) -> Inspection {
    check(&[SourceFile::new("phase03.py", "phase03", source)], "3.11")
}

fn check_source_version(source: &str, version: &str) -> Inspection {
    check(&[SourceFile::new("phase03.py", "phase03", source)], version)
}

#[test]
fn v03_01_local_acquisition_creates_one_exit_duty() {
    let result = check_source(&format!("{RESOURCE}def leak():\n    resource = C()\n"));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
    assert!(leaks[0].message.ends_with("on fallthrough"));
    assert_eq!(leaks[0].related.len(), 1, "{result:#?}");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_01_failed_initialization_creates_no_object_duty() {
    let source = "from typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def __init__(self):\n        raise RuntimeError\n    def close(self): pass\ndef fail():\n    resource = C()\n";
    let result = check_source(source);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert!(result.coverage.status.starts_with("incomplete"));
}

#[test]
fn v03_01_constructor_argument_failure_creates_no_object_duty() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def __init__(self, marker): pass\n    def close(self): pass\ndef fail() -> int:\n    raise RuntimeError()\ndef cleanup():\n    resource = C(fail())\n";
    let result = check_source(source);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "argument evaluation raises before the resource is acquired: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "a resolved argument failure is fully modeled without acquiring an object: {result:#?}"
    );
}

#[test]
fn v03_01_failed_later_initialization_preserves_earlier_duty_on_raise() {
    let source = format!(
        "{RESOURCE}@resource(closed_by=\"close\")\nclass Broken:\n    def __init__(self):\n        raise RuntimeError\n    def close(self): pass\ndef fail():\n    first = C()\n    second = Broken()\n"
    );
    let first_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "first = C()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on raise"), "{result:#?}");
    assert_eq!(
        leaks[0].related[0].line, first_acquisition_line,
        "{result:#?}"
    );
    assert!(result.coverage.status.starts_with("incomplete"));
}

#[test]
fn v03_01_custom_new_boundary_preserves_the_prior_duty_as_possible() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass Stable:\n    def close(self): pass\n@resource(closed_by='close')\nclass Broken:\n    def __new__(cls):\n        raise RuntimeError()\n    def close(self): pass\ndef fail():\n    first = Stable()\n    second = Broken()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 11, "{result:#?}");
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TP9001"
            && diagnostic
                .message
                .contains("custom __new__ allocation effects")
            && diagnostic.primary.line == 12
    }));
    assert!(result.coverage.status.starts_with("incomplete"));
}

#[test]
fn v03_07_owned_return_identity_survives_aliasing_and_finally() {
    let result = check_source(&format!(
        "{RESOURCE}def make() -> Owned[C]:\n    resource = C()\n    alias = resource\n    try:\n        return alias\n    finally:\n        alias = None\ndef forward() -> Owned[C]:\n    return make()\ndef caller() -> Owned[C]:\n    return forward()\n"
    ));
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_07_direct_owned_factory_return_hands_the_same_duty_to_its_caller() {
    let result = check_source(&format!(
        "{RESOURCE}def make() -> Owned[C]:\n    return C()\ndef caller() -> Owned[C]:\n    return make()\n"
    ));
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_07_return_expression_failure_keeps_the_owned_duty_on_raise() {
    let result = check_source(&format!(
        "{RESOURCE}def fail(resource: Owned[C]) -> Owned[C]:\n    raise RuntimeError()\ndef make() -> Owned[C]:\n    resource = C()\n    return fail(resource)\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert!(leaks[0].message.ends_with("on raise"), "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
}

#[test]
fn v03_07_finally_replacement_only_hands_off_the_final_owned_result() {
    let result = check_source(&format!(
        "{RESOURCE}def make() -> Owned[C]:\n    abandoned = C()\n    forwarded = C()\n    try:\n        return abandoned\n    finally:\n        return forwarded\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(
        leaks.len(),
        1,
        "the replaced result must remain open: {result:#?}"
    );
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on return"), "{result:#?}");
    assert_eq!(
        leaks[0].primary.line,
        RESOURCE.lines().count() + 7,
        "the surviving return is the actual function exit: {result:#?}"
    );
    assert_eq!(
        leaks[0].related[0].line,
        RESOURCE.lines().count() + 2,
        "the finding belongs to the abandoned acquisition: {result:#?}"
    );
    assert!(!result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "TP1003"));
}

#[test]
fn v03_07_branching_finally_replacement_keeps_abandoned_and_raised_duties() {
    let source = format!(
        "{RESOURCE}def make(flag) -> Owned[C]:\n    abandoned = C()\n    replacement = C()\n    try:\n        return abandoned\n    finally:\n        if flag:\n            return replacement\n        else:\n            raise RuntimeError()\n"
    );
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 3, "{report:#?}");
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.certainty == "definite"));
    let completions = leaks.iter().fold(
        std::collections::BTreeMap::<&str, usize>::new(),
        |mut counts, diagnostic| {
            *counts
                .entry(diagnostic.completion.as_deref().unwrap_or_default())
                .or_default() += 1;
            counts
        },
    );
    assert_eq!(
        completions,
        std::collections::BTreeMap::from([("raise", 2), ("return", 1)]),
        "{report:#?}"
    );
    let return_leak = leaks
        .iter()
        .find(|diagnostic| diagnostic.completion.as_deref() == Some("return"))
        .expect("the final return abandons the pending resource");
    assert_eq!(return_leak.related[0].line, 7, "{report:#?}");
    let raise_origins = leaks
        .iter()
        .filter(|diagnostic| diagnostic.completion.as_deref() == Some("raise"))
        .flat_map(|diagnostic| diagnostic.related.iter().map(|span| span.line))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(raise_origins, std::collections::BTreeSet::from([7, 8]));
}

#[test]
fn v03_08_borrowed_to_owned_return_is_rejected_without_inventing_a_handoff() {
    let result = check_source(&format!(
        "{RESOURCE}def invalid(resource: Borrowed[C]) -> Owned[C]:\n    return resource\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1003"
                && diagnostic.certainty == "definite"
                && diagnostic.message.contains("Borrowed")
        }),
        "{result:#?}"
    );
    assert!(
        !result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("resource value escapes through an unsupported expression")
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_08_borrowed_alias_cannot_satisfy_an_owned_return_contract() {
    let source = format!(
        "{RESOURCE}def invalid(resource: Borrowed[C]) -> Owned[C]:\n    alias = resource\n    return alias\n"
    );
    let return_line = source
        .lines()
        .position(|line| line.trim() == "return alias")
        .unwrap()
        + 1;
    let result = check_source(&source);
    let violations = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1003")
        .collect::<Vec<_>>();
    assert_eq!(violations.len(), 1, "{result:#?}");
    assert_eq!(violations[0].certainty, "definite", "{result:#?}");
    assert_eq!(violations[0].primary.line, return_line, "{result:#?}");
    assert!(violations[0].message.contains("Borrowed"), "{result:#?}");
    assert!(!result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TP1002" || diagnostic.code == "TP9001"
    }), "a resolved borrowed alias is a contract violation, not a leak or unknown escape: {result:#?}");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_08_unannotated_owned_return_keeps_an_unknown_escape_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def escape():\n    resource = C()\n    return resource\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("returned without an Owned contract")
        }),
        "{result:#?}"
    );
    assert!(!result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "TP1003"));
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_08_field_and_container_storage_keep_unknown_escape_duties() {
    for (name, body) in [
        ("field", "holder = Holder()\n    holder.resource = resource"),
        ("container", "container = [resource]"),
    ] {
        let result = check_source(&format!(
            "{RESOURCE}class Holder:\n    pass\ndef escape():\n    resource = C()\n    {body}\n"
        ));
        assert!(
            result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP9001" && diagnostic.certainty == "unknown"
            }),
            "{name}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "{name}: {result:#?}"
        );
        assert_eq!(result.coverage.status, "incomplete", "{name}: {result:#?}");
    }
}

#[test]
fn v03_02_lost_last_alias_does_not_discard_the_local_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    resource = C()\n    alias = resource\n    resource = None\n    alias = None\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_13_uncertain_alias_close_cannot_discharge_every_candidate() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(flag):\n    first = C()\n    second = C()\n    if flag:\n        alias = first\n    else:\n        alias = second\n    alias.close()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.certainty == "possible"));
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.message.ends_with("on fallthrough")));
}

#[test]
fn v03_03_finally_replacement_cancels_owned_return_handoff() {
    let result = check_source(&format!(
        "{RESOURCE}def invalid() -> Owned[C]:\n    resource = C()\n    try:\n        return resource\n    finally:\n        return None\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
    assert!(leaks[0].message.ends_with("on return"));
    assert_eq!(leaks[0].primary.line, 11, "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 7, "{result:#?}");
}

#[test]
fn v03_03_raise_in_finally_replaces_the_pending_return_exit() {
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    resource = C()\n    try:\n        return\n    finally:\n        raise RuntimeError()\n"
    ));
    let leak = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP1002")
        .expect("replacing raise retains the cleanup duty");
    assert!(leak.message.ends_with("on raise"), "{result:#?}");
    assert_eq!(
        leak.primary.line,
        RESOURCE.lines().count() + 6,
        "{result:#?}"
    );
    assert_eq!(leak.related.len(), 1, "{result:#?}");
    assert_eq!(leak.related[0].line, RESOURCE.lines().count() + 2);
}

#[test]
fn v03_03_finally_replacements_keep_the_actual_exit_and_duty_origin() {
    let source = format!(
        "{RESOURCE}def raise_from_finally():\n    resource = C()\n    try:\n        return 1\n    finally:\n        raise RuntimeError()\ndef return_from_finally():\n    resource = C()\n    try:\n        raise ValueError()\n    finally:\n        return 1\n"
    );
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    for (completion, exit_line, acquisition_line) in [("raise", 11, 7), ("return", 17, 13)] {
        let leak = leaks
            .iter()
            .find(|diagnostic| diagnostic.message.ends_with(&format!("on {completion}")))
            .unwrap_or_else(|| panic!("missing {completion} finding: {result:#?}"));
        assert_eq!(leak.certainty, "definite", "{result:#?}");
        assert_eq!(leak.primary.line, exit_line, "{result:#?}");
        assert_eq!(leak.related.len(), 1, "{result:#?}");
        assert_eq!(leak.related[0].line, acquisition_line, "{result:#?}");
    }
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_03_nested_finally_replacement_stack_keeps_each_actual_exit() {
    let source = format!(
        "{RESOURCE}def nested(flag, replace):\n    first = C()\n    second = C()\n    try:\n        try:\n            return 1\n        finally:\n            if flag:\n                raise RuntimeError()\n    finally:\n        if replace:\n            return 2\n"
    );
    let source_line = |needle: &str| {
        source
            .lines()
            .position(|line| line.trim() == needle)
            .unwrap()
            + 1
    };
    let inner_return_line = source_line("return 1");
    let inner_raise_line = source_line("raise RuntimeError()");
    let outer_return_line = source_line("return 2");
    let first_acquisition_line = source_line("first = C()");
    let second_acquisition_line = source_line("second = C()");
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(
        leaks.len(),
        6,
        "each duty is reported at each actual nested exit: {report:#?}"
    );
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.certainty == "definite"));
    let by_exit = leaks.iter().fold(
        std::collections::BTreeMap::<(String, &str), usize>::new(),
        |mut counts, diagnostic| {
            *counts
                .entry((
                    diagnostic
                        .completion
                        .as_deref()
                        .unwrap_or_default()
                        .to_owned(),
                    if diagnostic.primary.line == inner_return_line {
                        "inner-return"
                    } else if diagnostic.primary.line == inner_raise_line {
                        "inner-raise"
                    } else if diagnostic.primary.line == outer_return_line {
                        "outer-return"
                    } else {
                        panic!("unexpected actual exit span: {report:#?}")
                    },
                ))
                .or_default() += 1;
            counts
        },
    );
    assert_eq!(
        by_exit,
        std::collections::BTreeMap::from([
            (("raise".to_owned(), "inner-raise"), 2),
            (("return".to_owned(), "inner-return"), 2),
            (("return".to_owned(), "outer-return"), 2),
        ]),
        "the nested pending completion is replaced only on the outer return path: {report:#?}"
    );
    let acquisition_lines = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        acquisition_lines,
        std::collections::BTreeSet::from([first_acquisition_line, second_acquisition_line])
    );
    assert_eq!(report.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_03_cleanup_after_unconditional_return_is_unreachable() {
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    resource = C()\n    return\n    resource.close()\n"
    ));
    let leak = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP1002")
        .expect("unreachable cleanup cannot discharge the duty");
    assert_eq!(leak.certainty, "definite", "{result:#?}");
    assert!(leak.message.ends_with("on return"), "{result:#?}");
}

#[test]
fn v03_03_finally_close_tracks_resource_after_alias_rebinding() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    alias = resource\n    try:\n        return\n    finally:\n        alias.close()\n        resource = None\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_03_nested_finally_runs_cleanup_through_the_pending_raise() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    try:\n        try:\n            raise RuntimeError()\n        finally:\n            alias = resource\n            alias.close()\n    finally:\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_06_distinct_return_and_raise_exits_keep_separate_findings() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(flag):\n    resource = C()\n    if flag:\n        return\n    raise RuntimeError()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on return")));
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on raise")));
    let mut exits = leaks
        .iter()
        .map(|diagnostic| diagnostic.primary.line)
        .collect::<Vec<_>>();
    exits.sort_unstable();
    assert_eq!(exits, [9, 10], "{result:#?}");
    assert!(
        leaks.iter().all(|diagnostic| {
            diagnostic.certainty == "definite"
                && diagnostic.related.len() == 1
                && diagnostic.related[0].line == 7
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_07_closing_a_pending_owned_return_is_a_contract_violation() {
    let result = check_source(&format!(
        "{RESOURCE}def invalid() -> Owned[C]:\n    resource = C()\n    try:\n        return resource\n    finally:\n        resource.close()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1003"
                && diagnostic.certainty == "definite"
                && diagnostic.message.contains("must return an open resource")
        }),
        "{result:#?}"
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn v03_07_context_manager_cleanup_invalidates_owned_return() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef invalid() -> Owned[C]:\n    resource = C()\n    with closing(resource):\n        return resource\n"
    ));
    let violations = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1003")
        .collect::<Vec<_>>();
    assert_eq!(violations.len(), 1, "{result:#?}");
    assert_eq!(violations[0].certainty, "definite", "{result:#?}");
    assert!(
        violations[0]
            .message
            .contains("must return an open resource"),
        "{result:#?}"
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "the wrapper closes the duty before return handoff: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_07_finally_close_then_replacement_return_hands_off_only_the_open_result() {
    let source = format!(
        "{RESOURCE}def make() -> Owned[C]:\n    abandoned = C()\n    forwarded = C()\n    try:\n        return abandoned\n    finally:\n        abandoned.close()\n        return forwarded\n"
    );
    let result = check_source(&source);
    assert!(
        result.diagnostics.is_empty(),
        "the replaced result is cleaned before the open replacement is handed off: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_09_owned_helper_transfers_and_discharges_the_callers_duty_once() {
    let result = check_source(&format!(
        "{RESOURCE}def finish(resource: Owned[C]):\n    resource.close()\ndef caller():\n    resource = C()\n    finish(resource)\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_resolved_helper_transfer_preserves_unrelated_caller_duties() {
    let result = check_source(&format!(
        "{RESOURCE}def finish(resource: Owned[C]):\n    resource.close()\ndef caller():\n    first = C()\n    second = C()\n    finish(first)\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 10, "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_owned_helper_return_restores_the_same_duty_identity() {
    let result = check_source(&format!(
        "{RESOURCE}def forward(resource: Owned[C]) -> Owned[C]:\n    return resource\ndef cleanup():\n    resource = C()\n    returned = forward(resource)\n    returned.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_repeated_owned_argument_positions_share_one_duty() {
    let source = "from typepact import resource\nfrom typepact.rust import Owned\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef consume(first: Owned[C], second: Owned[C]):\n    first.close()\n    second.status()\ndef cleanup():\n    resource = C()\n    consume(resource, resource)\n";
    let result = check_source(source);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1001")
            .count(),
        1,
        "{result:#?}"
    );
    let helper_leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(helper_leaks.len(), 1, "{result:#?}");
    assert_eq!(helper_leaks[0].primary.line, 7, "{result:#?}");
}

#[test]
fn v03_09_borrowed_helper_does_not_delegate_the_callers_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def inspect(resource: Borrowed[C]):\n    resource.close()\ndef cleanup():\n    resource = C()\n    inspect(resource)\n    resource.close()\n"
    ));
    let borrowed_closes = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP2001")
        .collect::<Vec<_>>();
    assert_eq!(borrowed_closes.len(), 1, "{result:#?}");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn v03_09_borrowed_helper_keeps_the_duty_with_the_caller_on_return_and_raise() {
    let result = check_source(&format!(
        "{RESOURCE}def inspect(resource: Borrowed[C]):\n    pass\ndef fail(resource: Borrowed[C]):\n    raise RuntimeError()\ndef leaked_caller():\n    resource = C()\n    inspect(resource)\ndef cleaned_raise_caller():\n    resource = C()\n    try:\n        fail(resource)\n    except RuntimeError:\n        resource.close()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(
        leaks.len(),
        1,
        "only the caller that abandons its duty leaks: {result:#?}"
    );
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 11, "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_argument_failure_precedes_owned_helper_entry() {
    let result = check_source(&format!(
        "{RESOURCE}def fail() -> int:\n    raise RuntimeError()\ndef consume(resource: Owned[C], marker: int):\n    resource.close()\ndef cleanup():\n    resource = C()\n    try:\n        consume(resource, fail())\n    except RuntimeError:\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_09_owned_helper_cleanup_effects_survive_raising_exits() {
    let result = check_source(&format!(
        "{RESOURCE}def close_then_raise(resource: Owned[C]):\n    resource.close()\n    raise RuntimeError()\ndef close_in_finally(resource: Owned[C]):\n    try:\n        raise ValueError()\n    finally:\n        resource.close()\ndef caller():\n    first = C()\n    try:\n        close_then_raise(first)\n    except RuntimeError:\n        pass\n    second = C()\n    try:\n        close_in_finally(second)\n    except ValueError:\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "successful delegated cleanup remains effective on a later raise: {result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_owned_helper_close_failure_keeps_the_duty_on_raise() {
    let source = "from typepact import resource\nfrom typepact.rust import Owned\n@resource(closed_by='close')\nclass Failing:\n    def close(self):\n        raise OSError()\ndef finish(resource: Owned[Failing]):\n    resource.close()\ndef caller():\n    resource = Failing()\n    try:\n        finish(resource)\n    except OSError:\n        pass\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on raise"), "{result:#?}");
    assert_eq!(leaks[0].primary.line, 8, "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 7, "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_09_owned_helper_return_without_caller_cleanup_keeps_one_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def handoff(resource: Owned[C]) -> Owned[C]:\n    return resource\ndef caller():\n    resource = C()\n    handoff(resource)\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(
        leaks.len(),
        1,
        "the returned owner remains the caller's duty: {result:#?}"
    );
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_04_close_raise_preserves_the_open_duty() {
    let source = "from typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def close(self):\n        raise RuntimeError()\ndef cleanup():\n    resource = C()\n    resource.close()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
    assert!(leaks[0].message.ends_with("on raise"));
}

#[test]
fn v03_05_matched_exception_handler_cleans_and_else_runs_only_on_success() {
    let result = check_source(&format!(
        "{RESOURCE}def fail():\n    raise RuntimeError\ndef cleanup():\n    resource = C()\n    try:\n        fail()\n    except RuntimeError:\n        resource.close()\ndef cleanup_else():\n    resource = C()\n    try:\n        pass\n    except RuntimeError:\n        pass\n    else:\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_05_builtin_exception_constructor_keeps_exact_handler_routing() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    try:\n        raise RuntimeError('failure')\n    except RuntimeError:\n        resource.close()\n"
    ));
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_05_shadowed_builtin_exception_constructor_keeps_a_boundary() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    RuntimeError = ValueError\n    try:\n        raise RuntimeError()\n    except RuntimeError:\n        resource.close()\n"
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP9001"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_05_shadowed_base_exception_handler_does_not_hide_an_unmatched_raise() {
    let source = format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    BaseException = ValueError\n    try:\n        raise RuntimeError()\n    except BaseException:\n        resource.close()\n"
    );
    let acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let handler_line = source
        .lines()
        .position(|line| line.trim() == "except BaseException:")
        .unwrap()
        + 1;
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{result:#?}");
    assert!(leaks[0].message.ends_with("on raise"), "{result:#?}");
    assert_eq!(leaks[0].related[0].line, acquisition_line, "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.primary.line == handler_line
                && diagnostic
                    .message
                    .contains("exception type may or may not match")
        }),
        "a rebound handler type must remain an explicit uncertainty: {result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_05_custom_exception_handler_keeps_the_unmatched_exit_unknown() {
    let result = check_source(&format!(
        "{RESOURCE}class CleanupError(Exception):\n    pass\ndef fail():\n    raise CleanupError()\ndef cleanup():\n    resource = C()\n    try:\n        fail()\n    except CleanupError:\n        resource.close()\n"
    ));
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP9001"),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_05_custom_exception_inheritance_keeps_handler_matching_unknown() {
    let source = format!(
        "{RESOURCE}class ParentError(Exception):\n    pass\nclass ChildError(ParentError):\n    pass\ndef cleanup():\n    resource = C()\n    try:\n        raise ChildError()\n    except ParentError:\n        resource.close()\n"
    );
    let acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let raise_line = source
        .lines()
        .position(|line| line.trim() == "raise ChildError()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("exception type may or may not match")
        }),
        "custom exception ancestry must remain an explicit boundary: {result:#?}"
    );
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(leaks.iter().all(|diagnostic| {
        diagnostic.certainty == "possible"
            && diagnostic.message.ends_with("on raise")
            && diagnostic.primary.line == raise_line
            && diagnostic.related[0].line == acquisition_line
    }));
    let primary_columns = leaks
        .iter()
        .map(|diagnostic| diagnostic.primary.column)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        primary_columns,
        std::collections::BTreeSet::from([9, 15]),
        "constructor failure and explicit raise retain separate source witnesses: {result:#?}"
    );
}

#[test]
fn v03_05_handler_order_and_handler_else_raises_still_cross_finally() {
    let result = check_source(&format!(
        "{RESOURCE}def ordered():\n    resource = C()\n    try:\n        raise RuntimeError()\n    except ValueError:\n        pass\n    except RuntimeError:\n        resource.close()\ndef handler_raises():\n    resource = C()\n    try:\n        raise RuntimeError()\n    except RuntimeError:\n        raise ValueError()\n    finally:\n        resource.close()\ndef else_raises():\n    resource = C()\n    try:\n        pass\n    except RuntimeError:\n        pass\n    else:\n        raise RuntimeError()\n    finally:\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_05_unmatched_raise_replaced_by_finally_raise_keeps_actual_exit() {
    let source = format!(
        "{RESOURCE}def leak():\n    resource = C()\n    try:\n        raise RuntimeError()\n    except ValueError:\n        pass\n    finally:\n        raise OSError()\n"
    );
    let acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let replacement_raise_line = source
        .lines()
        .position(|line| line.trim() == "raise OSError()")
        .unwrap()
        + 1;
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("raise"), "{report:#?}");
    assert_eq!(leaks[0].primary.line, replacement_raise_line, "{report:#?}");
    assert_eq!(leaks[0].related[0].line, acquisition_line, "{report:#?}");
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
}

#[test]
fn v03_05_handled_exception_skips_else_and_retains_the_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def fail():\n    raise RuntimeError\ndef leak():\n    resource = C()\n    try:\n        fail()\n    except RuntimeError:\n        pass\n    else:\n        resource.close()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "definite"
                && diagnostic.message.ends_with("on fallthrough")
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_05_handler_type_expression_effects_reach_matched_and_unmatched_paths() {
    let result = check_source(&format!(
        "{RESOURCE}def fail():\n    raise RuntimeError\ndef cleanup():\n    resource = C()\n    try:\n        fail()\n    except resource.close():\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP9001"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_09_owned_helper_leak_is_not_reported_again_by_the_caller() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(resource: Owned[C]):\n    pass\ndef caller():\n    resource = C()\n    leak(resource)\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"));
}

#[test]
fn v03_02_unqualified_resource_parameters_do_not_create_incoming_duties() {
    let result = check_source(&format!("{RESOURCE}def observe(resource: C):\n    pass\n"));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn v03_11_contextlib_closing_and_import_alias_discharge_on_normal_exit() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing as close_on_exit\ndef cleanup():\n    resource = C()\n    with close_on_exit(resource):\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_11_contextlib_module_qualified_import_uses_the_reviewed_model() {
    let result = check_source(&format!(
        "{RESOURCE}import contextlib as ctx\ndef cleanup():\n    resource = C()\n    with ctx.closing(resource):\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_11_contextlib_closing_aliases_the_entered_resource() {
    let result = check_source(
        "import contextlib as ctx\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): return 'ready'\ndef cleanup():\n    resource = C()\n    with ctx.closing(resource) as entered:\n        entered.status()\n",
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.code.as_str(), "TP1001" | "TP1002")),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_11_contextlib_closing_preserves_body_exception_routing() {
    let result = check_source(
        "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    primary = C()\n    handler_resource = C()\n    try:\n        with closing(primary):\n            raise ValueError()\n    except ValueError:\n        handler_resource.close()\n",
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_11_contextlib_reexport_preserves_exact_model_identity() {
    let bridge = SourceFile::new(
        "bridge.py",
        "bridge",
        "from contextlib import closing as managed\n",
    );
    let client = SourceFile::new(
        "client.py",
        "client",
        format!(
            "from bridge import managed\n{RESOURCE}def cleanup():\n    resource = C()\n    with managed(resource):\n        pass\n"
        ),
    );
    let result = check(&[client, bridge], "3.11");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_11_contextlib_module_support_does_not_resolve_other_module_imports() {
    let result = check_source(&format!(
        "{RESOURCE}import pathlib as paths\ndef cleanup():\n    resource = C()\n    resource.close()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message == "module imports are not resolved in 0.1"
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_11_supplied_contextlib_module_cannot_claim_the_stdlib_model() {
    let local_module = SourceFile::new(
        "contextlib.py",
        "contextlib",
        "def closing(value):\n    return value\n",
    );
    for client in [
        format!(
            "import contextlib as ctx\n{RESOURCE}def leak():\n    resource = C()\n    with ctx.closing(resource):\n        pass\n"
        ),
        format!(
            "from contextlib import closing as managed\n{RESOURCE}def leak():\n    resource = C()\n    with managed(resource):\n        pass\n"
        ),
    ] {
        let source = client.clone();
        let client = SourceFile::new("client.py", "client", client);
        let result = check(&[client, local_module.clone()], "3.11");
        assert!(
            result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
            }),
            "{source}\n{result:#?}"
        );
        assert_eq!(result.coverage.status, "incomplete");
    }
}

#[test]
fn v03_11_contextlib_lookalike_cannot_discharge_a_duty() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef closing(value):\n    pass\ndef leak():\n    resource = C()\n    with closing(resource):\n        pass\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_10_multiple_contextlib_closers_unwind_all_successfully_entered_managers() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef cleanup():\n    first = C()\n    second = C()\n    with closing(first), closing(second):\n        raise RuntimeError\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_10_reviewed_context_cleanup_runs_on_return_break_and_continue() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef return_cleanup():\n    resource = C()\n    with closing(resource):\n        return\ndef break_cleanup():\n    resource = C()\n    for unused in (None,):\n        with closing(resource):\n            break\ndef continue_cleanup():\n    resource = C()\n    for unused in (None,):\n        with closing(resource):\n            continue\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "all three control transfers unwind the reviewed manager: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_10_unknown_context_keeps_duties_across_return_break_and_continue() {
    let source = format!(
        "{RESOURCE}class UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef return_path():\n    resource = C()\n    with UnknownManager():\n        return\ndef break_path():\n    resource = C()\n    for item in (None,):\n        with UnknownManager():\n            break\ndef continue_path():\n    resource = C()\n    for item in (None,):\n        with UnknownManager():\n            continue\n"
    );
    let acquisition_lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "resource = C()").then_some(index + 1))
        .collect::<std::collections::BTreeSet<_>>();
    let result =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 6, "{result:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty == "possible"),
        "an unknown manager cannot prove cleanup on any transfer: {result:#?}"
    );
    let completions = leaks.iter().fold(
        std::collections::BTreeMap::<&str, Vec<_>>::new(),
        |mut grouped, diagnostic| {
            grouped
                .entry(diagnostic.completion.as_deref().unwrap_or_default())
                .or_default()
                .push(*diagnostic);
            grouped
        },
    );
    assert_eq!(
        completions.get("return").map(Vec::len),
        Some(1),
        "{result:#?}"
    );
    assert_eq!(
        completions.get("fallthrough").map(Vec::len),
        Some(2),
        "break and continue reach the enclosing function exit, not a function-level break/continue exit: {result:#?}"
    );
    assert_eq!(
        completions.get("raise").map(Vec::len),
        Some(3),
        "manager construction can raise before each body or exit is entered: {result:#?}"
    );
    let reported_acquisitions = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(reported_acquisitions, acquisition_lines, "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_11_closing_a_borrowed_value_is_still_rejected() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef helper(resource: Borrowed[C]):\n    with closing(resource):\n        pass\n"
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP2001"),
        "{result:#?}"
    );
}

#[test]
fn v03_12_unknown_context_manager_cannot_discharge_a_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(manager):\n    resource = C()\n    with manager:\n        pass\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_12_finalizer_does_not_establish_deterministic_cleanup() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def __del__(self): self.close()\n@resource(closed_by='close')\nclass Plain(object):\n    def close(self): pass\ndef cleanup():\n    finalizable = C()\n    plain = Plain()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(
        leaks.iter().any(|leak| {
            leak.certainty == "possible"
                && leak.message.ends_with("on fallthrough")
                && leak.related.first().is_some_and(|span| span.line == 10)
        }),
        "the finalizer leaves its own duty uncertain: {result:#?}"
    );
    assert!(
        leaks.iter().any(|leak| {
            leak.certainty == "definite"
                && leak.message.ends_with("on fallthrough")
                && leak.related.first().is_some_and(|span| span.line == 11)
        }),
        "the finalizer boundary must not weaken unrelated duties: {result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("finalizer")),
        "{result:#?}"
    );

    let explicit_close = check_source(
        "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def __del__(self): self.close()\ndef cleanup():\n    resource = C()\n    resource.close()\n",
    );
    assert!(
        explicit_close
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP1002"),
        "{explicit_close:#?}"
    );
    assert_eq!(
        explicit_close.coverage.status, "complete-for-supported-subset",
        "{explicit_close:#?}"
    );

    let inherited_finalizer = check_source(
        "from typepact import resource\nclass Base:\n    def __del__(self): pass\n@resource(closed_by='close')\nclass C(Base):\n    def close(self): pass\ndef cleanup():\n    resource = C()\n",
    );
    assert!(
        inherited_finalizer
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002" && diagnostic.certainty == "possible"),
        "inherited finalizer effects cannot support a definite finding: {inherited_finalizer:#?}"
    );
    assert!(
        inherited_finalizer
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("base-class finalizer effects are not modeled") }),
        "{inherited_finalizer:#?}"
    );
}

#[test]
fn v03_10_failed_later_context_entry_still_unwinds_earlier_managers() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef cleanup(manager):\n    first = C()\n    second = C()\n    with closing(first), manager:\n        pass\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(leaks.iter().all(|diagnostic| {
        diagnostic.certainty == "possible" && diagnostic.related[0].line == 9
    }));
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on fallthrough")));
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on raise")));
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_10_failed_later_context_argument_unwinds_earlier_manager() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef fail() -> C:\n    raise RuntimeError()\ndef cleanup():\n    first = C()\n    try:\n        with closing(first), closing(fail()):\n            pass\n    except RuntimeError:\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_14_zero_iteration_loop_cannot_prove_cleanup() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(flag):\n    resource = C()\n    while flag:\n        resource.close()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible");
    assert!(result.coverage.status.starts_with("incomplete"));
}

#[test]
fn v03_14_nested_while_loops_keep_zero_iteration_cleanup_uncertain() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(outer, inner):\n    resource = C()\n    while outer:\n        while inner:\n            resource.close()\n            break\n        break\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_15_unknown_iterator_effect_does_not_prove_loop_cleanup() {
    let source = format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    for item in unresolved_values():\n        resource.close()\n"
    );
    let acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let iterator_line = source
        .lines()
        .position(|line| line.contains("for item in unresolved_values()"))
        .unwrap()
        + 1;
    let close_line = source
        .lines()
        .position(|line| line.trim() == "resource.close()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    let fallthrough = leaks
        .iter()
        .find(|diagnostic| diagnostic.message.ends_with("on fallthrough"))
        .expect("unknown iterator still has an ordinary exit");
    assert_eq!(fallthrough.certainty, "possible", "{result:#?}");
    assert_eq!(fallthrough.related.len(), 2, "{result:#?}");
    assert_eq!(fallthrough.related[0].line, acquisition_line, "{result:#?}");
    assert_eq!(fallthrough.related[1].line, close_line, "{result:#?}");
    let raised = leaks
        .iter()
        .find(|diagnostic| diagnostic.message.ends_with("on raise"))
        .expect("iterator evaluation can raise before the body executes");
    assert_eq!(raised.certainty, "possible", "{result:#?}");
    assert_eq!(raised.primary.line, iterator_line, "{result:#?}");
    assert_eq!(raised.related.len(), 1, "{result:#?}");
    assert_eq!(raised.related[0].line, acquisition_line, "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_13_alias_target_budget_keeps_candidate_duties_possible() {
    let flag_names = (0..64)
        .map(|index| format!("flag_{index}"))
        .collect::<Vec<_>>();
    let mut source = format!("{RESOURCE}def cleanup({}):\n", flag_names.join(", "));
    for index in 0..=64 {
        source.push_str(&format!("    resource_{index} = C()\n"));
    }
    source.push_str("    selected = resource_0\n");
    for index in 0..64 {
        source.push_str(&format!(
            "    if flag_{index}:\n        selected = resource_{}\n",
            index + 1
        ));
    }
    source.push_str("    selected.close()\n");

    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("alias target budget exceeded for selected") }),
        "{result:#?}"
    );
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(!leaks.is_empty(), "{result:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty != "definite"),
        "an over-budget alias cannot leave any candidate duty definitely open: {result:#?}"
    );
}

#[test]
fn v03_16_alias_budget_inside_a_loop_cannot_prove_candidate_cleanup() {
    let flag_names = (0..64)
        .map(|index| format!("flag_{index}"))
        .collect::<Vec<_>>();
    let mut source = format!("{RESOURCE}def cleanup({}, again):\n", flag_names.join(", "));
    for index in 0..=64 {
        source.push_str(&format!("    resource_{index} = C()\n"));
    }
    source.push_str("    selected = resource_0\n");
    for index in 0..64 {
        source.push_str(&format!(
            "    if flag_{index}:\n        selected = resource_{}\n",
            index + 1
        ));
    }
    source.push_str("    while again:\n        selected.close()\n        break\n");

    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("alias target budget exceeded for selected")),
        "the loop must retain the widened alias boundary: {result:#?}"
    );
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(!leaks.is_empty(), "{result:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty != "definite"),
        "a loop over an over-budget alias cannot prove cleanup of any candidate duty: {result:#?}"
    );
    let candidate_acquisition_lines = source
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            (text.trim_start().starts_with("resource_") && text.trim_end().ends_with("= C()"))
                .then_some(line + 1)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let reported_acquisition_lines = leaks
        .iter()
        .flat_map(|diagnostic| diagnostic.related.iter().map(|span| span.line))
        .filter(|line| candidate_acquisition_lines.contains(line))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        reported_acquisition_lines, candidate_acquisition_lines,
        "every candidate duty remains represented after loop widening: {result:#?}"
    );
}

#[test]
fn v03_14_finite_for_over_tuple_cleans_each_alias() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    first = C()\n    second = C()\n    for resource in (first, second):\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_14_tuple_values_are_captured_before_loop_body_rebinding() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    first = C()\n    second = C()\n    for item in (first, second):\n        second = first\n        item.close()\n"
    ));
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_14_finite_for_zero_items_preserves_the_unclosed_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    resource = C()\n    for unused in ():\n        resource.close()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
}

#[test]
fn v03_14_unresolved_tuple_name_keeps_iterator_failure_and_skips_false_cleanup_proof() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    for item in (missing_value,):\n        resource.close()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_14_finite_for_break_skips_later_items_and_loop_else() {
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    first = C()\n    second = C()\n    for resource in (first, second):\n        resource.close()\n        break\n    else:\n        second.close()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite");
}

#[test]
fn v03_14_loop_else_runs_on_finite_exhaustion() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    for unused in (None,):\n        pass\n    else:\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_14_generated_break_and_continue_paths_route_loop_else_correctly() {
    let source = format!(
        "{RESOURCE}def break_cleanup(flag):\n    resource = C()\n    for item in (None,):\n        if flag:\n            break\n    else:\n        resource.close()\ndef continue_cleanup(flag):\n    resource = C()\n    for item in (None,):\n        if flag:\n            continue\n    else:\n        resource.close()\n"
    );
    let break_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let break_close_line = source
        .lines()
        .position(|line| line.trim() == "resource.close()")
        .unwrap()
        + 1;
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("fallthrough"));
    assert_eq!(
        leaks[0].related[0].line, break_acquisition_line,
        "{report:#?}"
    );
    assert_eq!(leaks[0].related[1].line, break_close_line, "{report:#?}");
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
}

#[test]
fn v03_15_repeated_loop_acquisition_retains_unknown_multiplicity() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(flag):\n    while flag:\n        resource = C()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result.coverage.reasons.iter().any(|reason| {
        reason.contains("repeated loop allocation is summarized with unknown multiplicity")
    }));
}

#[test]
fn v03_15_closing_the_latest_loop_allocation_keeps_older_duties() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(flag):\n    while flag:\n        resource = C()\n    resource.close()\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
    assert!(result.coverage.status.starts_with("incomplete"));
}

#[test]
fn v03_14_continue_and_break_run_finally_cleanup_before_loop_exit() {
    let result = check_source(&format!(
        "{RESOURCE}def continue_cleanup():\n    resource = C()\n    for item in (None,):\n        try:\n            continue\n        finally:\n            resource.close()\ndef break_cleanup():\n    resource = C()\n    while True:\n        try:\n            break\n        finally:\n            resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_14_break_and_continue_from_finally_replace_exits_and_route_loop_else() {
    let source = format!(
        "{RESOURCE}def break_from_finally():\n    resource = C()\n    for item in (None,):\n        try:\n            raise RuntimeError()\n        finally:\n            break\n    else:\n        resource.close()\ndef continue_from_finally():\n    resource = C()\n    for item in (None,):\n        try:\n            raise RuntimeError()\n        finally:\n            continue\n    else:\n        resource.close()\n"
    );
    let report = typepact::check_report(
        &[SourceFile::new("phase03.py", "phase03", source.clone())],
        "3.11",
    );
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    let break_function_line = source
        .lines()
        .position(|line| line.trim() == "def break_from_finally():")
        .unwrap()
        + 1;
    let break_resource_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    assert_eq!(leaks[0].certainty, "definite", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("fallthrough"));
    assert_eq!(
        leaks[0].primary.line, break_function_line,
        "fallthrough after the replaced raise is attributed to its function: {report:#?}"
    );
    assert_eq!(leaks[0].primary.end_line, 14, "{report:#?}");
    assert_eq!(leaks[0].related[0].line, break_resource_line, "{report:#?}");
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "{report:#?}"
    );
}

#[test]
fn v03_16_static_for_iteration_budget_widens_instead_of_claiming_cleanup() {
    let items = std::iter::repeat_n("None", 17)
        .collect::<Vec<_>>()
        .join(", ");
    let result = check_source(&format!(
        "{RESOURCE}def leak():\n    resource = C()\n    for unused in ({items},):\n        pass\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("static for-loop iteration budget exceeded") }));
}

#[test]
fn v03_16_break_before_static_for_budget_does_not_create_a_truncation_boundary() {
    let items = vec!["None"; 17].join(", ");
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    for item in ({items},):\n        resource.close()\n        break\n"
    ));
    assert!(result.diagnostics.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_16_loop_fixed_point_budget_keeps_cleanup_incomplete() {
    let mut source = format!("{RESOURCE}def cleanup():\n    value_0 = C()\n");
    for index in 1..=20 {
        source.push_str(&format!("    value_{index} = None\n"));
    }
    source.push_str("    while keep_running:\n");
    for index in (1..=20).rev() {
        source.push_str(&format!("        value_{index} = value_{}\n", index - 1));
    }
    source.push_str("    value_20.close()\n");

    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("loop fixed-point budget exceeded") }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_17_unawaited_async_cleanup_does_not_discharge_a_duty() {
    let source = "from typepact import resource\n@resource(closed_by=\"aclose\")\nclass C:\n    async def aclose(self): pass\nasync def leak():\n    resource = C()\n    resource.aclose()\n";
    let result = check_source(source);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002" && diagnostic.certainty == "definite"),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("created coroutine is not awaited") }));
}

#[test]
fn v03_18_async_coroutine_creation_does_not_delegate_an_owned_duty() {
    let source = "from typepact import resource\nfrom typepact.rust import Owned\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\nasync def finish(resource: Owned[C]):\n    resource.close()\ndef caller():\n    resource = C()\n    pending = finish(resource)\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "definite"
                && diagnostic.message.ends_with("on fallthrough")
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("created coroutine is not awaited") }));
}

#[test]
fn v03_18_cancellation_before_coroutine_entry_keeps_only_caller_duties() {
    let source = "from typepact import resource\nfrom typepact.rust import Owned\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nasync def close_local():\n    local = C()\n    local.close()\nasync def close_owned(resource: Owned[C]):\n    resource.close()\nasync def caller():\n    await close_local()\n    resource = C()\n    await close_owned(resource)\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{result:#?}");
    assert!(leaks[0].message.ends_with("on cancelled"), "{result:#?}");
    assert_eq!(leaks[0].primary.line, 14, "{result:#?}");
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("cancellation may interrupt the awaited operation")));
}

#[test]
fn v03_20_task_scheduling_does_not_infer_completion_or_cleanup() {
    let source = "from typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\nasync def finish(resource):\n    resource.close()\ndef caller():\n    resource = C()\n    pending = finish(resource)\n    asyncio.create_task(pending)\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && (diagnostic.message.contains("unresolved method")
                    || diagnostic.message.contains("unknown call effect"))
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("created coroutine is not awaited") }));
}

#[test]
fn v03_20_asyncio_handoff_helpers_remain_explicit_boundaries() {
    for scheduler in [
        "asyncio.create_task(pending)",
        "asyncio.gather(pending)",
        "asyncio.shield(pending)",
        "asyncio.wait_for(pending, timeout=1)",
    ] {
        let source = format!(
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nasync def finish(resource):\n    resource.close()\ndef caller():\n    resource = C()\n    pending = finish(resource)\n    {scheduler}\n"
        );
        let result = check_source(&source);
        assert_eq!(
            result.coverage.status, "incomplete",
            "{scheduler}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "TP9001" || diagnostic.code == "TP9002" }),
            "{scheduler}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "{scheduler}: {result:#?}"
        );
    }
}

#[test]
fn v03_20_scheduled_multi_resource_capture_keeps_each_duty_uncertain() {
    let source = format!(
        "{RESOURCE}async def finish(first: Owned[C], second: Owned[C]):\n    first.close()\n    second.close()\ndef caller():\n    first = C()\n    second = C()\n    pending = finish(first, second)\n    asyncio.create_task(pending)\n"
    );
    let first_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "first = C()")
        .unwrap()
        + 1;
    let second_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "second = C()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 4, "{result:#?}");
    assert!(
        leaks.iter().all(|diagnostic| {
            diagnostic.certainty == "possible"
                && (diagnostic.message.ends_with("on fallthrough")
                    || diagnostic.message.ends_with("on raise"))
        }),
        "scheduler handoff cannot prove task execution or successful cleanup: {result:#?}"
    );
    assert_eq!(
        leaks
            .iter()
            .filter(|diagnostic| diagnostic.message.ends_with("on fallthrough"))
            .count(),
        2,
        "both captured duties must be checked on the ordinary exit: {result:#?}"
    );
    assert_eq!(
        leaks
            .iter()
            .filter(|diagnostic| diagnostic.message.ends_with("on raise"))
            .count(),
        2,
        "both captured duties must be checked if scheduling raises: {result:#?}"
    );
    let acquisition_counts = leaks
        .iter()
        .flat_map(|diagnostic| diagnostic.related.iter().map(|span| span.line))
        .fold(std::collections::BTreeMap::new(), |mut counts, line| {
            *counts.entry(line).or_insert(0) += 1;
            counts
        });
    assert_eq!(
        acquisition_counts,
        std::collections::BTreeMap::from([
            (first_acquisition_line, 2),
            (second_acquisition_line, 2),
        ]),
        "both captured caller duties remain represented: {result:#?}"
    );
}

#[test]
fn v03_26_pending_coroutine_budget_widens_captures_and_keeps_the_duty_unknown() {
    let mut source = format!(
        "{RESOURCE}async def hold(resource: Borrowed[C]):\n    pass\ndef leak():\n    resource = C()\n"
    );
    for index in 0..129 {
        source.push_str(&format!("    pending_{index} = hold(resource)\n"));
    }
    let result = check_source(&source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("pending coroutine budget exceeded")
        }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_26_pending_coroutine_budget_edit_recovers_with_fresh_result_parity() {
    let mut source = format!(
        "{RESOURCE}async def hold(resource: Borrowed[C]):\n    pass\ndef cleanup():\n    resource = C()\n"
    );
    for index in 0..129 {
        source.push_str(&format!("    pending_{index} = hold(resource)\n"));
    }
    let over_budget = SourceFile::new("phase03.py", "phase03", source);
    let mut session = AnalysisSession::default();
    let first = session.check(std::slice::from_ref(&over_budget), "3.11");
    assert_eq!(first.coverage.status, "incomplete", "{first:#?}");
    assert!(first.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TP9001"
            && diagnostic
                .message
                .contains("pending coroutine budget exceeded")
    }));

    let recovered_file = SourceFile::new(
        "phase03.py",
        "phase03",
        format!("{RESOURCE}def cleanup():\n    resource = C()\n    resource.close()\n"),
    );
    let recovered = session.check(std::slice::from_ref(&recovered_file), "3.11");
    assert_eq!(
        recovered,
        check(std::slice::from_ref(&recovered_file), "3.11")
    );
    assert_eq!(
        recovered.coverage.status, "complete-for-supported-subset",
        "{recovered:#?}"
    );
    assert!(!recovered.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("pending coroutine budget exceeded")
    }));
    assert!(!recovered
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "TP1002"));
}

#[test]
fn v03_17_awaited_async_cleanup_tracks_cancellation_as_uncertain() {
    let source = "from typepact import resource\n@resource(closed_by=\"aclose\")\nclass C:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = C()\n    await resource.aclose()\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let cleanup_duties = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(cleanup_duties.len(), 1, "{report:#?}");
    let cancelled = cleanup_duties[0];
    assert_eq!(cancelled.certainty, "possible", "{report:#?}");
    assert_eq!(cancelled.completion.as_deref(), Some("cancelled"));
    assert!(cancelled.message.contains("remains open on cancelled"));
    assert_eq!(cancelled.primary.line, 7, "{report:#?}");
    assert_eq!(cancelled.related.len(), 1, "{report:#?}");
    assert_eq!(cancelled.related[0].line, 6, "{report:#?}");
    assert!(report
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("cancellation may interrupt the awaited operation")));
    assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
}

#[test]
fn v03_17_awaited_local_coroutine_alias_keeps_cancellation_uncertain() {
    let source = "from typepact import resource\nfrom typepact.rust import Owned\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\nasync def finish(resource: Owned[C]):\n    await resource.aclose()\nasync def cleanup():\n    resource = C()\n    pending = finish(resource)\n    alias = pending\n    await alias\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("cancellation may interrupt the awaited operation") }));
}

#[test]
fn v03_18_cancellation_during_use_enters_finally_and_can_interrupt_awaited_close() {
    let source = "from typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\n    async def pause(self): pass\nasync def cleanup():\n    resource = C()\n    try:\n        await resource.pause()\n    finally:\n        await resource.aclose()\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("cancellation may interrupt the awaited operation") }));
}

#[test]
fn v03_18_unknown_sync_context_can_suppress_await_cancellation() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    with UnknownManager():\n        await resource.pause()\n        resource.close()\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
                && diagnostic
                    .related
                    .first()
                    .is_some_and(|span| span.line == 10)
        }),
        "the cancellation path remains feasible: {result:#?}"
    );
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TP1002"
            && diagnostic.certainty == "possible"
            && diagnostic.message.ends_with("on fallthrough")
            && diagnostic.related.first().is_some_and(|span| span.line == 10)
    }), "the unknown synchronous exit may suppress cancellation and skip the body cleanup: {result:#?}");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_17_repeated_coroutine_await_is_an_explicit_boundary() {
    let source = "from typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\nasync def finish(resource):\n    await resource.aclose()\nasync def cleanup():\n    resource = C()\n    pending = finish(resource)\n    await pending\n    await pending\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001" && diagnostic.message.contains("awaited more than once")
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn v03_17_ambiguous_coroutine_join_keeps_captured_duties_unknown() {
    let source = format!(
        "{RESOURCE}async def finish(resource: Owned[C]):\n    resource.close()\nasync def cleanup(flag):\n    first = C()\n    second = C()\n    if flag:\n        pending = finish(first)\n    else:\n        pending = finish(second)\n    await pending\n"
    );
    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && (diagnostic
                    .message
                    .contains("await target is not a resolved direct coroutine")
                    || diagnostic
                        .message
                        .contains("coroutine execution or captures differ"))
        }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_19_async_context_cleanup_keeps_the_cancellation_exit() {
    let source = "from contextlib import aclosing\nfrom typepact import resource\n@resource(closed_by=\"aclose\")\nclass C:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = C()\n    async with aclosing(resource):\n        pass\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("cancellation may interrupt the awaited operation") }));
}

#[test]
fn v03_19_async_context_entry_failure_unwinds_prior_manager() {
    let source = "from contextlib import aclosing\nfrom typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\ndef fail() -> C:\n    raise RuntimeError()\nasync def cleanup():\n    resource = C()\n    try:\n        async with aclosing(resource), aclosing(fail()):\n            pass\n    except RuntimeError:\n        pass\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("cancellation may interrupt the awaited operation") }));
}

#[test]
fn v03_19_unknown_async_context_keeps_exit_cancellation_visible() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return False\nasync def cleanup():\n    resource = C()\n    async with UnknownManager():\n        pass\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
        }),
        "an unknown asynchronous exit can be interrupted: {result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_19_unknown_async_exit_can_suppress_body_cancellation() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    async with UnknownManager():\n        await resource.pause()\n        resource.close()\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 4, "{report:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty == "possible"),
        "unknown entry/exit effects retain possible duties: {report:#?}"
    );
    let completions = leaks
        .iter()
        .filter_map(|diagnostic| diagnostic.completion.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        completions,
        std::collections::BTreeSet::from([
            "cancelled".to_owned(),
            "fallthrough".to_owned(),
            "raise".to_owned(),
        ]),
        "{report:#?}"
    );
    let fallthrough = leaks
        .iter()
        .find(|diagnostic| diagnostic.completion.as_deref() == Some("fallthrough"))
        .expect("unknown async exit may suppress cancellation and skip body cleanup");
    assert_eq!(fallthrough.primary.line, 9, "{report:#?}");
    assert_eq!(
        fallthrough
            .related
            .iter()
            .map(|span| span.line)
            .collect::<Vec<_>>(),
        [10, 13],
        "{report:#?}"
    );
    let raised = leaks
        .iter()
        .find(|diagnostic| diagnostic.completion.as_deref() == Some("raise"))
        .expect("manager construction can fail before the body");
    assert_eq!(raised.primary.line, 11, "{report:#?}");
    assert_eq!(
        raised
            .related
            .iter()
            .map(|span| span.line)
            .collect::<Vec<_>>(),
        [10, 13],
        "{report:#?}"
    );
    let cancelled = leaks
        .iter()
        .filter(|diagnostic| diagnostic.completion.as_deref() == Some("cancelled"))
        .collect::<Vec<_>>();
    assert_eq!(cancelled.len(), 2, "{report:#?}");
    let entry_cancel = cancelled
        .iter()
        .find(|diagnostic| diagnostic.primary.line == 11)
        .expect("entry/exit suspension may be cancelled");
    assert_eq!(
        entry_cancel
            .related
            .iter()
            .map(|span| span.line)
            .collect::<Vec<_>>(),
        [10, 13],
        "{report:#?}"
    );
    let body_cancel = cancelled
        .iter()
        .find(|diagnostic| diagnostic.primary.line == 12)
        .expect("body await may be cancelled before the later close");
    assert_eq!(body_cancel.related.len(), 1, "{report:#?}");
    assert_eq!(body_cancel.related[0].line, 10, "{report:#?}");
}

#[test]
fn v03_19_unknown_async_context_keeps_duties_across_return_break_and_continue() {
    let source = format!(
        "{RESOURCE}class UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def return_path():\n    resource = C()\n    async with UnknownManager():\n        return\nasync def break_path():\n    resource = C()\n    for item in (None,):\n        async with UnknownManager():\n            break\nasync def continue_path():\n    resource = C()\n    for item in (None,):\n        async with UnknownManager():\n            continue\n"
    );
    let acquisition_lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "resource = C()").then_some(index + 1))
        .collect::<std::collections::BTreeSet<_>>();
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 9, "{report:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty == "possible"),
        "an unknown async manager cannot prove cleanup on any transfer: {report:#?}"
    );
    let completions = leaks.iter().fold(
        std::collections::BTreeMap::<&str, Vec<_>>::new(),
        |mut grouped, diagnostic| {
            grouped
                .entry(diagnostic.completion.as_deref().unwrap_or_default())
                .or_default()
                .push(*diagnostic);
            grouped
        },
    );
    assert_eq!(
        completions.get("return").map(Vec::len),
        Some(1),
        "{report:#?}"
    );
    assert_eq!(
        completions.get("fallthrough").map(Vec::len),
        Some(2),
        "break and continue reach the enclosing async function exit: {report:#?}"
    );
    assert_eq!(
        completions.get("raise").map(Vec::len),
        Some(3),
        "{report:#?}"
    );
    assert_eq!(
        completions.get("cancelled").map(Vec::len),
        Some(3),
        "async manager entry and exit cancellation outcomes remain visible: {report:#?}"
    );
    for (completion, expected_lines) in [
        ("return", std::collections::BTreeSet::from([12])),
        ("fallthrough", std::collections::BTreeSet::from([13, 18])),
        ("raise", std::collections::BTreeSet::from([11, 16, 21])),
        ("cancelled", std::collections::BTreeSet::from([11, 16, 21])),
    ] {
        let primary_lines = completions[completion]
            .iter()
            .map(|diagnostic| diagnostic.primary.line)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(primary_lines, expected_lines, "{completion}: {report:#?}");
    }
    let reported_acquisitions = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(reported_acquisitions, acquisition_lines, "{report:#?}");
    assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
}

#[test]
fn v03_19_reviewed_async_context_cleanup_runs_on_return_break_and_continue() {
    let source = "from contextlib import aclosing\nfrom typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\nasync def return_path():\n    resource = C()\n    async with aclosing(resource):\n        return\nasync def break_path():\n    resource = C()\n    for item in (None,):\n        async with aclosing(resource):\n            break\nasync def continue_path():\n    resource = C()\n    for item in (None,):\n        async with aclosing(resource):\n            continue\n";
    let acquisition_lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "resource = C()").then_some(index + 1))
        .collect::<std::collections::BTreeSet<_>>();
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 3, "{report:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty == "possible"),
        "reviewed async cleanup can be interrupted on every transfer: {report:#?}"
    );
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.completion.as_deref() == Some("cancelled")),
        "completed aclose discharges normal return/break/continue paths: {report:#?}"
    );
    let reported_acquisitions = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(reported_acquisitions, acquisition_lines, "{report:#?}");
    assert_eq!(
        report.coverage.status, "incomplete",
        "awaited aclose can be interrupted by cancellation: {report:#?}"
    );
}

#[test]
fn v03_19_nested_async_contexts_unwind_and_preserve_cancellation() {
    let source = "from contextlib import aclosing\nfrom typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\n    async def pause(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    async with UnknownManager():\n        async with aclosing(resource):\n            await resource.pause()\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
                && diagnostic
                    .related
                    .first()
                    .is_some_and(|span| span.line == 11)
        }),
        "the nested exit retains a cancellation propagation path: {result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on fallthrough")
                && diagnostic.related.first().is_some_and(|span| span.line == 11)
        }),
        "the outer unknown exit may suppress cancellation after inner cleanup is interrupted: {result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_16_context_exit_partition_budget_keeps_duties_unknown() {
    let managers = ["UnknownManager()"; 8].join(", ");
    let source = format!(
        "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    async with {managers}:\n        pass\n"
    );
    let result = check_source(&source);

    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("context-manager exit partition budget exceeded") }),
        "the dedicated path bound must report visible incomplete coverage: {result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "widening must preserve the outstanding duty as possible: {result:#?}"
    );
    assert!(
        !result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }),
        "truncated cleanup paths cannot establish a definite result: {result:#?}"
    );
}

#[test]
fn v03_26_coroutine_and_async_exit_budgets_keep_cleanup_unknown_together() {
    let managers = ["UnknownManager()"; 8].join(", ");
    let mut source = "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def hold(resource: Borrowed[C]):\n    await resource.pause()\nasync def cleanup():\n    resource = C()\n".to_owned();
    for index in 0..129 {
        source.push_str(&format!("    pending_{index} = hold(resource)\n"));
    }
    source.push_str(&format!("    async with {managers}:\n        pass\n"));

    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("pending coroutine budget exceeded") }),
        "the lost capture must remain visible: {result:#?}"
    );
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("context-manager exit partition budget exceeded") }),
        "the context unwind truncation must remain visible: {result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "the combined boundary must retain an uncertain duty: {result:#?}"
    );
    assert!(!result
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.code == "TP1002" && diagnostic.certainty == "definite" }));
}

#[test]
fn v03_16_owned_parameter_budget_reports_untracked_duties_as_incomplete() {
    let parameters = (0..=4_096)
        .map(|index| format!("resource_{index}: Owned[C]"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("{RESOURCE}def cleanup({parameters}):\n    pass\n");
    let result = check_source(&source);

    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("tracked resource object budget exceeded") }),
        "the untracked Owned input needs a visible budget boundary: {result:#?}"
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .count(),
        4_096,
        "all obligations within the budget remain individually visible: {result:#?}"
    );
}

#[test]
fn v03_16_context_exit_partition_budget_edit_recovers_with_fresh_result_parity() {
    let managers = ["UnknownManager()"; 8].join(", ");
    let over_budget = SourceFile::new(
        "phase03.py",
        "phase03",
        format!(
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return True\nasync def cleanup():\n    resource = C()\n    async with {managers}:\n        pass\n"
        ),
    );
    let mut session = AnalysisSession::default();
    let first = session.check(std::slice::from_ref(&over_budget), "3.11");
    assert!(
        first
            .coverage
            .reasons
            .iter()
            .any(|reason| { reason.contains("context-manager exit partition budget exceeded") }),
        "the first revision must expose the exhausted partition budget: {first:#?}"
    );

    let recovered_file = SourceFile::new(
        "phase03.py",
        "phase03",
        "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n",
    );
    let recovered = session.check(std::slice::from_ref(&recovered_file), "3.11");
    assert_eq!(
        recovered,
        check(std::slice::from_ref(&recovered_file), "3.11")
    );
    assert_eq!(
        recovered.coverage.status, "complete-for-supported-subset",
        "a later revision must not retain the prior budget boundary: {recovered:#?}"
    );
    assert!(!recovered
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("context-manager exit partition budget exceeded") }));
}

#[test]
fn v03_19_unknown_async_entry_cancellation_skips_the_body() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, exc_type, exc, traceback): return False\nasync def cleanup():\n    before = C()\n    async with UnknownManager():\n        before.close()\n        local = C()\n        local.close()\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
                && diagnostic.related.first().is_some_and(|span| span.line == 9)
        }),
        "cancellation during an unknown entry must retain the caller duty without acquiring body locals: {result:#?}"
    );
    assert!(result.diagnostics.iter().all(|diagnostic| {
        diagnostic.code != "TP1002"
            || !diagnostic.message.ends_with("on cancelled")
            || diagnostic
                .related
                .first()
                .is_some_and(|span| span.line == 9)
    }));
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_21_contextlib_model_is_disabled_for_unlisted_python_targets() {
    for source in [
        "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n",
        "import contextlib as ctx\nfrom typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with ctx.closing(resource):\n        pass\n",
    ] {
        let result = check_source_version(source, "3.14");
        assert!(
            result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
            }),
            "{result:#?}"
        );
        assert_eq!(result.coverage.status, "incomplete");
        let report = typepact::CheckResult::from_inspection(result, "3.14");
        assert!(report.available_models.is_empty());
        assert_eq!(report.model_catalog_version, 2);
    }
}

#[test]
fn v03_21_missing_python_version_input_disables_contextlib_models() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n";
    let result = check_source_version(source, "");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.code == "TP1002" && diagnostic.certainty != "definite" }));
    let report = typepact::CheckResult::from_inspection(result, "");
    assert!(report.available_models.is_empty());
}

#[test]
fn v03_22_incremental_body_edit_matches_a_fresh_check() {
    let mut session = typepact::AnalysisSession::default();
    for body in ["pass", "resource.close()", "raise RuntimeError"] {
        let source = format!("{RESOURCE}def cleanup():\n    resource = C()\n    {body}\n");
        let files = [SourceFile::new("phase03.py", "phase03", source)];
        let updated = session.check(&files, "3.11");
        assert_eq!(updated, check(&files, "3.11"));
    }
}

#[test]
fn v03_10_contextlib_closing_preserves_a_checked_close_raise() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self):\n        raise RuntimeError()\ndef cleanup():\n    resource = C()\n    try:\n        with closing(resource):\n            pass\n    except RuntimeError:\n        pass\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_16_unknown_effect_cannot_improve_cleanup_certainty() {
    let closed = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    resource.close()\n"
    ));
    let widened = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    opaque_effect(resource)\n"
    ));
    assert!(
        !closed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{closed:#?}"
    );
    assert!(
        widened.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{widened:#?}"
    );
    assert_eq!(widened.coverage.status, "incomplete", "{widened:#?}");
}

#[test]
fn v03_18_nested_awaited_cleanup_keeps_cancellation_uncertain() {
    let source = "from typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\n    async def pause(self): pass\nasync def cleanup():\n    resource = C()\n    try:\n        await resource.pause()\n    finally:\n        try:\n            await resource.pause()\n        finally:\n            await resource.aclose()\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "possible"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("cancellation may interrupt the awaited operation")));
}

#[test]
fn v03_26_closing_a_capture_does_not_complete_its_pending_coroutine() {
    let source = "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nasync def use(resource: Borrowed[C]):\n    await resource.pause()\ndef cleanup():\n    resource = C()\n    pending = use(resource)\n    resource.close()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && (diagnostic.message.contains("created coroutine")
                    || diagnostic.message.contains("captured resource"))
        }),
        "{result:#?}"
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn v03_26_pending_capture_across_async_context_exit_keeps_cancellation_visible() {
    let source = "from contextlib import aclosing\nfrom typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by=\"aclose\")\nclass C:\n    async def aclose(self): pass\n    async def pause(self): pass\nasync def use(resource: Borrowed[C]):\n    await resource.pause()\nasync def cleanup():\n    resource = C()\n    pending = use(resource)\n    async with aclosing(resource):\n        await pending\n";
    let result = check_source(source);

    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("created coroutine is not awaited")
                && diagnostic.primary.line == 12
        }),
        "cancellation before async-context body entry must retain the pending capture: {result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on cancelled")
                && diagnostic.primary.line == 13
                && diagnostic
                    .related
                    .first()
                    .is_some_and(|span| span.line == 11)
        }),
        "interrupted aclosing must leave the captured resource duty possible: {result:#?}"
    );
    assert!(
        !result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty == "definite"
        }),
        "a pending capture and interruptible async cleanup cannot establish a definite leak: {result:#?}"
    );
}

#[test]
fn v03_26_unknown_actor_across_suspension_keeps_cleanup_uncertain() {
    let source = "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nasync def use(resource: Borrowed[C]):\n    opaque_effect(resource)\n    await resource.pause()\nasync def cleanup():\n    resource = C()\n    unrelated = C()\n    try:\n        await use(resource)\n    finally:\n        resource.close()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.related.iter().any(|span| span.line == 11)
        }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "definite"
                && diagnostic.related.iter().any(|span| span.line == 12)
        }),
        "the unknown actor affects the coroutine capture, not the unrelated duty: {result:#?}"
    );
    assert!(
        result.coverage.reasons.iter().any(|reason| {
            reason.contains("opaque") || reason.contains("cancellation may interrupt")
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_09_owned_helper_raise_reports_one_callee_obligation() {
    let result = check_source(&format!(
        "{RESOURCE}def fail(resource: Owned[C]):\n    raise RuntimeError()\ndef caller():\n    resource = C()\n    try:\n        fail(resource)\n    except RuntimeError:\n        pass\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert!(leaks[0].message.ends_with("on raise"), "{result:#?}");
    assert_eq!(leaks[0].primary.line, 7, "{result:#?}");
}

#[test]
fn v03_10_contextlib_close_raise_replaces_body_raise_without_discharging() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self):\n        raise OSError()\ndef cleanup():\n    resource = C()\n    try:\n        with closing(resource):\n            raise ValueError()\n    except OSError:\n        pass\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("fallthrough"));
    assert_eq!(leaks[0].primary.line, 7, "{report:#?}");
    assert_eq!(leaks[0].related.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].related[0].line, 8, "{report:#?}");
    assert_eq!(report.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_10_unknown_suppression_cannot_hide_the_resource_exit() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef cleanup():\n    resource = C()\n    with UnknownManager():\n        raise RuntimeError()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
}

#[test]
fn v03_10_unknown_suppression_keeps_propagation_and_continuation_paths() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef cleanup():\n    before = C()\n    with UnknownManager():\n        raise RuntimeError()\n    after = C()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(
        leaks.iter().any(|diagnostic| {
            diagnostic.certainty == "possible" && diagnostic.message.ends_with("on raise")
        }),
        "the unsuppressed exception path must remain feasible: {result:#?}"
    );
    let after_leaks = leaks
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .related
                .first()
                .is_some_and(|span| span.line == 12)
        })
        .collect::<Vec<_>>();
    assert_eq!(after_leaks.len(), 1, "{result:#?}");
    assert_eq!(after_leaks[0].certainty, "definite", "{result:#?}");
    assert!(
        after_leaks[0].message.ends_with("on fallthrough"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_10_unknown_outer_suppression_can_continue_after_inner_entry_failure() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef fail() -> C:\n    raise RuntimeError()\ndef cleanup():\n    outer = C()\n    with closing(outer), UnknownManager(), closing(fail()):\n        pass\n    after = C()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    let after_leaks = leaks
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .related
                .first()
                .is_some_and(|span| span.line == 15)
        })
        .collect::<Vec<_>>();
    assert_eq!(after_leaks.len(), 1, "{result:#?}");
    assert_eq!(after_leaks[0].certainty, "definite", "{result:#?}");
    assert!(
        after_leaks[0].message.ends_with("on fallthrough"),
        "{result:#?}"
    );
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.related[0].line != 12));
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_10_unknown_outer_suppression_can_continue_after_inner_cleanup_failure() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self):\n        raise RuntimeError()\nclass UnknownManager:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\ndef cleanup():\n    resource = C()\n    with UnknownManager(), closing(resource):\n        pass\n    after = C()\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(
        leaks.iter().any(|diagnostic| {
            diagnostic.message.ends_with("on raise")
                && diagnostic.primary.line == 12
                && diagnostic
                    .related
                    .first()
                    .is_some_and(|span| span.line == 11)
        }),
        "the cleanup exception must remain propagatable: {result:#?}"
    );
    assert!(
        leaks.iter().any(|diagnostic| {
            diagnostic.message.ends_with("on fallthrough")
                && diagnostic
                    .related
                    .first()
                    .is_some_and(|span| span.line == 14)
        }),
        "the outer manager may suppress the inner cleanup exception: {result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_20_taskgroup_and_executor_handoffs_remain_unknown() {
    let task_group = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\nasync def finish(resource):\n    resource.close()\nasync def caller():\n    resource = C()\n    pending = finish(resource)\n    async with asyncio.TaskGroup() as group:\n        group.create_task(pending)\n";
    let task_group_result = check_source(task_group);
    assert_eq!(task_group_result.coverage.status, "incomplete");
    assert!(
        task_group_result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{task_group_result:#?}"
    );

    for executor in ["ThreadPoolExecutor", "ProcessPoolExecutor"] {
        let source = format!(
            "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef consume(resource):\n    resource.close()\ndef caller():\n    resource = C()\n    pool = {executor}()\n    pool.submit(consume, resource)\n"
        );
        let result = check_source(&source);
        assert_eq!(
            result.coverage.status, "incomplete",
            "{executor}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "TP9001" || diagnostic.code == "TP9002" }),
            "{executor}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "{executor}: {result:#?}"
        );
    }
}

#[test]
fn v03_24_custom_awaitable_does_not_infer_coroutine_execution() {
    let source = "from typepact import resource\n@resource(closed_by='aclose')\nclass C:\n    async def aclose(self): pass\nclass CustomAwaitable:\n    def __await__(self): pass\nasync def cleanup():\n    resource = C()\n    await CustomAwaitable()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" || diagnostic.code == "TP9002" }),
        "{result:#?}"
    );
}

#[test]
fn v03_24_generator_and_except_star_flow_remain_visible_boundaries() {
    for (name, source) in [
        (
            "generator",
            format!("{RESOURCE}def cleanup():\n    resource = C()\n    yield resource\n"),
        ),
        (
            "except-star",
            format!("{RESOURCE}def cleanup():\n    resource = C()\n    try:\n        raise ExceptionGroup('failure', [RuntimeError()])\n    except* RuntimeError:\n        resource.close()\n"),
        ),
    ] {
        let result = check_source(&source);
        assert_eq!(result.coverage.status, "incomplete", "{name}: {result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP9001"),
            "{name}: {result:#?}"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP1002"),
            "{name}: {result:#?}"
        );
    }
}

#[test]
fn v03_13_callback_escape_keeps_captured_cleanup_unknown() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef leak(register):\n    resource = C()\n    register(lambda: resource.close())\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002" && diagnostic.certainty != "definite"
        }),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("unsupported") || reason.contains("unknown")));
}

#[test]
fn v03_13_multiple_callback_alias_escapes_keep_each_duty_uncertain() {
    let source = format!(
        "{RESOURCE}def leak(register):\n    first = C()\n    second = C()\n    first_alias = first\n    second_alias = second\n    register(lambda: first_alias.close())\n    register(lambda: second_alias.close())\n"
    );
    let first_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "first = C()")
        .unwrap()
        + 1;
    let second_acquisition_line = source
        .lines()
        .position(|line| line.trim() == "second = C()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert!(!leaks.is_empty(), "{result:#?}");
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty != "definite"),
        "escaped callbacks cannot prove when or whether captured cleanup runs: {result:#?}"
    );
    let acquisition_lines = leaks
        .iter()
        .flat_map(|diagnostic| diagnostic.related.iter().map(|span| span.line))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        acquisition_lines.contains(&first_acquisition_line),
        "{result:#?}"
    );
    assert!(
        acquisition_lines.contains(&second_acquisition_line),
        "{result:#?}"
    );
}

#[test]
fn v03_13_escaped_bound_method_keeps_its_captured_duty_uncertain() {
    let source = format!(
        "{RESOURCE}def leak(register):\n    resource = C()\n    register(resource.close)\n"
    );
    let acquisition_line = source
        .lines()
        .position(|line| line.trim() == "resource = C()")
        .unwrap()
        + 1;
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert_eq!(
        leaks.len(),
        2,
        "unknown registration may return or raise: {result:#?}"
    );
    assert!(
        leaks
            .iter()
            .all(|diagnostic| diagnostic.certainty == "possible"),
        "{result:#?}"
    );
    assert!(leaks.iter().all(|diagnostic| {
        diagnostic
            .related
            .iter()
            .any(|span| span.line == acquisition_line)
    }));
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on fallthrough")));
    assert!(leaks
        .iter()
        .any(|diagnostic| diagnostic.message.ends_with("on raise")));
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("unknown") || reason.contains("unsupported") }));
}

#[test]
fn v03_15_nonterminating_loop_is_not_a_function_exit_witness() {
    let result = check_source(&format!(
        "{RESOURCE}def never_returns():\n    resource = C()\n    while True:\n        pass\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
}

#[test]
fn v03_15_unreachable_while_else_cleanup_does_not_hide_a_break_exit() {
    let source = format!(
        "{RESOURCE}def cleanup(stop):\n    resource = C()\n    while True:\n        if stop:\n            break\n        continue\n    else:\n        resource.close()\n"
    );
    let result = check_source(&source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(
        leaks.len(),
        1,
        "the break exits without running loop-else: {result:#?}"
    );
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related.len(), 1, "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_15_unconditional_continue_does_not_create_a_function_exit() {
    let result = check_source(&format!(
        "{RESOURCE}def never_returns():\n    resource = C()\n    while True:\n        try:\n            continue\n        finally:\n            pass\n    else:\n        resource.close()\n"
    ));
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "a loop with no break has no cleanup exit witness: {result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_01_each_open_obligation_is_reported_at_each_distinct_return_exit() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(flag):\n    first = C()\n    second = C()\n    if flag:\n        return\n    first.close()\n    return\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 3, "{result:#?}");
    assert!(leaks.iter().all(|diagnostic| {
        diagnostic.certainty == "definite"
            && diagnostic.message.ends_with("on return")
            && diagnostic.related.len() == 1
    }));
    let acquisition_lines = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<Vec<_>>();
    assert_eq!(acquisition_lines, [7, 8, 8], "{result:#?}");
}

#[test]
fn v03_02_only_explicit_owned_parameters_create_incoming_duties() {
    let result = check_source(&format!(
        "{RESOURCE}def owns(resource: Owned[C]):\n    pass\ndef borrows(resource: Borrowed[C]):\n    pass\ndef observes(resource: C):\n    pass\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related.len(), 1, "{result:#?}");
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_03_nested_cleanup_raise_still_runs_outer_cleanup_once() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n@resource(closed_by='close')\nclass Failing:\n    def close(self):\n        raise OSError()\ndef cleanup():\n    outer = C()\n    inner = Failing()\n    try:\n        try:\n            raise ValueError()\n        finally:\n            inner.close()\n    finally:\n        outer.close()\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("raise"));
    assert_eq!(leaks[0].primary.line, 16, "{report:#?}");
    assert_eq!(leaks[0].related.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].related[0].line, 11, "{report:#?}");
    assert_eq!(report.coverage.status, "complete-for-supported-subset");
}

#[test]
fn v03_08_ambiguous_return_alias_keeps_the_local_duty_and_unknown_boundary() {
    let result = check_source(&format!(
        "{RESOURCE}def escape(flag):\n    resource = C()\n    if flag:\n        result = resource\n    else:\n        result = unknown_factory()\n    return result\n"
    ));
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "definite"
                && diagnostic.message.ends_with("on return")
        }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001" && diagnostic.message.contains("resource value escapes")
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v03_06_check_report_retains_each_return_and_raise_completion() {
    let report = typepact::check_report(
        &[SourceFile::new(
            "phase03.py",
            "phase03",
            format!(
                "{RESOURCE}def leak(flag):\n    resource = C()\n    if flag:\n        return\n    raise RuntimeError()\n"
            ),
        )],
        "3.11",
    );
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{report:#?}");
    let completions = leaks
        .iter()
        .map(|diagnostic| diagnostic.completion.as_deref().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        completions,
        std::collections::BTreeSet::from(["raise", "return"])
    );
    assert!(leaks.iter().all(|diagnostic| {
        diagnostic.certainty == "definite"
            && diagnostic.related.len() == 1
            && diagnostic.related[0].line == 7
    }));
}

#[test]
fn v03_14_conditional_loop_cleanup_keeps_the_zero_iteration_witness() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(flag, close_now):\n    resource = C()\n    while flag:\n        if close_now:\n            resource.close()\n        break\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{result:#?}");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_18_exception_handler_does_not_swallow_cancellation() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nasync def cleanup():\n    resource = C()\n    try:\n        await resource.pause()\n    except Exception:\n        resource.close()\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.certainty == "possible"
                && diagnostic.completion.as_deref() == Some("cancelled")
        }),
        "{report:#?}"
    );
    assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
}

#[test]
fn v03_18_base_exception_handler_catches_cancellation_but_keeps_state_unknown() {
    let source = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    async def pause(self): pass\nasync def cleanup():\n    resource = C()\n    try:\n        await resource.pause()\n    except BaseException:\n        resource.close()\n    else:\n        resource.close()\n";
    let report =
        typepact::check_report(&[SourceFile::new("phase03.py", "phase03", source)], "3.11");
    let leaks = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{report:#?}");
    assert_eq!(leaks[0].certainty, "possible", "{report:#?}");
    assert_eq!(leaks[0].completion.as_deref(), Some("fallthrough"));
    assert_eq!(leaks[0].primary.line, 6, "{report:#?}");
    assert_eq!(
        leaks[0]
            .related
            .iter()
            .map(|span| span.line)
            .collect::<Vec<_>>(),
        [7, 11],
        "caught cancellation continues, but suspension leaves the resource state uncertain: {report:#?}"
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TP9001"
            && diagnostic
                .message
                .contains("cancellation may interrupt the awaited operation")
    }));
    assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
}

#[test]
fn v03_02_shorthand_and_verbose_parameter_access_forms_share_duty_semantics() {
    let source = "from typing import Annotated\nfrom typepact import resource\nfrom typepact.rust import Owned, Borrowed\nfrom typepact.contracts import OwnedAccess, Resource, SharedBorrow\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef owned_short(value: Owned[C]):\n    pass\ndef owned_verbose(value: Annotated[C, OwnedAccess()]):\n    pass\ndef borrowed_short(value: Borrowed[C]):\n    pass\ndef borrowed_verbose(value: Annotated[C, SharedBorrow()]):\n    pass\ndef ordinary(value: C):\n    pass\nclass D:\n    def close(self): pass\nDResource = Annotated[D, Resource(closed_by='close')]\ndef explicit_owned_short(value: Owned[DResource]):\n    pass\ndef explicit_owned_verbose(value: Annotated[DResource, OwnedAccess()]):\n    pass\ndef explicit_borrowed(value: Borrowed[DResource]):\n    pass\n";
    let result = check_source(source);
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 4, "{result:#?}");
    assert!(leaks
        .iter()
        .all(|diagnostic| diagnostic.certainty == "definite"
            && diagnostic.message.ends_with("on fallthrough")));
    let origins = leaks
        .iter()
        .map(|diagnostic| diagnostic.related[0].line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        origins,
        std::collections::BTreeSet::from([8, 10, 21, 23]),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_02_borrowed_entry_does_not_hide_a_local_acquisition_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def leak(value: Borrowed[C]):\n    local = C()\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "definite", "{result:#?}");
    assert!(leaks[0].message.ends_with("on fallthrough"), "{result:#?}");
    assert_eq!(leaks[0].related.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].related[0].line, 7, "{result:#?}");
}

#[test]
fn v03_02_borrowed_alias_cannot_be_closed_directly_or_through_a_wrapper() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef direct(resource: Borrowed[C]):\n    alias = resource\n    alias.close()\ndef wrapped(resource: Borrowed[C]):\n    alias = resource\n    with closing(alias):\n        pass\n"
    ));
    let borrowed_closes = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP2001")
        .collect::<Vec<_>>();
    assert_eq!(borrowed_closes.len(), 2, "{result:#?}");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "borrowed aliases create no owner duty: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_06_branch_paths_sharing_one_return_are_grouped_per_duty() {
    let result = check_source(&format!(
        "{RESOURCE}def cleanup(flag):\n    first = C()\n    second = C()\n    if flag:\n        first.close()\n    return\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(
        leaks.iter().all(|diagnostic| {
            diagnostic.message.ends_with("on return") && diagnostic.primary.line == 11
        }),
        "{result:#?}"
    );
    assert!(
        leaks.iter().any(|diagnostic| {
            diagnostic.certainty == "possible"
                && diagnostic.related.len() == 2
                && diagnostic.related[0].line == 7
                && diagnostic.related[1].line == 10
        }),
        "{result:#?}"
    );
    assert!(
        leaks.iter().any(|diagnostic| {
            diagnostic.certainty == "definite"
                && diagnostic.related.len() == 1
                && diagnostic.related[0].line == 8
        }),
        "{result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_06_branch_paths_sharing_one_raise_are_grouped_per_duty() {
    let source = format!(
        "{RESOURCE}def cleanup(flag):\n    first = C()\n    second = C()\n    if flag:\n        first.close()\n    else:\n        second.close()\n    raise RuntimeError()\n"
    );
    let result = check_source(&source);
    let raise_line = source
        .lines()
        .position(|line| line.trim() == "raise RuntimeError()")
        .unwrap()
        + 1;
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 2, "{result:#?}");
    assert!(
        leaks.iter().all(|diagnostic| {
            diagnostic.certainty == "possible"
                && diagnostic.message.ends_with("on raise")
                && diagnostic.primary.line == raise_line
                && diagnostic.related.len() == 2
        }),
        "{result:#?}"
    );
    let related_lines = leaks
        .iter()
        .map(|diagnostic| {
            diagnostic
                .related
                .iter()
                .map(|span| span.line)
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        related_lines,
        std::collections::BTreeSet::from([vec![7, 10], vec![8, 12]]),
        "each abstract duty keeps only its own acquisition and close witnesses: {result:#?}"
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v03_16_generated_finite_branch_paths_match_the_exit_oracle() {
    // Enumerate every assignment to the generated branch predicates. Each
    // true branch closes the resource, so exactly one path remains open: the
    // all-false assignment. All paths share the same fallthrough exit.
    for branch_count in 1..=5 {
        let arguments = (0..branch_count)
            .map(|index| format!("flag_{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut source = format!("{RESOURCE}def cleanup({arguments}):\n    resource = C()\n");
        let mut indentation = 4;
        for index in 0..branch_count {
            let current_indent = " ".repeat(indentation);
            let body_indent = " ".repeat(indentation + 4);
            source.push_str(&format!(
                "{current_indent}if flag_{index}:\n{body_indent}resource.close()\n"
            ));
            if index + 1 < branch_count {
                source.push_str(&format!("{current_indent}else:\n"));
                indentation += 4;
            }
        }

        let path_count = 1usize << branch_count;
        let closed_paths = (0..path_count)
            .filter(|assignment| *assignment != 0)
            .count();
        let open_paths = path_count - closed_paths;
        assert_eq!((closed_paths, open_paths), (path_count - 1, 1));

        let result = check_source(&source);
        let leaks = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(leaks.len(), 1, "branches={branch_count}: {result:#?}");
        assert_eq!(
            leaks[0].certainty, "possible",
            "branches={branch_count}: {result:#?}"
        );
        assert!(
            leaks[0].message.ends_with("on fallthrough"),
            "branches={branch_count}: {result:#?}"
        );
        assert_eq!(
            leaks[0].related[0].line, 7,
            "branches={branch_count}: {result:#?}"
        );
        assert_eq!(
            result.coverage.status, "complete-for-supported-subset",
            "branches={branch_count}: {result:#?}"
        );

        let unknown_effect = source.replacen("resource.close()", "opaque_effect(resource)", 1);
        let widened = check_source(&unknown_effect);
        let widened_leak = widened
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "TP1002")
            .expect("widening one mutually-exclusive close must retain the duty");
        assert_eq!(
            widened_leak.certainty, "possible",
            "branches={branch_count}: {widened:#?}"
        );
        assert_eq!(
            widened.coverage.status, "incomplete",
            "branches={branch_count}: {widened:#?}"
        );
    }
}

#[test]
fn v03_16_generated_static_loop_paths_preserve_zero_and_repeated_iteration_cases() {
    // A finite static tuple has a known number of iterations. At length zero,
    // no cleanup runs. For positive lengths, the stable flag has an open path
    // and a close path; repeated closes stay incomplete without a model.
    for iteration_count in 0..=16 {
        let items = (0..iteration_count)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let iterable = if iteration_count == 1 {
            format!("({items},)")
        } else {
            format!("({items})")
        };
        let source = format!(
            "{RESOURCE}def cleanup(flag):\n    resource = C()\n    for item in {iterable}:\n        if flag:\n            resource.close()\n"
        );
        let known_closed_paths = usize::from(iteration_count == 1);
        let unknown_paths = usize::from(iteration_count > 1);
        let open_paths = 1;
        let expected_certainty = if known_closed_paths + unknown_paths == 0 {
            "definite"
        } else {
            "possible"
        };

        let result = check_source(&source);
        let leaks = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1002")
            .collect::<Vec<_>>();
        assert_eq!(leaks.len(), 1, "iterations={iteration_count}: {result:#?}");
        assert_eq!(
            leaks[0].certainty,
            expected_certainty,
            "iterations={iteration_count}, open_paths={open_paths}, known_closed_paths={known_closed_paths}, unknown_paths={unknown_paths}: {result:#?}"
        );
        assert!(
            leaks[0].message.ends_with("on fallthrough"),
            "iterations={iteration_count}: {result:#?}"
        );
        if iteration_count <= 1 {
            assert_eq!(
                result.coverage.status, "complete-for-supported-subset",
                "iterations={iteration_count}: {result:#?}"
            );
        } else {
            assert_eq!(
                result.coverage.status, "incomplete",
                "iterations={iteration_count}: {result:#?}"
            );
            assert!(
                result
                    .coverage
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("repeat-close behavior is not modeled")),
                "iterations={iteration_count}: {result:#?}"
            );
        }
    }
}

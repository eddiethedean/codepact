use typepact::{check, Inspection, SourceFile};

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
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
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
fn v03_10_failed_later_context_entry_still_unwinds_earlier_managers() {
    let result = check_source(&format!(
        "{RESOURCE}from contextlib import closing\ndef cleanup(manager):\n    first = C()\n    second = C()\n    with closing(first), manager:\n        pass\n"
    ));
    let leaks = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1002")
        .collect::<Vec<_>>();
    assert_eq!(leaks.len(), 1, "{result:#?}");
    assert_eq!(leaks[0].certainty, "possible");
    assert_eq!(leaks[0].related[0].line, 9);
    assert_eq!(result.coverage.status, "incomplete");
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
fn v03_17_awaited_async_cleanup_tracks_cancellation_as_uncertain() {
    let source = "from typepact import resource\n@resource(closed_by=\"aclose\")\nclass C:\n    async def aclose(self): pass\nasync def cleanup():\n    resource = C()\n    await resource.aclose()\n";
    let result = check_source(source);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002" && diagnostic.certainty == "possible"),
        "{result:#?}"
    );
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| { reason.contains("cancellation may interrupt the awaited operation") }));
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
fn v03_21_contextlib_model_is_disabled_for_unlisted_python_targets() {
    let source = "from contextlib import closing\nfrom typepact import resource\n@resource(closed_by=\"close\")\nclass C:\n    def close(self): pass\ndef cleanup():\n    resource = C()\n    with closing(resource):\n        pass\n";
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
    assert_eq!(report.model_catalog_version, 1);
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

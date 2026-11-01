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
    assert!(leaks.iter().all(|diagnostic| diagnostic.related.len() == 1));
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
    let result = check_source(&format!(
        "{RESOURCE}def cleanup():\n    resource = C()\n    for item in unresolved_values():\n        resource.close()\n"
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP1002"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
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
    let result = check_source(source);
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

use codepact::{check, check_with_ownership, Diagnostic, Inspection, Ownership, SourceFile};

const RESOURCE: &str = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n";

fn check_source(source: &str) -> Inspection {
    check(&[SourceFile::new("main.py", "main", source)], "3.11")
}

fn findings(result: &Inspection) -> Vec<&Diagnostic> {
    result
        .diagnostics
        .iter()
        .filter(|d| d.code == "TP1001")
        .collect()
}

#[test]
fn branch_local_closed_resources_only_report_unbound_aliases_when_read() {
    let unused = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\n"
    ));
    assert_eq!(
        unused.coverage.status, "complete-for-supported-subset",
        "{unused:#?}"
    );
    assert!(
        unused
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP9001"),
        "{unused:#?}"
    );

    let rebound = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\nvalue = C()\nvalue.close()\n"
    ));
    assert_eq!(
        rebound.coverage.status, "complete-for-supported-subset",
        "{rebound:#?}"
    );
    assert!(
        rebound
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP9001"),
        "{rebound:#?}"
    );

    let read = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\nvalue.status()\n"
    ));
    assert_eq!(read.coverage.status, "incomplete", "{read:#?}");
    assert!(
        read.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP9001"),
        "{read:#?}"
    );

    for use_after_join in ["alias = value", "value = value", "ignore(value)"] {
        let source = format!(
            "{RESOURCE}def ignore(item): pass\nif flag:\n    value = C()\n    value.close()\n{use_after_join}\n"
        );
        let result = check_source(&source);
        assert!(
            result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "TP9001"
                    && diagnostic.message.contains("may be unbound on one branch")
            }),
            "{use_after_join}: {result:#?}"
        );
    }

    let nested_assignment = check_source(&format!(
        "{RESOURCE}def ignore(item): return None\nif flag:\n    value = C()\n    value.close()\nvalue = ignore(value)\n"
    ));
    assert!(
        nested_assignment.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("may be unbound on one branch")
        }),
        "{nested_assignment:#?}"
    );

    let owned_call = check_with_ownership(
        &[SourceFile::new(
            "main.py",
            "main",
            format!(
                "{RESOURCE}def ignore(item): pass\ndef expose():\n    if flag:\n        value = C()\n        value.close()\n    ignore(value)\n"
            ),
        )],
        "3.11",
        Ownership::Enabled,
    );
    assert!(
        owned_call.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("may be unbound on one branch")
        }),
        "{owned_call:#?}"
    );
    assert!(
        owned_call
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("unsupported expression")),
        "{owned_call:#?}"
    );

    let deleted_on_branch = check_with_ownership(
        &[SourceFile::new(
            "main.py",
            "main",
            format!(
                "{RESOURCE}def expose(flag):\n    value = None\n    if flag:\n        value = C()\n        value.close()\n    else:\n        del value\n    alias = value\n"
            ),
        )],
        "3.11",
        Ownership::Enabled,
    );
    assert!(
        deleted_on_branch.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("may be unbound on one branch")
        }),
        "{deleted_on_branch:#?}"
    );

    for ownership in [Ownership::Disabled, Ownership::Enabled] {
        let parameter_binding = check_with_ownership(
            &[SourceFile::new(
                "main.py",
                "main",
                format!(
                    "{RESOURCE}def expose(value, flag):\n    if flag:\n        value = C()\n        value.close()\n    alias = value\n"
                ),
            )],
            "3.11",
            ownership,
        );
        assert!(
            parameter_binding.diagnostics.iter().all(|diagnostic| {
                diagnostic.code != "TP9001"
                    || !diagnostic.message.contains("may be unbound on one branch")
            }),
            "{parameter_binding:#?}"
        );
    }

    let exception_target = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\ntry:\n    raise ValueError()\nexcept Exception as value:\n    alias = value\n"
    ));
    assert!(
        exception_target.diagnostics.iter().all(|diagnostic| {
            diagnostic.code != "TP9001"
                || !diagnostic
                    .message
                    .contains("value may be unbound on one branch")
        }),
        "{exception_target:#?}"
    );

    let non_resource_binding = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\nelse:\n    value = None\nalias = value\n"
    ));
    assert!(
        non_resource_binding
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP9001"),
        "{non_resource_binding:#?}"
    );

    for ownership in [Ownership::Disabled, Ownership::Enabled] {
        let function_rebind = check_with_ownership(
            &[SourceFile::new(
                "main.py",
                "main",
                format!(
                    "{RESOURCE}if flag:\n    value = C()\n    value.close()\ndef value(): pass\nalias = value\n"
                ),
            )],
            "3.11",
            ownership,
        );
        assert!(
            function_rebind
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "TP9001"),
            "{function_rebind:#?}"
        );
    }

    let owned_return = check_with_ownership(
        &[SourceFile::new(
            "main.py",
            "main",
            format!(
                "{RESOURCE}def expose():\n    if flag:\n        value = C()\n        value.close()\n    return value\n"
            ),
        )],
        "3.11",
        Ownership::Enabled,
    );
    assert!(
        owned_return.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("may be unbound on one branch")
        }),
        "{owned_return:#?}"
    );

    let loop_rebind = check_source(&format!(
        "{RESOURCE}if flag:\n    value = C()\n    value.close()\nfor value in (None,):\n    alias = value\n"
    ));
    assert!(
        loop_rebind
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP9001"),
        "{loop_rebind:#?}"
    );
}

#[test]
fn invalid_function_bindings_do_not_execute_the_body_or_normal_successor() {
    for (parameters, call) in [
        ("value: C, required", "finish(c)"),
        ("value: C", "finish(c, c)"),
        ("value: C", "finish(c, value=c)"),
        ("value: C", "finish(c, extra=c)"),
        ("*, value: C", "finish(c)"),
        ("value: C, /", "finish(value=c)"),
    ] {
        let source = format!("{RESOURCE}def finish({parameters}): value.close()\nc = C()\n{call}\nc.close()\nc.status()\n");
        let result = check_source(&source);
        assert!(findings(&result).is_empty(), "{call}: {result:#?}");
        assert_eq!(result.coverage.status, "incomplete", "{call}: {result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("invalid call argument binding")),
            "{call}: {result:#?}"
        );
    }
}

#[test]
fn valid_keyword_and_positional_bindings_keep_ordered_resource_effects() {
    for (parameters, call) in [
        ("value: C", "finish(value=c)"),
        ("value: C, /", "finish(c)"),
        ("*, value: C", "finish(value=c)"),
        ("value: C, /, *, flag", "finish(c, flag=True)"),
    ] {
        let source = format!(
            "{RESOURCE}def finish({parameters}): value.close()\nc = C()\n{call}\nc.status()\n"
        );
        let result = check_source(&source);
        assert_eq!(findings(&result).len(), 1, "{call}: {result:#?}");
        assert_eq!(
            findings(&result)[0].certainty,
            "definite",
            "{call}: {result:#?}"
        );
        assert_eq!(
            result.coverage.status, "complete-for-supported-subset",
            "{call}: {result:#?}"
        );
    }
}

#[test]
fn invalid_method_and_constructor_bindings_never_apply_body_effects() {
    for (method, call) in [
        ("def work(self, *, value): value.close()", "worker.work(c)"),
        (
            "def work(self, value): value.close()",
            "worker.work(c, value=c)",
        ),
        (
            "def work(self, value, /): value.close()",
            "worker.work(value=c)",
        ),
        (
            "def __init__(self, *, value): value.close()",
            "worker = Worker(c)",
        ),
        (
            "def __init__(self, value): value.close()",
            "worker = Worker(c, value=c)",
        ),
    ] {
        let initial = if method.contains("__init__") {
            ""
        } else {
            "worker = Worker()\n"
        };
        let source = format!("{RESOURCE}@resource(closed_by='close')\nclass Worker:\n    {method}\n    def close(self): pass\nc = C()\n{initial}{call}\nc.close()\nc.status()\n");
        let result = check_source(&source);
        assert!(findings(&result).is_empty(), "{call}: {result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("invalid call argument binding")),
            "{call}: {result:#?}"
        );
    }
}

#[test]
fn unpacking_variadic_and_omitted_default_bindings_remain_visible_boundaries() {
    for (parameters, call) in [
        ("value: C", "finish(*items)"),
        ("value: C", "finish(**items)"),
        ("value: C, *extras", "finish(c, c)"),
        ("value: C, **extras", "finish(c, extra=c)"),
        ("value: C, flag=external()", "finish(c)"),
    ] {
        let source = format!(
            "{RESOURCE}def finish({parameters}): value.close()\nc = C()\n{call}\nc.status()\n"
        );
        let result = check_source(&source);
        assert_eq!(result.coverage.status, "incomplete", "{call}: {result:#?}");
        assert!(findings(&result).is_empty(), "{call}: {result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message == "call argument binding is not modeled"),
            "{call}: {result:#?}"
        );
    }
}

#[test]
fn helper_effects_use_the_actual_objects_resource_identity() {
    for (operation, expected) in [("close", 0), ("release", 1)] {
        let provider = format!("from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef wrap(value):\n    temp = C()\n    value.{operation}()\n");
        let caller = "from codepact import resource\nfrom provider import wrap\n@resource(closed_by='release')\nclass C:\n    def close(self): pass\n    def release(self): pass\n    def status(self): pass\nc = C()\nwrap(c)\nc.status()\n";
        let result = check(
            &[
                SourceFile::new("caller.py", "caller", caller),
                SourceFile::new("provider.py", "provider", provider),
            ],
            "3.11",
        );
        assert_eq!(
            findings(&result).len(),
            expected,
            "{operation}: {result:#?}"
        );
        if expected == 1 {
            assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");
            assert_eq!(findings(&result)[0].primary.file, "caller.py");
        }
    }
}

#[test]
fn imported_constructor_aliases_in_helpers_use_provider_scope() {
    let files = [
        SourceFile::new("model.py", "model", "from codepact import resource\n@resource(closed_by='close')\nclass Actual:\n    def __init__(self, value): value.close()\n    def close(self): pass\n"),
        SourceFile::new("provider.py", "provider", "from model import Actual as C\ndef wrap(value):\n    temp = C(value)\n"),
        SourceFile::new("caller.py", "caller", "from codepact import resource\nfrom provider import wrap\n@resource(closed_by='close')\nclass C:\n    def __init__(self): pass\n    def close(self): pass\n    def status(self): pass\nc = C()\nwrap(c)\nc.status()\n"),
    ];
    let result = check(&files, "3.11");
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite");
    assert_eq!(findings(&result)[0].primary.file, "caller.py");
}

#[test]
fn default_constructors_reject_arguments_without_creating_normal_successors() {
    for call in ["value = C(c)", "value = C(extra=c)"] {
        let result = check_source(&format!(
            "{RESOURCE}c = C()\n{call}\nc.close()\nc.status()\n"
        ));
        assert!(findings(&result).is_empty(), "{result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("invalid call argument binding")),
            "{result:#?}"
        );
    }
}

#[test]
fn signature_edits_refresh_incremental_call_effects() {
    let mut session = codepact::AnalysisSession::default();
    let caller = SourceFile::new(
        "caller.py",
        "caller",
        "from provider import C, finish\nc = C()\nfinish(c)\nc.status()\n",
    );
    for parameters in ["value: C", "*, value: C", "value: C"] {
        let provider = SourceFile::new(
            "provider.py",
            "provider",
            format!("{RESOURCE}def finish({parameters}): value.close()\n"),
        );
        let files = [caller.clone(), provider];
        let updated = session.check(&files, "3.11");
        assert_eq!(updated, check(&files, "3.11"));
        let expected = usize::from(!parameters.starts_with('*'));
        assert_eq!(findings(&updated).len(), expected, "{updated:#?}");
    }
}

fn receiver_source(first: &str, second: &str, suffix: &str) -> String {
    format!("{RESOURCE}@resource(closed_by='close')\nclass A:\n    def close(self): pass\n    def work(self, c):\n{first}\n@resource(closed_by='close')\nclass B:\n    def close(self): pass\n    def work(self, c):\n{second}\na = A()\nb = B()\nc = C()\nif flag:\n    selected = a\nelse:\n    selected = b\nselected.work(c)\n{suffix}")
}

#[test]
fn receiver_alternatives_execute_from_the_same_incoming_state() {
    let source = receiver_source("        c.close()", "        c.status()", "");
    let result = check_source(&source);
    assert!(findings(&result).is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete");

    let result = check_source(&receiver_source(
        "        c.close()",
        "        c.status()",
        "c.status()\n",
    ));
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "possible");
    assert_eq!(
        findings(&result)[0].primary.line,
        source.lines().count() + 1
    );

    let result = check_source(&receiver_source(
        "        c.close()",
        "        c.close()",
        "c.status()\n",
    ));
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite");
}

#[test]
fn raising_receiver_alternatives_are_excluded_from_normal_continuations() {
    for (first, second, expected_coverage) in [
        (
            "        raise RuntimeError()",
            "        raise RuntimeError()",
            "incomplete",
        ),
        (
            "        c.close()\n        raise RuntimeError()",
            "        c.status()",
            "incomplete",
        ),
        (
            "        c.status()",
            "        c.close()\n        raise RuntimeError()",
            "incomplete",
        ),
    ] {
        let suffix = if first.contains("raise") && second.contains("raise") {
            "c.close()\nc.status()\n"
        } else {
            "c.status()\n"
        };
        let result = check_source(&receiver_source(first, second, suffix));
        assert!(findings(&result).is_empty(), "{result:#?}");
        assert_eq!(result.coverage.status, expected_coverage, "{result:#?}");
    }
}

#[test]
fn raising_expressions_stop_returns_conditions_arguments_and_initializers() {
    for (body, expected_coverage) in [
        (
            "def wrapper(): return fail()\nwrapper()\nc.close()\nc.status()\n",
            "incomplete",
        ),
        (
            "if fail():\n    c.close()\nc.close()\nc.status()\n",
            "incomplete",
        ),
        (
            "def use(first, second): pass\nc.close()\nuse(fail(), c.status())\n",
            "complete-for-supported-subset",
        ),
        (
            "@resource(closed_by='close')\nclass Worker:\n    def __init__(self, first, second): second.status()\n    def close(self): pass\nc.close()\nw = Worker(fail(), c)\n",
            "incomplete",
        ),
    ] {
        let result = check_source(&format!("{RESOURCE}def fail(): raise RuntimeError()\nc = C()\n{body}"));
        assert!(findings(&result).is_empty(), "{result:#?}");
        assert_eq!(result.coverage.status, expected_coverage, "{result:#?}");
    }
}

#[test]
fn return_states_exclude_the_raising_fallthrough_state() {
    let result = check_source(&format!("{RESOURCE}def finish(c):\n    if flag:\n        c.close()\n        return\n    raise RuntimeError()\nc = C()\nfinish(c)\nc.status()\n"));
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite");
}

#[test]
fn redefined_functions_keep_their_own_parameter_contracts() {
    let source = "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef f(value: C): pass\ndef f(value: Borrowed[C]): value.close()\nc = C()\nf(c)\n";
    let result = check_source(source);
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP2001" && diagnostic.message.contains("Borrowed parameter")
        }),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");

    let source = "from codepact import resource\nfrom codepact.rust import Borrowed\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef f(value: Borrowed[C]): pass\ndef f(value: C): value.close()\n";
    let result = check_source(source);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "TP2001"),
        "{result:#?}"
    );
}

#[test]
fn same_module_helpers_resolve_global_classes_without_callers_local_aliases() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n@resource(closed_by='close')\nclass Closer:\n    def __init__(self, value): value.close()\n    def close(self): pass\n@resource(closed_by='close')\nclass Safe:\n    def __init__(self, value): pass\n    def close(self): pass\ndef helper(value: C):\n    temp = Closer(value)\ndef caller(value: C):\n    Closer = Safe\n    helper(value)\nc = C()\nunused = caller(c)\nunused = c.status()\n";
    let result = check_source(source);
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn resource_instances_do_not_recover_stale_class_identity_through_aliases() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n@resource(closed_by='close')\nclass Factory:\n    def __init__(self, value): pass\n    def __call__(self, value): value.close()\n    def close(self): pass\nc = C()\nFactory = Factory(c)\nalias = Factory\nother = alias(c)\nc.status()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" && diagnostic.primary.line == 14 }),
        "{result:#?}"
    );
}

#[test]
fn arbitrary_annotation_subscripts_create_a_boundary() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\nclass Hook:\n    def __class_getitem__(cls, arg):\n        c.close()\n        return int\nc = C()\ndef f(value: Hook[int]): pass\nc.status()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" && diagnostic.primary.line == 11 }),
        "{result:#?}"
    );
}

#[test]
fn attribute_evaluation_in_annotations_creates_a_boundary() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n@resource(closed_by='close')\nclass Hook:\n    def __getattribute__(self, name):\n        c.close()\n        return int\n    def close(self): pass\nhook = Hook()\nc = C()\ndef f(value: hook.annotation): pass\nc.status()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" && diagnostic.primary.line == 14 }),
        "{result:#?}"
    );
}

#[test]
fn nested_helpers_share_the_control_flow_stack_budget() {
    let mut source = RESOURCE.to_owned();
    for function in 0..8 {
        source.push_str(&format!("def f{function}(value: C):\n"));
        for depth in 0..120 {
            source.push_str(&format!("{}if flag:\n", "    ".repeat(depth + 1)));
        }
        let body = if function == 7 {
            "value.close()".to_owned()
        } else {
            format!("unused = f{}(value)", function + 1)
        };
        source.push_str(&format!("{}{}\n", "    ".repeat(121), body));
    }
    source.push_str("c = C()\nunused = f0(c)\nc.status()\n");
    let result = check_source(&source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("control-flow nesting budget")));
}

#[test]
fn imported_and_explicit_resource_constructors_preserve_resource_imports() {
    let provider = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n";
    let imported = "from provider import C\nfrom codepact import resource\nc = C()\n@resource(closed_by='close')\nclass D:\n    def close(self): pass\nc.close()\nc.status()\n";
    let result = check(
        &[
            SourceFile::new("main.py", "main", imported),
            SourceFile::new("provider.py", "provider", provider),
        ],
        "3.11",
    );
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");

    let explicit = "from typing import Annotated\nfrom codepact.contracts import Resource\nfrom codepact import resource\nclass C:\n    def close(self): pass\n    def status(self): pass\nView = Annotated[C, Resource(closed_by='close')]\nc = View()\n@resource(closed_by='close')\nclass D:\n    def close(self): pass\nc.close()\nc.status()\n";
    let result = check_source(explicit);
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");
}

#[test]
fn recursive_constructors_stop_at_a_boundary() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def __init__(self): nested = C()\n    def close(self): pass\nc = C()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic
                    .message
                    .contains("constructor effects are unknown")
        }),
        "{result:#?}"
    );
}

#[test]
fn reassignment_constructor_rhs_keeps_the_old_class_binding_during_evaluation() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\nC = C()\n@resource(closed_by='close')\nclass D:\n    def close(self): pass\nC.close()\nC.status()\n";
    let result = check_source(source);
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");
}

#[test]
fn annotation_metadata_hashing_creates_a_boundary() {
    let source = "from codepact import resource\nfrom typing import Annotated\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n@resource(closed_by='close')\nclass Hook:\n    def __hash__(self):\n        c.close()\n        return 1\n    def close(self): pass\nhook = Hook()\nc = C()\ndef f(value: Annotated[C, {hook}]): pass\nc.status()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" && diagnostic.primary.line == 15 }),
        "{result:#?}"
    );
}

#[test]
fn aliased_annotated_metadata_hashing_creates_a_boundary() {
    for (import, annotation) in [
        ("from typing import Annotated as A", "A"),
        ("import typing as t", "t.Annotated"),
        ("from typing_extensions import Annotated as A", "A"),
    ] {
        let source = format!(
            "from codepact import resource\n{import}\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n@resource(closed_by='close')\nclass Hook:\n    def __hash__(self):\n        c.close()\n        return 1\n    def close(self): pass\nhook = Hook()\nc = C()\ndef f(value: {annotation}[C, hook]): pass\nc.status()\n"
        );
        let result = check_source(&source);
        assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TP9001"),
            "{import}: {result:#?}"
        );
        assert!(findings(&result).is_empty(), "{import}: {result:#?}");
    }
}

#[test]
fn unknown_effects_do_not_restore_shadowed_builtin_annotation_names() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\nclass list:\n    def __class_getitem__(cls, item):\n        c.close()\n        return item\nunknown()\nc = C()\ndef f(value: list[C]): pass\nc.status()\n";
    let result = check_source(source);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TP9001" && diagnostic.primary.line == 12 }),
        "{result:#?}"
    );
    assert!(findings(&result).is_empty(), "{result:#?}");
}

#[test]
fn modeled_resource_construction_preserves_the_resource_decorator_import() {
    let source = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\nc = C()\n@resource(closed_by='close')\nclass D:\n    def close(self): pass\nc.close()\nc.status()\n";
    let result = check_source(source);
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite", "{result:#?}");
}

#[test]
fn executable_method_decorators_create_a_boundary_at_class_definition() {
    let result = check_source(&format!("{RESOURCE}c = C()\nclass Config:\n    @decorate(c.close())\n    def setting(self): pass\nc.status()\n"));
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "TP9001" && d.primary.line == 7),
        "{result:#?}"
    );
}

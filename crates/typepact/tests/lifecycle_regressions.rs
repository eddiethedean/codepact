use typepact::{check, Diagnostic, Inspection, SourceFile};

const RESOURCE: &str = "from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n";

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
        let provider = format!("from typepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\ndef wrap(value):\n    temp = C()\n    value.{operation}()\n");
        let caller = "from typepact import resource\nfrom provider import wrap\n@resource(closed_by='release')\nclass C:\n    def close(self): pass\n    def release(self): pass\n    def status(self): pass\nc = C()\nwrap(c)\nc.status()\n";
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
        SourceFile::new("model.py", "model", "from typepact import resource\n@resource(closed_by='close')\nclass Actual:\n    def __init__(self, value): value.close()\n    def close(self): pass\n"),
        SourceFile::new("provider.py", "provider", "from model import Actual as C\ndef wrap(value):\n    temp = C(value)\n"),
        SourceFile::new("caller.py", "caller", "from typepact import resource\nfrom provider import wrap\n@resource(closed_by='close')\nclass C:\n    def __init__(self): pass\n    def close(self): pass\n    def status(self): pass\nc = C()\nwrap(c)\nc.status()\n"),
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
    let mut session = typepact::AnalysisSession::default();
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
    assert_eq!(result.coverage.status, "complete-for-supported-subset");

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
    for (first, second) in [
        (
            "        raise RuntimeError()",
            "        raise RuntimeError()",
        ),
        (
            "        c.close()\n        raise RuntimeError()",
            "        c.status()",
        ),
        (
            "        c.status()",
            "        c.close()\n        raise RuntimeError()",
        ),
    ] {
        let suffix = if first.contains("raise") && second.contains("raise") {
            "c.close()\nc.status()\n"
        } else {
            "c.status()\n"
        };
        let result = check_source(&receiver_source(first, second, suffix));
        assert!(findings(&result).is_empty(), "{result:#?}");
        assert_eq!(result.coverage.status, "incomplete");
    }
}

#[test]
fn raising_expressions_stop_returns_conditions_arguments_and_initializers() {
    for body in [
        "def wrapper(): return fail()\nwrapper()\nc.close()\nc.status()\n",
        "if fail():\n    c.close()\nc.close()\nc.status()\n",
        "def use(first, second): pass\nc.close()\nuse(fail(), c.status())\n",
        "@resource(closed_by='close')\nclass Worker:\n    def __init__(self, first, second): second.status()\n    def close(self): pass\nc.close()\nw = Worker(fail(), c)\n",
    ] {
        let result = check_source(&format!("{RESOURCE}def fail(): raise RuntimeError()\nc = C()\n{body}"));
        assert!(findings(&result).is_empty(), "{result:#?}");
        assert_eq!(result.coverage.status, "incomplete");
    }
}

#[test]
fn return_states_exclude_the_raising_fallthrough_state() {
    let result = check_source(&format!("{RESOURCE}def finish(c):\n    if flag:\n        c.close()\n        return\n    raise RuntimeError()\nc = C()\nfinish(c)\nc.status()\n"));
    assert_eq!(findings(&result).len(), 1, "{result:#?}");
    assert_eq!(findings(&result)[0].certainty, "definite");
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

use typepact::{check, inspect, AnalysisSession, SourceFile};

#[test]
fn eager_class_global_writes_invalidate_module_bindings() {
    for suite in [
        "    global C\n    C = int\n",
        "    if True:\n        global C\n        C = int\n",
        "    class Nested:\n        global C\n        C = int\n",
        "    global C\n    value = (C := int)\n",
    ] {
        let source = format!("from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C: pass\nclass Change:\n{suite}def use(value: Borrowed[C]): pass\n");
        for version in ["3.11", "3.12", "3.13"] {
            let result = inspect(
                &[SourceFile::new("global_write.py", "global_write", &source)],
                version,
            );
            assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
            assert!(result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("class-body global")));
            assert!(!result
                .contracts
                .iter()
                .any(|c| c.display_name == "use.value"));
        }
    }
}

#[test]
fn deferred_method_globals_do_not_invalidate_module_bindings() {
    let source = "from typepact.rust import Borrowed\nclass C: pass\nclass Change:\n    def later(self):\n        global C\n        C = int\ndef use(value: Borrowed[C]): pass\n";
    let result = inspect(
        &[SourceFile::new("deferred.py", "deferred", source)],
        "3.11",
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result
        .contracts
        .iter()
        .any(|c| c.display_name == "use.value"));
}

#[test]
fn nested_contracts_are_visible_unsupported_boundaries() {
    let prefix = "from typepact import resource\nfrom typepact.rust import Borrowed\nfrom typepact.contracts import Resource\nfrom typing import Annotated\n@resource(closed_by='close')\nclass C: pass\nView = Borrowed[C]\n";
    for declaration in [
        "def use(value: list[Borrowed[C]]): pass\n",
        "def use() -> list[Annotated[C, Resource(closed_by='close')]]: pass\n",
        "def use(value: dict[str, View]): pass\n",
        "def use(value: list[C]): pass\n",
        "Nested = list[Borrowed[C]]\n",
        "Nested = tuple[View, ...]\n",
        "def use(value: (Borrowed[C],)): pass\n",
    ] {
        for version in ["3.11", "3.12", "3.13"] {
            let result = inspect(
                &[SourceFile::new(
                    "nested.py",
                    "nested",
                    format!("{prefix}{declaration}"),
                )],
                version,
            );
            assert_eq!(
                result.coverage.status, "incomplete",
                "{declaration}: {result:#?}"
            );
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("nested TypePact contracts")),
                "{result:#?}"
            );
            assert!(!result
                .contracts
                .iter()
                .any(|c| c.display_name.starts_with("use.")));
        }
    }
    let result = inspect(
        &[SourceFile::new(
            "plain.py",
            "plain",
            "def use(value: list[str]) -> dict[str, int]: pass\n",
        )],
        "3.11",
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn typepact_submodule_imports_resolve_metadata_and_access_aliases() {
    let source = "from typepact import rust as r, contracts as ct\nfrom typing import Annotated\nclass C: pass\nView = Annotated[C, ct.Resource(closed_by='close')]\ndef use(value: r.Borrowed[View]) -> ct.Owned[C]: pass\ndef mutate(value: ct.BorrowedMut[C]) -> None: pass\n";
    for version in ["3.11", "3.12", "3.13"] {
        let result = inspect(&[SourceFile::new("modules.py", "modules", source)], version);
        assert_eq!(
            result.coverage.status, "complete-for-supported-subset",
            "{result:#?}"
        );
        let shared = result
            .contracts
            .iter()
            .find(|c| c.display_name == "use.value")
            .unwrap();
        assert_eq!(shared.access, "shared");
        assert_eq!(shared.lifecycle.as_ref().unwrap().closed_by, "close");
        assert!(result
            .contracts
            .iter()
            .any(|c| c.display_name == "use.return" && c.access == "owned"));
        assert!(result
            .contracts
            .iter()
            .any(|c| c.display_name == "mutate.value" && c.access == "exclusive"));
    }
}

#[test]
fn submodule_import_rebinding_removes_canonical_identity() {
    let source = "from typepact import rust as r, contracts as ct\nfrom typing import Annotated\nclass C: pass\nr = list\nct = list\ndef use(value: r.Borrowed[C], other: Annotated[C, ct.Resource(closed_by='close')]): pass\n";
    let result = inspect(&[SourceFile::new("rebound.py", "rebound", source)], "3.11");
    assert!(
        result
            .contracts
            .iter()
            .all(|c| c.access == "unspecified" && c.lifecycle.is_none()),
        "{result:#?}"
    );
}

#[test]
fn submodule_metadata_conflicts_retain_imported_origins() {
    let provider = SourceFile::new(
        "provider.py",
        "provider",
        "from typepact import rust\nclass C: pass\nView = rust.Borrowed[C]\n",
    );
    let consumer = SourceFile::new("consumer.py", "consumer", "from provider import View\nfrom typepact import contracts\nfrom typing import Annotated\ndef use(value: Annotated[View, contracts.OwnedAccess()]): pass\n");
    let result = inspect(&[consumer, provider], "3.11");
    let conflict = result
        .diagnostics
        .iter()
        .find(|d| d.code == "TP4002")
        .unwrap();
    assert_eq!(conflict.related.len(), 1);
    assert_eq!(conflict.related[0].file, "provider.py");
    assert_eq!(conflict.related[0].line, 3);
}

#[test]
fn none_annotations_are_ordinary_non_resource_types() {
    let source = "from typepact.rust import Owned\nclass C: pass\ndef consume(conn: Owned[C], value: None) -> None: pass\nasync def finish(value: None) -> None: pass\n";
    for version in ["3.11", "3.12", "3.13"] {
        let result = inspect(&[SourceFile::new("none.py", "none", source)], version);
        assert_eq!(
            result.coverage.status, "complete-for-supported-subset",
            "{result:#?}"
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.contracts.len(), 1);
        assert_eq!(result.contracts[0].display_name, "consume.conn");
    }
    for annotation in ["'None'", "Owned[None]", "(lambda: None)()"] {
        let source = format!("from typepact.rust import Owned\ndef use() -> {annotation}: pass\n");
        let result = inspect(&[SourceFile::new("unknown.py", "unknown", source)], "3.11");
        assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    }
    let result = inspect(&[SourceFile::new("rebind_none.py", "rebind_none", "from typepact.rust import Owned\nclass C: pass\nC = None\ndef use(value: Owned[C]): pass\n")], "3.11");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result.contracts.is_empty());
}

#[test]
fn class_global_edits_invalidate_cached_consumers_and_recover() {
    let before = "from typepact import resource\n@resource(closed_by='close')\nclass C: pass\n";
    let after = format!("{before}class Change:\n    global C\n    C = int\n");
    let consumer = SourceFile::new("consumer.py", "consumer", "from provider import C\nfrom typepact import rust\ndef use(value: rust.Borrowed[C]) -> None: pass\n");
    let mut session = AnalysisSession::default();
    for source in [before, after.as_str(), before] {
        let files = [
            consumer.clone(),
            SourceFile::new("provider.py", "provider", source),
        ];
        let reused = session.inspect(&files, "3.11");
        assert_eq!(reused, inspect(&files, "3.11"));
        let emits_consumer = reused
            .contracts
            .iter()
            .any(|c| c.display_name == "use.value");
        assert_eq!(emits_consumer, source == before, "{reused:#?}");
    }
}

#[test]
fn long_alias_chains_flatten_without_losing_contracts() {
    let mut source =
        String::from("from typepact.rust import Borrowed\nclass C: pass\nView0 = Borrowed[C]\n");
    for index in 1..2000 {
        source.push_str(&format!("View{index} = View{}\n", index - 1));
    }
    source.push_str("C = None\ndef use(value: View1999) -> None: pass\n");
    let result = inspect(&[SourceFile::new("chain.py", "chain", source)], "3.11");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert_eq!(result.contracts.len(), 1);
    assert_eq!(result.contracts[0].base_symbol, "chain.C");
    assert_eq!(result.contracts[0].access, "shared");
}

#[test]
fn pythonic_and_explicit_access_lower_to_same_fact() {
    let source = r#"from typepact import resource
from typing import Annotated
from typepact.contracts import OwnedAccess, Resource, SharedBorrow
from typepact.rust import Borrowed, Owned
@resource(closed_by="close")
class Connection:
    def close(self): pass
ConnectionView = Annotated[Connection, Resource(closed_by="close")]
def explicit_resource(conn: ConnectionView): pass
def shorthand(conn: Borrowed[Connection]): pass
def verbose(conn: Annotated[Connection, SharedBorrow()]): pass
def shorthand_owned(conn: Owned[Connection]): pass
def verbose_owned(conn: Annotated[Connection, OwnedAccess()]): pass
"#;
    let result = inspect(
        &[SourceFile::new("contracts.py", "contracts", source)],
        "3.11",
    );
    let class_contract = result
        .contracts
        .iter()
        .find(|c| c.display_name == "Connection")
        .unwrap();
    let explicit_contract = result
        .contracts
        .iter()
        .find(|c| c.display_name == "ConnectionView")
        .unwrap();
    assert_eq!(class_contract.base_symbol, explicit_contract.base_symbol);
    assert_eq!(class_contract.lifecycle, explicit_contract.lifecycle);
    let shared: Vec<_> = result
        .contracts
        .iter()
        .filter(|c| c.access == "shared")
        .collect();
    assert_eq!(shared.len(), 2, "{:#?}", result);
    assert_eq!(shared[0].lifecycle, shared[1].lifecycle);
    let owned: Vec<_> = result
        .contracts
        .iter()
        .filter(|c| c.access == "owned")
        .collect();
    assert_eq!(owned.len(), 2, "{:#?}", result);
}

#[test]
fn lookalike_resource_decorator_is_not_trusted() {
    let source = "def resource(**kwargs): pass\n@resource(closed_by='close')\nclass C: pass\n";
    let result = inspect(&[SourceFile::new("local.py", "local", source)], "3.11");
    assert!(result.contracts.is_empty());
}

#[test]
fn unsupported_decorators_do_not_leave_class_or_function_contracts() {
    let class_source = "from typepact import resource\nfrom typepact.rust import Borrowed\ndef replace(cls): return int\n@replace\n@resource(closed_by='close')\nclass C: pass\ndef use(value: Borrowed[C]): pass\n";
    let class_result = inspect(
        &[SourceFile::new(
            "class_decorator.py",
            "class_decorator",
            class_source,
        )],
        "3.11",
    );
    assert_eq!(
        class_result.coverage.status, "incomplete",
        "{class_result:#?}"
    );
    assert!(!class_result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "C" || contract.display_name == "use.value"));

    let function_source = "from typepact.rust import Borrowed\nclass C: pass\ndef replace(function): return None\n@replace\ndef use(value: Borrowed[C]): pass\n";
    let function_result = inspect(
        &[SourceFile::new(
            "function_decorator.py",
            "function_decorator",
            function_source,
        )],
        "3.11",
    );
    assert_eq!(
        function_result.coverage.status, "incomplete",
        "{function_result:#?}"
    );
    assert!(!function_result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "use.value"));
}

#[test]
fn module_level_walrus_expressions_invalidate_stale_bindings() {
    let sources = [
        "from typepact.rust import Borrowed\nclass C: pass\nvalue = (C := int)\ndef use(value: Borrowed[C]): pass\n",
        "from typepact.rust import Borrowed\nclass C: pass\ndef initialize(value: Borrowed[C] = (C := int)): pass\ndef use(value: Borrowed[C]): pass\n",
        "from typepact.rust import Borrowed\nclass C: pass\ndef initialize(factory=lambda value=(C := int): value): pass\ndef use(value: Borrowed[C]): pass\n",
    ];
    for (index, source) in sources.iter().enumerate() {
        let module = format!("walrus_{index}");
        let path = format!("{module}.py");
        let result = inspect(&[SourceFile::new(path, module, *source)], "3.11");
        assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("named expressions")));
        assert!(!result
            .contracts
            .iter()
            .any(|contract| contract.display_name == "use.value"));
    }
}

#[test]
fn walrus_in_a_lambda_body_does_not_invalidate_module_bindings() {
    let source = "from typepact.rust import Borrowed\nclass C: pass\ndef use(value: Borrowed[C], factory=lambda: (C := int)): pass\ndef later(value: Borrowed[C]): pass\n";
    let result = inspect(&[SourceFile::new("lambda.py", "lambda", source)], "3.11");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    for name in ["use.value", "later.value"] {
        assert!(
            result.contracts.iter().any(|contract| {
                contract.display_name == name && contract.base_symbol == "lambda.C"
            }),
            "{result:#?}"
        );
    }
}

#[test]
fn lifecycle_identifiers_follow_the_selected_python_unicode_version() {
    // Unicode 15.0 Kawi, Unicode 15.1 CJK Extension I, and Unicode 16.0 Cyrillic.
    for (method, valid_versions) in [
        ("\u{11f02}", &["3.12", "3.13"][..]),
        ("a\u{11f00}", &["3.12", "3.13"][..]),
        ("\u{2ebf0}", &["3.13"][..]),
        ("\u{1c89}", &[][..]),
    ] {
        let sources = [
            format!("from typepact import resource\n@resource(closed_by='{method}')\nclass C: pass\n"),
            format!("from typing import Annotated\nfrom typepact import Resource\nclass C: pass\nView = Annotated[C, Resource(closed_by='{method}')]\n"),
            format!("from typing import Annotated\nfrom typepact import Resource\nclass C: pass\ndef use(value: Annotated[C, Resource(closed_by='{method}')]): pass\n"),
        ];
        for version in ["3.11", "3.12", "3.13"] {
            let valid = valid_versions.contains(&version);
            for source in &sources {
                let result = inspect(&[SourceFile::new("unicode.py", "unicode", source)], version);
                assert_eq!(
                    result.coverage.status == "complete-for-supported-subset",
                    valid,
                    "target {version}, method {method:?}: {result:#?}"
                );
                assert_eq!(!result.contracts.is_empty(), valid, "{result:#?}");
                assert_eq!(
                    result
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code == "TP4002"),
                    !valid,
                    "{result:#?}"
                );
            }
        }
    }
}

#[test]
fn future_annotations_are_reported_and_not_lowered_eagerly() {
    let source = "from __future__ import annotations\nfrom typepact.rust import Borrowed\nclass C: pass\ndef use(value: Borrowed[C]): pass\n";
    let result = inspect(&[SourceFile::new("future.py", "future", source)], "3.11");

    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("future annotations are not resolved")));
    assert!(!result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "use.value"));
}

#[test]
fn unicode_resource_method_names_match_python_identifiers() {
    let source = "from typepact import resource\n@resource(closed_by='关闭')\nclass C: pass\n";
    let result = inspect(&[SourceFile::new("unicode.py", "unicode", source)], "3.11");

    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "C"
            && contract
                .lifecycle
                .as_ref()
                .is_some_and(|lifecycle| lifecycle.closed_by == "关闭")
    }));
}

#[test]
fn annotated_requires_a_base_type_and_metadata() {
    let source =
        "from typing import Annotated\nclass C: pass\ndef use(value: Annotated[C]): pass\n";
    let result = inspect(&[SourceFile::new("arity.py", "arity", source)], "3.11");

    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("Annotated requires a base type and at least one metadata item")));
    assert!(!result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "use.value"));
}

#[test]
fn aliased_imports_resolve_by_import_identity() {
    let source = "from typepact import resource as pact_resource\nfrom typepact.rust import Borrowed as Shared\n@pact_resource(closed_by='close')\nclass C: pass\ndef use(value: Shared[C]): pass\n";
    let result = inspect(&[SourceFile::new("alias.py", "alias", source)], "3.11");
    assert_eq!(result.contracts.len(), 2, "{:#?}", result);
    assert!(result.contracts.iter().any(|c| c.access == "shared"));
    assert!(result.contracts.iter().all(|c| c.base_symbol == "alias.C"));
}

#[test]
fn output_order_is_stable_and_version_bounds_are_visible() {
    let source = "class C: pass\ndef f(x: C): pass\n";
    let a = inspect(
        &[SourceFile::new("contracts.py", "contracts", source)],
        "3.11",
    );
    let b = inspect(
        &[SourceFile::new("contracts.py", "contracts", source)],
        "3.15",
    );
    let again = inspect(
        &[SourceFile::new("contracts.py", "contracts", source)],
        "3.11",
    );
    assert_eq!(a.contracts, again.contracts);
    assert_eq!(b.coverage.status, "incomplete");
}

#[test]
fn duplicate_source_paths_are_visible_configuration_errors() {
    let first = SourceFile::new("same.py", "first", "class First: pass\n");
    let second = SourceFile::new("same.py", "second", "class Second: pass\n");
    let result = inspect(&[first, second], "3.11");
    assert_eq!(result.coverage.status, "incomplete");
    assert!(result.contracts.is_empty());
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate source path")));
}

#[test]
fn incompatible_metadata_reports_both_origins() {
    let source = "from typing import Annotated\nfrom typepact.contracts import SharedBorrow, OwnedAccess\nclass C: pass\ndef f(value: Annotated[C, SharedBorrow(), OwnedAccess()]): pass\n";
    let result = inspect(
        &[SourceFile::new("conflict.py", "conflict", source)],
        "3.11",
    );
    assert_eq!(result.diagnostics[0].code, "TP4002");
    assert_eq!(result.diagnostics[0].related.len(), 1);
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn example_matches_the_schema_v0_golden_snapshot() {
    let source = include_str!("contracts.py");
    let result = inspect(
        &[SourceFile::new(
            "examples/contracts.py",
            "examples.contracts",
            source,
        )],
        "3.11",
    );
    assert_eq!(
        serde_json::to_string_pretty(&result).unwrap(),
        include_str!("example-contracts.json").trim_end()
    );
}

#[test]
fn docstrings_and_shadowed_imports_do_not_create_contracts() {
    let source = "\"\"\"\nfrom typepact import resource\n@resource(closed_by='close')\nclass Phantom: pass\n\"\"\"\nfrom typepact.rust import Borrowed\nBorrowed = list\nclass C: pass\ndef f(x: Borrowed[C]): pass\n";
    let result = inspect(&[SourceFile::new("shadow.py", "shadow", source)], "3.11");
    assert!(result.contracts.is_empty(), "{:#?}", result);
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn shadowed_annotated_is_not_treated_as_typing_metadata() {
    let source="from typing import Annotated\nfrom typepact.contracts import SharedBorrow\nclass C: pass\nAnnotated = list\ndef f(x: Annotated[C, SharedBorrow()]): pass\n";
    let result = inspect(&[SourceFile::new("shadow.py", "shadow", source)], "3.11");
    assert!(result.contracts.is_empty());
}

#[test]
fn aliases_keep_access_and_lifecycle_and_returns_are_inspected() {
    let source = "from typepact import resource\nfrom typepact.rust import Borrowed, Owned\nfrom typing import Annotated\nfrom typepact.contracts import Resource, SharedBorrow\n@resource(closed_by='close')\nclass C: pass\nView = Annotated[C, Resource(closed_by='close'), SharedBorrow()]\nShortView = Borrowed[C]\ndef f(x: View): pass\ndef g(x: ShortView): pass\ndef make() -> Owned[C]: pass\n";
    let result = inspect(&[SourceFile::new("aliases.py", "aliases", source)], "3.11");
    assert!(result
        .contracts
        .iter()
        .any(|c| c.display_name == "f.x" && c.access == "shared" && c.base_symbol == "aliases.C"));
    assert!(result
        .contracts
        .iter()
        .any(|c| c.display_name == "g.x" && c.access == "shared" && c.base_symbol == "aliases.C"));
    assert!(result
        .contracts
        .iter()
        .any(|c| c.display_name == "make.return"
            && c.access == "owned"
            && c.base_symbol == "aliases.C"));
}

#[test]
fn unresolved_qualified_types_and_multiline_declarations_are_incomplete() {
    let source = "from typepact.rust import Borrowed\ndef f(x: Borrowed[missing.Connection]): pass\ndef g(\n    x: Borrowed[missing.Connection],\n): pass\n";
    let result = inspect(
        &[SourceFile::new("unresolved.py", "unresolved", source)],
        "3.11",
    );
    assert_eq!(result.coverage.status, "incomplete");
    assert!(result.contracts.is_empty());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unresolved annotation base type")));
    assert!(result
        .diagnostics
        .iter()
        .all(|d| !d.message.contains("multi-line function")));
}

#[test]
fn bindings_from_another_file_do_not_resolve_implicitly() {
    let a = SourceFile::new(
        "a.py",
        "a",
        "from typepact.rust import Borrowed\nclass C: pass\n",
    );
    let b = SourceFile::new("b.py", "b", "def f(x: Borrowed[C]): pass\n");
    let result = inspect(&[a, b], "3.11");
    assert!(result.contracts.is_empty(), "{:#?}", result);
}

#[test]
fn direct_supplied_module_imports_resolve_independent_of_file_order() {
    let provider = SourceFile::new(
        "models.py",
        "models",
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection: pass\n",
    );
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from models import Connection as Conn\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Conn]): pass\n",
    );
    let forward = inspect(&[provider.clone(), consumer.clone()], "3.11");
    let reverse = inspect(&[consumer, provider], "3.11");
    assert_eq!(forward, reverse);
    assert!(
        forward
            .contracts
            .iter()
            .any(|contract| contract.display_name == "use.value"
                && contract.base_symbol == "models.Connection"
                && contract.access == "shared"
                && contract
                    .lifecycle
                    .as_ref()
                    .is_some_and(|lifecycle| lifecycle.closed_by == "close")),
        "{forward:#?}"
    );
}

#[test]
fn qualified_names_do_not_bypass_explicit_imports() {
    let provider = SourceFile::new("models.py", "models", "class Connection: pass\n");
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from typepact.rust import Borrowed\ndef use(value: Borrowed[models.Connection]): pass\n",
    );
    let result = inspect(&[provider, consumer], "3.11");
    assert!(result.contracts.is_empty(), "{result:#?}");
    assert!(result.coverage.status == "incomplete");
}

#[test]
fn supplied_module_cycles_are_incomplete_and_deterministic() {
    let a = SourceFile::new("a.py", "a", "from b import B\nclass A: pass\n");
    let b = SourceFile::new("b.py", "b", "from a import A\nclass B: pass\n");
    let forward = inspect(&[a.clone(), b.clone()], "3.11");
    let reverse = inspect(&[b, a], "3.11");
    assert_eq!(forward, reverse);
    assert_eq!(forward.coverage.status, "incomplete");
    assert!(forward
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("import cycle")));
}

#[test]
fn acyclic_named_reexports_preserve_contract_and_lifecycle_identity() {
    let origin = SourceFile::new(
        "models.py",
        "models",
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
    );
    let facade = SourceFile::new(
        "facade.py",
        "facade",
        "from models import Connection as OriginConnection\n",
    );
    let api = SourceFile::new(
        "api.py",
        "api",
        "from facade import OriginConnection as Connection\n",
    );
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from api import Connection as Conn\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Conn]): return value.status()\nconnection = Conn()\nconnection.close()\nconnection.status()\n",
    );
    let result = check(
        &[
            consumer.clone(),
            api.clone(),
            facade.clone(),
            origin.clone(),
        ],
        "3.11",
    );
    let reordered = check(&[origin, facade, api, consumer], "3.11");
    assert_eq!(result, reordered);
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    let contract = result
        .contracts
        .iter()
        .find(|contract| contract.display_name == "use.value")
        .expect("the borrowed declaration resolves through both re-export modules");
    assert_eq!(contract.base_symbol, "models.Connection");
    assert_eq!(
        contract
            .lifecycle
            .as_ref()
            .map(|lifecycle| lifecycle.closed_by.as_str()),
        Some("close")
    );
    let findings: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "TP1001")
        .collect();
    assert_eq!(findings.len(), 1, "{result:#?}");
    assert_eq!(findings[0].certainty, "definite");
    assert_eq!(findings[0].primary.file, "client.py");
    assert_eq!(findings[0].related[0].file, "client.py");
}

#[test]
fn conflicting_resource_decorators_are_rejected() {
    let source="from typepact import resource\n@resource(closed_by='close')\n@resource(closed_by='shutdown')\nclass C: pass\n";
    let result = inspect(
        &[SourceFile::new("conflict.py", "conflict", source)],
        "3.11",
    );
    assert!(result.contracts.is_empty());
    assert_eq!(result.diagnostics[0].code, "TP4002");
    assert!(!result.diagnostics[0].related.is_empty());
}

#[test]
fn eager_alias_reassignments_capture_the_original_declaration() {
    let source = "from typing import Annotated\nclass C: pass\nA = Annotated[C, 'opaque']\nB = Annotated[A, 'opaque']\nA = Annotated[B, 'opaque']\nfrom typepact.rust import Borrowed\ndef f(x: Borrowed[A]): pass\n";
    let result = inspect(&[SourceFile::new("cycle.py", "cycle", source)], "3.11");
    assert!(
        result
            .contracts
            .iter()
            .any(|contract| contract.display_name == "f.x"
                && contract.base_symbol == "cycle.C"
                && contract.access == "shared"),
        "{:#?}",
        result
    );
}

#[test]
fn calls_in_function_bodies_do_not_hide_parameter_or_return_annotations() {
    let source = "from typepact.rust import Borrowed, Owned\nclass C: pass\ndef use(x: Borrowed[C]): print(x)\nasync def make() -> Owned[C]: return C()\n";
    let result = inspect(&[SourceFile::new("calls.py", "calls", source)], "3.11");
    assert!(
        result
            .contracts
            .iter()
            .any(|c| c.display_name == "use.x" && c.access == "shared"),
        "{:#?}",
        result
    );
    assert!(
        result
            .contracts
            .iter()
            .any(|c| c.display_name == "make.return" && c.access == "owned"),
        "{:#?}",
        result
    );
}

#[test]
fn comment_quotes_do_not_mask_following_declarations() {
    let source = "from typepact.rust import Owned\n# triple quote: \\\"\\\"\\\"\nclass C: pass\ndef f(x: Owned[C]): pass\n";
    let result = inspect(&[SourceFile::new("comment.py", "comment", source)], "3.11");
    assert!(
        result.contracts.iter().any(|c| c.display_name == "f.x"),
        "{:#?}",
        result
    );
}

#[test]
fn triple_quote_text_inside_ordinary_strings_does_not_mask_code() {
    let source = "from typepact.rust import Owned\ntext = '\"\"\"'\nclass C: pass\ndef f(x: Owned[C]): pass\n";
    let result = inspect(&[SourceFile::new("string.py", "string", source)], "3.11");
    assert!(
        result.contracts.iter().any(|c| c.display_name == "f.x"),
        "{:#?}",
        result
    );
}

#[test]
fn return_metadata_conflicts_are_rejected() {
    let source = "from typing import Annotated\nfrom typepact.contracts import OwnedAccess, SharedBorrow\nclass C: pass\ndef f() -> Annotated[C, OwnedAccess(), SharedBorrow()]: pass\n";
    let result = inspect(
        &[SourceFile::new(
            "return_conflict.py",
            "return_conflict",
            source,
        )],
        "3.11",
    );
    assert!(result.contracts.is_empty(), "{:#?}", result);
    assert!(result.diagnostics.iter().any(|d| d.code == "TP4002"));
}

#[test]
fn conflicting_alias_metadata_is_an_error() {
    let source = "from typing import Annotated\nfrom typepact.contracts import SharedBorrow, OwnedAccess\nclass C: pass\nView = Annotated[C, SharedBorrow(), OwnedAccess()]\ndef f(x: View): pass\n";
    let result = inspect(
        &[SourceFile::new(
            "conflict_alias.py",
            "conflict_alias",
            source,
        )],
        "3.11",
    );
    assert!(result.diagnostics.iter().any(|d| d.code == "TP4002"));
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn rebound_classes_and_imports_lose_their_typepact_identity() {
    let source = "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C: pass\nC = int\ndef use(x: Borrowed[C]): pass\nfrom typepact import resource\nfrom other import resource\n@resource(closed_by='close')\nclass D: pass\n";
    let result = inspect(&[SourceFile::new("rebind.py", "rebind", source)], "3.11");
    assert!(
        result
            .contracts
            .iter()
            .all(|c| c.display_name != "use.x" && c.display_name != "D"),
        "{:#?}",
        result
    );
}

#[test]
fn annotated_import_alias_is_resolved_by_canonical_identity() {
    let source = "from typing import Annotated as A\nfrom typepact.contracts import SharedBorrow\nclass C: pass\ndef use(x: A[C, SharedBorrow()]): pass\n";
    let result = inspect(
        &[SourceFile::new(
            "annotated_alias.py",
            "annotated_alias",
            source,
        )],
        "3.11",
    );
    assert!(
        result
            .contracts
            .iter()
            .any(|c| c.display_name == "use.x" && c.access == "shared"),
        "{:#?}",
        result
    );
}

#[test]
fn root_and_contracts_reexports_share_canonical_metadata_identities() {
    let source = "from typing import Annotated\nfrom typepact import Resource as RootResource\nfrom typepact.contracts import Borrowed, BorrowedMut, Owned, Resource\nclass C: pass\nRootView = Annotated[C, RootResource(closed_by='close')]\nContractView = Annotated[C, Resource(closed_by='close')]\ndef root(value: RootView): pass\ndef borrowed(value: Borrowed[C]): pass\ndef owned(value: Owned[C]): pass\ndef mutable(value: BorrowedMut[C]): pass\n";
    let result = inspect(&[SourceFile::new("aliases.py", "aliases", source)], "3.11");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "RootView"
            && contract
                .lifecycle
                .as_ref()
                .is_some_and(|value| value.closed_by == "close")));
    assert!(result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "root.value"
            && contract
                .lifecycle
                .as_ref()
                .is_some_and(|value| value.closed_by == "close")));
    assert!(result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "borrowed.value" && contract.access == "shared"));
    assert!(result
        .contracts
        .iter()
        .any(|contract| contract.display_name == "owned.value" && contract.access == "owned"));
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "mutable.value" && contract.access == "exclusive"
    }));
}

#[test]
fn inline_resource_metadata_is_lowered_on_function_annotations() {
    let source = "from typing import Annotated\nfrom typepact.contracts import Resource\nclass C: pass\ndef keyword(value: Annotated[C, Resource(closed_by='close')]): pass\ndef positional(value: Annotated[C, Resource('release')]): pass\n";
    let result = inspect(&[SourceFile::new("inline.py", "inline", source)], "3.11");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "keyword.value"
            && contract
                .lifecycle
                .as_ref()
                .is_some_and(|value| value.closed_by == "close")
    }));
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "positional.value"
            && contract
                .lifecycle
                .as_ref()
                .is_some_and(|value| value.closed_by == "release")
    }));
}

#[test]
fn inherited_lifecycle_and_access_conflicts_report_both_origins() {
    let lifecycle_source = "from typing import Annotated\nfrom typepact import resource\nfrom typepact.contracts import Resource\n@resource(closed_by='close')\nclass C: pass\nView = Annotated[C, Resource(closed_by='release')]\n";
    let lifecycle = inspect(
        &[SourceFile::new(
            "lifecycle.py",
            "lifecycle",
            lifecycle_source,
        )],
        "3.11",
    );
    let lifecycle_diagnostic = lifecycle
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP4002")
        .expect("inherited lifecycle conflict is diagnosed");
    assert_eq!(lifecycle_diagnostic.related.len(), 1);
    assert_eq!(lifecycle_diagnostic.primary.line, 6);
    assert_eq!(lifecycle_diagnostic.related[0].line, 4);

    let access_source = "from typepact.rust import Borrowed, Owned\nclass C: pass\nView = Borrowed[C]\ndef use(value: Owned[View]): pass\n";
    let access = inspect(
        &[SourceFile::new("access.py", "access", access_source)],
        "3.11",
    );
    let access_diagnostic = access
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP4002")
        .expect("inherited access conflict is diagnosed");
    assert_eq!(access_diagnostic.related.len(), 1);
    assert_eq!(access_diagnostic.primary.line, 4);
    assert_eq!(access_diagnostic.related[0].line, 3);
}

#[test]
fn imported_metadata_conflict_points_to_the_dependency_declaration() {
    let provider = SourceFile::new(
        "provider.py",
        "provider",
        "from typepact import resource\n@resource(closed_by='close')\nclass C: pass\n",
    );
    let consumer = SourceFile::new(
        "consumer.py",
        "consumer",
        "from provider import C\nfrom typing import Annotated\nfrom typepact.contracts import Resource\ndef use(value: Annotated[C, Resource(closed_by='release')]): pass\n",
    );
    let result = inspect(&[consumer, provider], "3.11");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP4002")
        .expect("the imported lifecycle conflict is diagnosed");
    assert_eq!(diagnostic.primary.file, "consumer.py");
    assert_eq!(diagnostic.primary.line, 4);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].file, "provider.py");
    assert_eq!(diagnostic.related[0].line, 2);
}

#[test]
fn later_import_shadowing_preserves_earlier_dependency_provenance() {
    let provider = SourceFile::new(
        "provider.py",
        "provider",
        "from typepact import resource\n@resource(closed_by='close')\nclass C: pass\n",
    );
    let consumer = SourceFile::new(
        "consumer.py",
        "consumer",
        "from provider import C\nfrom typing import Annotated as A\nfrom typepact.contracts import Resource\ndef use(value: A[C, Resource(closed_by='release')]): pass\nfrom somewhere import A\n",
    );
    let mut session = AnalysisSession::default();
    let result = session.inspect(&[consumer, provider], "3.11");

    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP4002")
        .expect("the lifecycle conflict is diagnosed");
    assert_eq!(diagnostic.primary.file, "consumer.py");
    assert_eq!(diagnostic.primary.line, 4);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].file, "provider.py");
    assert_eq!(diagnostic.related[0].line, 2);
}

#[test]
fn assignments_resolve_the_old_binding_and_annotation_only_does_not_rebind() {
    let reassignment = "from typepact.rust import Borrowed\nclass C: pass\nC = Borrowed[C]\ndef use(value: C): pass\n";
    let result = inspect(
        &[SourceFile::new("rebind.py", "rebind", reassignment)],
        "3.11",
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "use.value"
            && contract.base_symbol == "rebind.C"
            && contract.access == "shared"
    }));

    let annotated = "from typepact.rust import Borrowed\nclass C: pass\nC: type\ndef use(value: Borrowed[C]): pass\n";
    let result = inspect(
        &[SourceFile::new("annotated.py", "annotated", annotated)],
        "3.11",
    );
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
    assert!(result.contracts.iter().any(|contract| {
        contract.display_name == "use.value" && contract.base_symbol == "annotated.C"
    }));
}

#[test]
fn unresolved_imported_annotation_makes_coverage_incomplete() {
    let source = "from nonexistent_module import C\ndef use(value: C): pass\n";
    let result = inspect(&[SourceFile::new("missing.py", "missing", source)], "3.11");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unresolved module nonexistent_module")
        && diagnostic.related.len() == 1));
}

#[test]
fn unexpected_resource_arguments_are_not_accepted_as_contracts() {
    for source in [
        "from typepact import resource\n@resource(closed_by='close', surprise=True)\nclass C: pass\n",
        "from typing import Annotated\nfrom typepact.contracts import Resource\nclass C: pass\nView = Annotated[C, Resource(closed_by='close', surprise=True)]\n",
    ] {
        let result = inspect(&[SourceFile::new("malformed.py", "malformed", source)], "3.11");
        assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
        assert!(!result
            .contracts
            .iter()
            .any(|contract| contract.display_name == "View" || contract.display_name == "C"));
    }
}

#[test]
fn syntax_error_ranges_cover_multibyte_characters_without_panicking() {
    let result = inspect(
        &[SourceFile::new("unicode.py", "unicode", "x = 💥\n")],
        "3.11",
    );
    assert_eq!(result.coverage.status, "incomplete");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP4001")
        .expect("the syntax error is retained");
    assert_eq!(diagnostic.primary.column, 5);
    assert_eq!(diagnostic.primary.end_column, 9);
}

#[test]
fn reusable_session_matches_fresh_inspection_after_file_updates() {
    let mut session = typepact::AnalysisSession::default();
    let first = SourceFile::new(
        "one.py",
        "one",
        "from typepact.rust import Borrowed\nclass C: pass\ndef f(x: Borrowed[C]): pass\n",
    );
    let cached = session.inspect(std::slice::from_ref(&first), "3.11");
    assert_eq!(cached, inspect(std::slice::from_ref(&first), "3.11"));

    let updated = SourceFile::new("one.py", "one", "class D: pass\n");
    let reused = session.inspect(std::slice::from_ref(&updated), "3.11");
    assert_eq!(reused, inspect(std::slice::from_ref(&updated), "3.11"));
    assert!(reused.contracts.is_empty());
}

#[test]
fn session_resolves_a_dependency_added_after_an_initial_missing_import() {
    let mut session = typepact::AnalysisSession::default();
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from models import Connection\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Connection]): pass\n",
    );
    let missing = session.inspect(std::slice::from_ref(&consumer), "3.11");
    assert_eq!(missing.coverage.status, "incomplete");

    let provider = SourceFile::new("models.py", "models", "class Connection: pass\n");
    let resolved = session.inspect(&[consumer.clone(), provider], "3.11");
    assert!(resolved.contracts.iter().any(|contract| {
        contract.display_name == "use.value" && contract.base_symbol == "models.Connection"
    }));
    assert_eq!(
        resolved,
        inspect(
            &[
                consumer,
                SourceFile::new("models.py", "models", "class Connection: pass\n")
            ],
            "3.11"
        )
    );
}

#[test]
fn eager_alias_keeps_its_class_after_the_name_is_rebound() {
    let source = "from typepact.rust import Borrowed\nclass C: pass\nView = Borrowed[C]\nC = int\ndef use(value: View): pass\n";
    let result = inspect(&[SourceFile::new("capture.py", "capture", source)], "3.11");
    assert!(
        result
            .contracts
            .iter()
            .any(|contract| contract.display_name == "use.value"
                && contract.base_symbol == "capture.C"
                && contract.access == "shared"),
        "{:#?}",
        result
    );
}

#[test]
fn function_annotations_read_the_old_binding_before_the_function_rebinds_it() {
    let source = "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass C: pass\ndef C(value: Borrowed[C]): pass\ndef use(value: Borrowed[C]): pass\n";
    let result = inspect(
        &[SourceFile::new(
            "function_rebind.py",
            "function_rebind",
            source,
        )],
        "3.11",
    );
    assert!(
        result
            .contracts
            .iter()
            .any(|contract| contract.display_name == "C.value"
                && contract.base_symbol == "function_rebind.C"),
        "{result:#?}"
    );
    assert!(result
        .contracts
        .iter()
        .all(|contract| contract.display_name != "use.value"));
    assert_eq!(result.coverage.status, "incomplete");
}

#[test]
fn deleting_a_name_removes_its_current_contract_binding() {
    let source = "from typepact.rust import Borrowed\nclass C: pass\ndel C\ndef use(value: Borrowed[C]): pass\n";
    let result = inspect(&[SourceFile::new("deleted.py", "deleted", source)], "3.11");
    assert!(result.contracts.is_empty(), "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete");
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unresolved annotation base type")));
}

#[test]
fn unsupported_bases_are_visible_in_coverage() {
    let result = inspect(
        &[SourceFile::new("base.py", "base", "class C(Base): pass\n")],
        "3.11",
    );
    assert_eq!(result.coverage.status, "incomplete");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Ruff AST marked syntax")));
}

#[test]
fn eval_text_in_a_string_does_not_look_like_execution() {
    let result = inspect(
        &[SourceFile::new(
            "text.py",
            "text",
            "message = \"eval(\"\nclass C: pass\n",
        )],
        "3.11",
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn multiline_signatures_are_lowered_from_ast_ranges() {
    let source = "from typepact.rust import Borrowed\nclass C: pass\ndef use(\n    value: Borrowed[\n        C\n    ],\n) -> C: pass\n";
    let result = inspect(
        &[SourceFile::new("multiline.py", "multiline", source)],
        "3.11",
    );
    let parameter = result
        .contracts
        .iter()
        .find(|contract| contract.display_name == "use.value")
        .expect("multiline parameter annotation is extracted");
    assert_eq!(parameter.base_symbol, "multiline.C");
    assert_eq!(parameter.access, "shared");
    assert_eq!(parameter.source.line, 4);
    assert_eq!(parameter.source.end_line, 6);
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

#[test]
fn repeated_identical_access_metadata_is_not_a_conflict() {
    let source = "from typing import Annotated\nfrom typepact.contracts import SharedBorrow\nclass C: pass\ndef use(value: Annotated[C, SharedBorrow(), SharedBorrow()]): pass\n";
    let result = inspect(
        &[SourceFile::new("same_access.py", "same_access", source)],
        "3.11",
    );
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code != "TP4002"));
    assert!(result
        .contracts
        .iter()
        .any(|contract| { contract.display_name == "use.value" && contract.access == "shared" }));
}

#[test]
fn direct_module_imports_preserve_exported_alias_views() {
    let provider = SourceFile::new(
        "models.py",
        "models",
        "from typepact import resource\nfrom typepact.rust import Borrowed\n@resource(closed_by='close')\nclass Connection: pass\nConnectionView = Borrowed[Connection]\n",
    );
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from models import ConnectionView as View\ndef use(value: View): pass\n",
    );
    let result = inspect(&[consumer, provider], "3.11");
    assert!(
        result.contracts.iter().any(|contract| {
            contract.display_name == "use.value"
                && contract.base_symbol == "models.Connection"
                && contract.access == "shared"
                && contract
                    .lifecycle
                    .as_ref()
                    .is_some_and(|life| life.closed_by == "close")
        }),
        "{result:#?}"
    );
}

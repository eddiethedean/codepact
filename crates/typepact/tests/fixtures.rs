use typepact::{inspect, SourceFile};

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
fn supplied_module_reexports_are_reported_as_incomplete() {
    let origin = SourceFile::new("origin.py", "origin", "class Connection: pass\n");
    let facade = SourceFile::new("facade.py", "facade", "from origin import Connection\n");
    let consumer = SourceFile::new(
        "client.py",
        "client",
        "from facade import Connection\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Connection]): pass\n",
    );
    let result = inspect(&[origin, facade, consumer], "3.11");
    assert_eq!(result.coverage.status, "incomplete");
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("does not directly export class")));
    assert!(result
        .contracts
        .iter()
        .all(|contract| contract.display_name != "use.value"));
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

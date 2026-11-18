use super::*;

pub(super) fn function_is_must_use(
    decorators: &[AstExpr],
    returns: Option<&AstExpr>,
    imports: &BTreeMap<String, String>,
) -> bool {
    let decorated = decorators
        .iter()
        .any(|decorator| canonical(decorator, imports).as_deref() == Some("codepact::must_use"));
    decorated || returns.is_some_and(|annotation| annotation_is_must_use(annotation, imports))
}

pub(super) fn supported_method_must_use(
    decorators: &[AstExpr],
    returns: Option<&AstExpr>,
    imports: &BTreeMap<String, String>,
) -> Result<bool, ()> {
    if !function_is_must_use(decorators, returns, imports) {
        return Ok(false);
    }
    let mut marker_index = None;
    let mut wrapper_index = None;
    for (index, decorator) in decorators.iter().enumerate() {
        match canonical(decorator, imports).as_deref() {
            Some("codepact::must_use") => marker_index = Some(index),
            Some("builtins::staticmethod" | "builtins::classmethod") => {
                if wrapper_index.replace(index).is_some() {
                    return Err(());
                }
            }
            _ => return Err(()),
        }
    }
    if let (Some(marker), Some(wrapper)) = (marker_index, wrapper_index) {
        if wrapper > marker {
            return Err(());
        }
    }
    Ok(true)
}

pub(super) fn annotation_is_must_use(
    annotation: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    match &annotation.kind {
        AstExprKind::Subscript(container, arguments) => {
            match canonical(container, imports).as_deref() {
                Some("codepact::MustUse") => true,
                Some("typing::Annotated") => arguments.iter().skip(1).any(|metadata| {
                    let expression = match &metadata.kind {
                        AstExprKind::Call { function, .. } => function.as_ref(),
                        _ => metadata,
                    };
                    canonical(expression, imports).as_deref()
                        == Some("codepact::contracts::MustUseResult")
                }),
                _ => false,
            }
        }
        _ => false,
    }
}

pub(super) fn contains_must_use_marker(
    expression: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    if annotation_is_must_use(expression, imports)
        || canonical(expression, imports).is_some_and(|identity| {
            matches!(
                identity.as_str(),
                "codepact::MustUse" | "codepact::contracts::MustUseResult"
            )
        })
    {
        return true;
    }
    match &expression.kind {
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            contains_must_use_marker(value, imports)
        }
        AstExprKind::Subscript(container, arguments) => {
            contains_must_use_marker(container, imports)
                || arguments
                    .iter()
                    .any(|argument| contains_must_use_marker(argument, imports))
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| contains_must_use_marker(argument, imports)),
        AstExprKind::OtherCalls(calls, _) => calls.iter().any(|call| {
            canonical_path(call, imports).is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "codepact::MustUse" | "codepact::contracts::MustUseResult"
                )
            })
        }),
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            contains_must_use_marker(function, imports)
                || arguments
                    .iter()
                    .any(|argument| contains_must_use_marker(argument, imports))
                || keywords
                    .iter()
                    .any(|(_, value)| contains_must_use_marker(value, imports))
        }
        _ => false,
    }
}

pub(super) fn report_misplaced_must_use_annotation(
    result: &mut Inspection,
    file: &SourceFile,
    annotation: &AstExpr,
    imports: &BTreeMap<String, String>,
) {
    if contains_must_use_marker(annotation, imports) {
        declaration_error(
            result,
            file,
            annotation.range,
            "TP4002",
            "must-use metadata is supported only on function return annotations",
            Vec::new(),
        );
    }
}

pub(super) fn is_misplaced_must_use_decorator(
    decorator: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> bool {
    let identity = match &decorator.kind {
        AstExprKind::Call { function, .. } => canonical(function, imports),
        _ => canonical(decorator, imports),
    };
    identity.is_some_and(|identity| {
        matches!(
            identity.as_str(),
            "codepact::MustUse" | "codepact::contracts::MustUseResult"
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check_must_use_body(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    state: &FileState,
    class_scope: Option<(&str, Option<&str>)>,
    parameters: &[String],
    parameter_annotations: &[(String, AstExpr)],
    finite_matches: bool,
) {
    let mut locals = parameters.iter().cloned().collect::<BTreeSet<_>>();
    collect_must_use_locals(statements, &mut locals);
    if finite_matches {
        let mut finite_imports = state.imports.clone();
        let mut local_returns = state.outcome_returns.clone();
        for name in &locals {
            finite_imports.remove(name);
            local_returns.remove(name);
        }
        check_finite_matches(
            result,
            file,
            statements,
            &finite_imports,
            parameter_annotations,
            &local_returns,
        );
    }
    let mut marked = state.must_use_callables.clone();
    let mut imports = state.imports.clone();
    let mut instances = state.instances.clone();
    for name in locals {
        marked.retain(|candidate, _| {
            candidate != &name && !candidate.starts_with(&format!("{name}."))
        });
        imports.remove(&name);
        instances.remove(&name);
    }
    walk_must_use_body(
        result,
        file,
        statements,
        &mut marked,
        &mut instances,
        &imports,
        &state.known_classes,
        class_scope,
        0,
    );
}

pub(super) fn outcome_variants_for_annotation(
    annotation: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> Option<BTreeSet<String>> {
    let AstExprKind::Subscript(container, _) = &annotation.kind else {
        return None;
    };
    match canonical(container, imports).as_deref() {
        Some("codepact::Result") => Some(BTreeSet::from([
            "codepact::Ok".into(),
            "codepact::Err".into(),
        ])),
        Some("codepact::Option") => Some(BTreeSet::from([
            "codepact::Some".into(),
            "codepact::Nothing".into(),
        ])),
        _ => None,
    }
}

type OutcomeFacts = BTreeMap<String, BTreeSet<String>>;

fn expression_outcomes(
    expression: &AstExpr,
    facts: &OutcomeFacts,
    imports: &BTreeMap<String, String>,
    returns: &BTreeMap<String, BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    match &expression.kind {
        AstExprKind::Name(name) => facts.get(name).cloned(),
        AstExprKind::Call { function, .. } => {
            let name = expression_path(function)?;
            if let Some(identity) =
                canonical(function, imports).filter(|identity| is_outcome_variant(identity))
            {
                return Some(BTreeSet::from([identity]));
            }
            returns.get(&name).cloned()
        }
        _ => None,
    }
}

fn is_outcome_variant(identity: &str) -> bool {
    matches!(
        identity,
        "codepact::Ok" | "codepact::Err" | "codepact::Some" | "codepact::Nothing"
    )
}

fn pattern_is_irrefutable(pattern: &crate::frontend::AstPattern) -> bool {
    use crate::frontend::AstPattern as Pattern;
    match pattern {
        Pattern::Capture { .. } | Pattern::As { pattern: None, .. } => true,
        Pattern::As {
            pattern: Some(pattern),
            ..
        } => pattern_is_irrefutable(pattern),
        Pattern::Or { patterns, .. } => patterns.iter().any(pattern_is_irrefutable),
        _ => false,
    }
}

#[derive(Default)]
struct PatternCoverage {
    covered: BTreeSet<String>,
    unresolved: BTreeSet<String>,
}

fn payload_pattern_is_capture(pattern: &crate::frontend::AstPattern) -> bool {
    use crate::frontend::AstPattern as Pattern;
    match pattern {
        Pattern::Capture { .. } => true,
        Pattern::As { pattern: None, .. } => true,
        Pattern::As {
            pattern: Some(pattern),
            ..
        } => payload_pattern_is_capture(pattern),
        _ => false,
    }
}

fn pattern_coverage(
    pattern: &crate::frontend::AstPattern,
    imports: &BTreeMap<String, String>,
    reachable: &BTreeSet<String>,
) -> PatternCoverage {
    use crate::frontend::AstPattern as Pattern;
    match pattern {
        Pattern::Capture { .. } | Pattern::As { pattern: None, .. } => PatternCoverage {
            covered: reachable.clone(),
            ..PatternCoverage::default()
        },
        Pattern::As {
            pattern: Some(pattern),
            ..
        } => pattern_coverage(pattern, imports, reachable),
        Pattern::Or { patterns, .. } => {
            let mut result = PatternCoverage::default();
            let mut remaining = reachable.clone();
            for pattern in patterns {
                if remaining.is_empty() {
                    break;
                }
                let alternative = pattern_coverage(pattern, imports, &remaining);
                remaining.retain(|variant| !alternative.covered.contains(variant));
                result.covered.extend(alternative.covered);
                result.unresolved.extend(alternative.unresolved);
            }
            result
                .unresolved
                .retain(|variant| remaining.contains(variant));
            result
        }
        Pattern::Class {
            class,
            positional,
            keywords,
            ..
        } => {
            let Some(identity) = canonical(class, imports).filter(|name| is_outcome_variant(name))
            else {
                return PatternCoverage {
                    unresolved: reachable.clone(),
                    ..PatternCoverage::default()
                };
            };
            if !reachable.contains(&identity) {
                return PatternCoverage::default();
            }
            let field = match identity.as_str() {
                "codepact::Ok" | "codepact::Some" => Some("value"),
                "codepact::Err" => Some("error"),
                "codepact::Nothing" => None,
                _ => unreachable!(),
            };
            let valid = positional.len() + keywords.len() <= usize::from(field.is_some())
                && positional.iter().all(payload_pattern_is_capture)
                && keywords.iter().all(|(name, pattern)| {
                    Some(name.as_str()) == field && payload_pattern_is_capture(pattern)
                });
            if valid {
                PatternCoverage {
                    covered: BTreeSet::from([identity]),
                    ..PatternCoverage::default()
                }
            } else {
                PatternCoverage {
                    unresolved: BTreeSet::from([identity]),
                    ..PatternCoverage::default()
                }
            }
        }
        Pattern::Unsupported { .. } => PatternCoverage {
            unresolved: reachable.clone(),
            ..PatternCoverage::default()
        },
    }
}

fn merge_outcome_facts(paths: &[OutcomeFacts]) -> OutcomeFacts {
    let Some(first) = paths.first() else {
        return OutcomeFacts::new();
    };
    let mut merged = OutcomeFacts::new();
    for (name, first_values) in first {
        let mut values = first_values.clone();
        let mut present = true;
        for path in &paths[1..] {
            if let Some(other) = path.get(name) {
                values.extend(other.iter().cloned());
            } else {
                present = false;
                break;
            }
        }
        if present {
            merged.insert(name.clone(), values);
        }
    }
    merged
}

fn check_finite_matches(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    imports: &BTreeMap<String, String>,
    parameter_annotations: &[(String, AstExpr)],
    returns: &BTreeMap<String, BTreeSet<String>>,
) {
    let mut facts = OutcomeFacts::new();
    for (name, annotation) in parameter_annotations {
        if let Some(variants) = outcome_variants_for_annotation(annotation, imports) {
            facts.insert(name.clone(), variants);
        }
    }
    scan_outcome_statements(result, file, statements, imports, returns, &mut facts, 0);
}

fn scan_outcome_statements(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    imports: &BTreeMap<String, String>,
    returns: &BTreeMap<String, BTreeSet<String>>,
    facts: &mut OutcomeFacts,
    depth: usize,
) {
    use crate::frontend::FlowStatement as Flow;
    if depth >= 128 {
        return;
    }
    for statement in statements {
        match statement {
            Flow::Assign {
                name,
                annotation: _,
                value,
                ..
            } => {
                // A local annotation describes the expected value; it cannot
                // turn an arbitrary or contradictory RHS into a closed result.
                let assigned = expression_outcomes(value, facts, imports, returns);
                if let Some(assigned) = assigned {
                    facts.insert(name.clone(), assigned);
                } else {
                    facts.remove(name);
                }
            }
            Flow::EscapingAssign { name, .. } => {
                facts.remove(name);
            }
            Flow::If {
                test,
                body,
                alternate,
                ..
            } => {
                if let AstExprKind::Boolean(value) = test.kind {
                    scan_outcome_statements(
                        result,
                        file,
                        if value { body } else { alternate },
                        imports,
                        returns,
                        facts,
                        depth + 1,
                    );
                } else {
                    let mut paths = Vec::new();
                    for statements in [body, alternate] {
                        let mut branch = facts.clone();
                        scan_outcome_statements(
                            result,
                            file,
                            statements,
                            imports,
                            returns,
                            &mut branch,
                            depth + 1,
                        );
                        if !must_use_sequence_terminates(statements, 0) {
                            paths.push(branch);
                        }
                    }
                    *facts = merge_outcome_facts(&paths);
                }
            }
            Flow::Match {
                subject,
                cases,
                range: _,
            } => {
                let Some(known) = expression_outcomes(subject, facts, imports, returns) else {
                    unsupported(
                        result,
                        file,
                        subject.range,
                        "finite outcome match has an unresolved or open scrutinee",
                    );
                    let mut paths = Vec::new();
                    for case in cases {
                        let mut branch = facts.clone();
                        let mut names = BTreeSet::new();
                        case.pattern.binding_names(&mut names);
                        for name in names {
                            branch.remove(&name);
                        }
                        scan_outcome_statements(
                            result,
                            file,
                            &case.body,
                            imports,
                            returns,
                            &mut branch,
                            depth + 1,
                        );
                        paths.push(branch);
                    }
                    if !paths.is_empty() {
                        paths.push(facts.clone());
                        *facts = merge_outcome_facts(&paths);
                    }
                    continue;
                };
                let mut remaining = known;
                let mut paths = Vec::new();
                let mut uncertain_patterns = Vec::new();
                for case in cases {
                    if remaining.is_empty() {
                        break;
                    }
                    let coverage = pattern_coverage(&case.pattern, imports, &remaining);
                    if coverage.covered.is_empty() && coverage.unresolved.is_empty() {
                        continue;
                    }
                    let guard_false = case
                        .guard
                        .as_ref()
                        .is_some_and(|guard| matches!(guard.kind, AstExprKind::Boolean(false)));
                    if guard_false {
                        let mut failed_guard = facts.clone();
                        let mut names = BTreeSet::new();
                        case.pattern.binding_names(&mut names);
                        for name in names {
                            failed_guard.remove(&name);
                        }
                        paths.push(failed_guard);
                        continue;
                    }
                    let mut branch = facts.clone();
                    let mut names = BTreeSet::new();
                    case.pattern.binding_names(&mut names);
                    for name in names {
                        branch.remove(&name);
                    }
                    scan_outcome_statements(
                        result,
                        file,
                        &case.body,
                        imports,
                        returns,
                        &mut branch,
                        depth + 1,
                    );
                    if !must_use_sequence_terminates(&case.body, 0) {
                        paths.push(branch);
                    }
                    let guard_true = case
                        .guard
                        .as_ref()
                        .is_none_or(|guard| matches!(guard.kind, AstExprKind::Boolean(true)));
                    if !guard_true {
                        let mut failed_guard = facts.clone();
                        let mut names = BTreeSet::new();
                        case.pattern.binding_names(&mut names);
                        for name in names {
                            failed_guard.remove(&name);
                        }
                        paths.push(failed_guard);
                        if !coverage.unresolved.is_empty() {
                            uncertain_patterns.push((case.pattern.range(), coverage.unresolved));
                        }
                    } else {
                        remaining.retain(|variant| !coverage.covered.contains(variant));
                        if !coverage.unresolved.is_empty() {
                            uncertain_patterns.push((case.pattern.range(), coverage.unresolved));
                        }
                    }
                }
                if !remaining.is_empty() {
                    paths.push(facts.clone());
                }
                let mut unresolved = BTreeSet::new();
                for (range, variants) in uncertain_patterns {
                    if !remaining.is_disjoint(&variants) {
                        unsupported(
                            result,
                            file,
                            range,
                            "outcome match pattern is unresolved or tests unsupported payload structure",
                        );
                        unresolved.extend(remaining.intersection(&variants).cloned());
                    }
                }
                remaining.retain(|variant| !unresolved.contains(variant));
                if !remaining.is_empty() {
                    let variants = remaining
                        .iter()
                        .map(|identity| identity.rsplit("::").next().unwrap_or(identity))
                        .collect::<Vec<_>>();
                    result.diagnostics.push(Diagnostic {
                        semantic_identity: remaining.iter().cloned().collect(),
                        code: "TP5001".into(),
                        severity: "warning".into(),
                        message: format!("outcome match leaves {} uncovered", variants.join(", ")),
                        primary: crate::source_span(&file.path, &file.source, subject.range),
                        related: Vec::new(),
                        certainty: if unresolved.is_empty() {
                            "definite"
                        } else {
                            "possible"
                        }
                        .into(),
                    });
                }
                if paths.is_empty() {
                    facts.clear();
                    break;
                }
                *facts = merge_outcome_facts(&paths);
            }
            Flow::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                let mut paths = vec![facts.clone()];
                let mut normal = facts.clone();
                scan_outcome_statements(
                    result,
                    file,
                    body,
                    imports,
                    returns,
                    &mut normal,
                    depth + 1,
                );
                if !alternate.is_empty() {
                    scan_outcome_statements(
                        result,
                        file,
                        alternate,
                        imports,
                        returns,
                        &mut normal,
                        depth + 1,
                    );
                }
                scan_outcome_statements(
                    result,
                    file,
                    finalbody,
                    imports,
                    returns,
                    &mut normal,
                    depth + 1,
                );
                paths.push(normal);
                for handler in handlers {
                    let mut branch = facts.clone();
                    scan_outcome_statements(
                        result,
                        file,
                        &handler.body,
                        imports,
                        returns,
                        &mut branch,
                        depth + 1,
                    );
                    paths.push(branch);
                }
                *facts = merge_outcome_facts(&paths);
            }
            Flow::Loop {
                target,
                body,
                alternate,
                ..
            } => {
                let mut looped = facts.clone();
                if let Some(target) = target {
                    looped.remove(target);
                }
                scan_outcome_statements(
                    result,
                    file,
                    body,
                    imports,
                    returns,
                    &mut looped,
                    depth + 1,
                );
                scan_outcome_statements(
                    result,
                    file,
                    alternate,
                    imports,
                    returns,
                    &mut looped,
                    depth + 1,
                );
                *facts = merge_outcome_facts(&[facts.clone(), looped]);
            }
            Flow::With { body, .. } => {
                scan_outcome_statements(result, file, body, imports, returns, facts, depth + 1);
            }
            Flow::DeleteLocal { names, .. } => {
                for name in names {
                    facts.remove(name);
                }
            }
            Flow::BindFunction { name, .. } | Flow::ClosureDefinition { name, .. } => {
                facts.remove(name);
            }
            _ => {}
        }
        if must_use_statement_terminates(statement, 0) {
            break;
        }
    }
}

fn collect_must_use_locals(
    statements: &[crate::frontend::FlowStatement],
    names: &mut BTreeSet<String>,
) {
    use crate::frontend::FlowStatement as Flow;
    for statement in statements {
        match statement {
            Flow::Assign { name, .. }
            | Flow::EscapingAssign { name, .. }
            | Flow::BindFunction { name, .. } => {
                names.insert(name.clone());
            }
            Flow::AnnotatedDeclaration {
                name: Some(name), ..
            } => {
                names.insert(name.clone());
            }
            Flow::If {
                body, alternate, ..
            }
            | Flow::Loop {
                body, alternate, ..
            } => {
                collect_must_use_locals(body, names);
                collect_must_use_locals(alternate, names);
            }
            Flow::Match { cases, .. } => {
                for case in cases {
                    case.pattern.binding_names(names);
                    collect_must_use_locals(&case.body, names);
                }
            }
            Flow::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                collect_must_use_locals(body, names);
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        names.insert(name.clone());
                    }
                    collect_must_use_locals(&handler.body, names);
                }
                collect_must_use_locals(alternate, names);
                collect_must_use_locals(finalbody, names);
            }
            Flow::With { items, body, .. } => {
                for item in items {
                    if let Some(name) = &item.target {
                        names.insert(name.clone());
                    }
                }
                collect_must_use_locals(body, names);
            }
            _ => {}
        }
    }
}

pub(super) fn resolve_marked_callable(
    name: &str,
    marked: &BTreeMap<String, MustUseCallable>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> Option<(String, MustUseCallable)> {
    if let Some(declaration) = marked.get(name) {
        return Some((name.to_owned(), declaration.clone()));
    }
    if let Some((class, Some(receiver))) = class_scope {
        if let Some(method) = name.strip_prefix(&format!("{receiver}.")) {
            let resolved = format!("{class}.{method}");
            if let Some(declaration) = marked.get(&resolved) {
                return Some((resolved, declaration.clone()));
            }
        }
    }
    let (receiver, method) = name.split_once('.')?;
    let class = instances.get(receiver)?;
    let resolved = format!("{class}.{method}");
    marked
        .get(&resolved)
        .cloned()
        .map(|declaration| (resolved, declaration))
}

fn expression_has_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, MustUseCallable>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    match &expression.kind {
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            expression_path(function).is_some_and(|name| {
                resolve_marked_callable(&name, marked, instances, class_scope).is_some()
            }) || arguments.iter().any(|argument| {
                expression_has_marked_call(argument, marked, instances, class_scope)
            }) || keywords
                .iter()
                .any(|(_, value)| expression_has_marked_call(value, marked, instances, class_scope))
        }
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            expression_has_marked_call(value, marked, instances, class_scope)
        }
        AstExprKind::Subscript(value, arguments) => {
            expression_has_marked_call(value, marked, instances, class_scope)
                || arguments.iter().any(|argument| {
                    expression_has_marked_call(argument, marked, instances, class_scope)
                })
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| expression_has_marked_call(argument, marked, instances, class_scope)),
        AstExprKind::OtherCalls(calls, _) => calls
            .iter()
            .any(|name| resolve_marked_callable(name, marked, instances, class_scope).is_some()),
        _ => false,
    }
}

/// Calls passed directly as an argument are considered immediate handling.
/// A marked call wrapped in another expression, await, container, or computed
/// callee remains an unqualified boundary.
fn has_unsupported_nested_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, MustUseCallable>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    match &expression.kind {
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            expression_has_marked_call(function, marked, instances, class_scope)
                || arguments
                    .iter()
                    .chain(keywords.iter().map(|(_, value)| value))
                    .any(|argument| {
                        if direct_marked_call(argument, marked, instances, class_scope) {
                            has_unsupported_nested_marked_call(
                                argument,
                                marked,
                                instances,
                                class_scope,
                            )
                        } else {
                            expression_has_marked_call(argument, marked, instances, class_scope)
                        }
                    })
        }
        AstExprKind::Attribute(value, _) | AstExprKind::Await(value) => {
            expression_has_marked_call(value, marked, instances, class_scope)
        }
        AstExprKind::Subscript(value, arguments) => {
            expression_has_marked_call(value, marked, instances, class_scope)
                || arguments.iter().any(|argument| {
                    expression_has_marked_call(argument, marked, instances, class_scope)
                })
        }
        AstExprKind::Tuple(arguments) | AstExprKind::Collection(arguments) => arguments
            .iter()
            .any(|argument| expression_has_marked_call(argument, marked, instances, class_scope)),
        AstExprKind::OtherCalls(..)
        | AstExprKind::Other
        | AstExprKind::Starred(_)
        | AstExprKind::DepthLimit => {
            expression_has_marked_call(expression, marked, instances, class_scope)
        }
        _ => false,
    }
}

fn direct_marked_call(
    expression: &AstExpr,
    marked: &BTreeMap<String, MustUseCallable>,
    instances: &BTreeMap<String, String>,
    class_scope: Option<(&str, Option<&str>)>,
) -> bool {
    let AstExprKind::Call { function, .. } = &expression.kind else {
        return false;
    };
    expression_path(function).is_some_and(|name| {
        resolve_marked_callable(&name, marked, instances, class_scope).is_some()
    })
}

fn merge_equal_bindings<K: Ord + Clone, V: PartialEq + Clone>(
    left: &mut BTreeMap<K, V>,
    right: &BTreeMap<K, V>,
) {
    left.retain(|name, value| right.get(name).is_some_and(|other| other == value));
}

fn must_use_sequence_terminates(
    statements: &[crate::frontend::FlowStatement],
    loop_depth: usize,
) -> bool {
    statements
        .iter()
        .any(|statement| must_use_statement_terminates(statement, loop_depth))
}

fn must_use_statement_terminates(
    statement: &crate::frontend::FlowStatement,
    loop_depth: usize,
) -> bool {
    use crate::frontend::FlowStatement as Flow;
    match statement {
        Flow::Return { .. } | Flow::Raise { .. } => true,
        Flow::Break { .. } | Flow::Continue { .. } => loop_depth > 0,
        Flow::If {
            test,
            body,
            alternate,
            ..
        } => match test.kind {
            AstExprKind::Boolean(value) => {
                must_use_sequence_terminates(if value { body } else { alternate }, loop_depth)
            }
            _ => {
                !alternate.is_empty()
                    && must_use_sequence_terminates(body, loop_depth)
                    && must_use_sequence_terminates(alternate, loop_depth)
            }
        },
        Flow::Try {
            body,
            handlers,
            alternate,
            finalbody,
            ..
        } => {
            must_use_sequence_terminates(finalbody, loop_depth)
                || (must_use_sequence_terminates(body, loop_depth)
                    && (alternate.is_empty()
                        || must_use_sequence_terminates(alternate, loop_depth))
                    && handlers
                        .iter()
                        .all(|handler| must_use_sequence_terminates(&handler.body, loop_depth)))
        }
        Flow::With { body, .. } => must_use_sequence_terminates(body, loop_depth),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_must_use_body(
    result: &mut Inspection,
    file: &SourceFile,
    statements: &[crate::frontend::FlowStatement],
    marked: &mut BTreeMap<String, MustUseCallable>,
    instances: &mut BTreeMap<String, String>,
    imports: &BTreeMap<String, String>,
    known_classes: &BTreeSet<String>,
    class_scope: Option<(&str, Option<&str>)>,
    loop_depth: usize,
) {
    use crate::frontend::FlowStatement as Flow;
    for statement in statements {
        match statement {
            Flow::Expression { value, .. } => {
                let call = match &value.kind {
                    AstExprKind::Call {
                        function,
                        arguments,
                        keywords,
                    } => Some((function.as_ref(), arguments.as_slice(), keywords.as_slice())),
                    _ => None,
                };
                let discard = call.is_some_and(|(function, _, _)| {
                    canonical(function, imports).as_deref() == Some("codepact::discard")
                });
                if discard {
                    if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                        unsupported(
                            result,
                            file,
                            value.range,
                            "must-use result is nested in an unsupported explicit discard expression",
                        );
                    }
                    continue;
                }
                let resolved = call.and_then(|(function, _, _)| {
                    expression_path(function).and_then(|name| {
                        resolve_marked_callable(&name, marked, instances, class_scope)
                    })
                });
                if let Some((resolved, declaration)) = resolved {
                    let primary = crate::source_span(&file.path, &file.source, value.range);
                    let provenance = match (&declaration.distribution, &declaration.version) {
                        (Some(distribution), Some(version)) => format!(
                            "; public symbol {}, canonical symbol {}, distribution {distribution} {version}",
                            declaration.public_symbol, declaration.canonical_symbol
                        ),
                        _ => String::new(),
                    };
                    let mut identity = vec![
                        resolved.clone(),
                        declaration.public_symbol.clone(),
                        declaration.canonical_symbol.clone(),
                        declaration.distribution.clone().unwrap_or_default(),
                        declaration.version.clone().unwrap_or_default(),
                    ];
                    if declaration.manifest {
                        identity.push("manifest".into());
                    }
                    result.diagnostics.push(Diagnostic {
                        semantic_identity: identity,
                        code: "TP3001".into(),
                        severity: "warning".into(),
                        message: format!(
                            "must-use result from {resolved} is discarded{provenance}"
                        ),
                        primary,
                        related: vec![declaration.span],
                        certainty: "definite".into(),
                    });
                }
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an expression whose immediate handling is unsupported",
                    );
                }
            }
            Flow::Assign {
                name,
                annotation,
                value,
                ..
            } => {
                if let Some(annotation) = annotation {
                    report_misplaced_must_use_annotation(result, file, annotation, imports);
                }
                if let Some(path) = expression_path(value) {
                    if let Some((_, declaration)) =
                        resolve_marked_callable(&path, marked, instances, class_scope)
                    {
                        marked.insert(name.clone(), declaration);
                    } else {
                        marked.remove(name);
                    }
                    if let Some(class) = instances.get(&path).cloned() {
                        instances.insert(name.clone(), class);
                    } else {
                        instances.remove(name);
                    }
                } else {
                    marked.remove(name);
                    if let AstExprKind::Call { function, .. } = &value.kind {
                        let class =
                            expression_path(function).filter(|class| known_classes.contains(class));
                        if let Some(class) = class {
                            instances.insert(name.clone(), class);
                        } else {
                            instances.remove(name);
                        }
                    } else {
                        instances.remove(name);
                    }
                }
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an assignment expression whose immediate handling is unsupported",
                    );
                }
            }
            Flow::EscapingAssign {
                name,
                annotation,
                value,
                ..
            } => {
                if let Some(annotation) = annotation {
                    report_misplaced_must_use_annotation(result, file, annotation, imports);
                }
                if expression_has_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use callable escapes through an unsupported assignment",
                    );
                }
                marked.remove(name);
                instances.remove(name);
            }
            Flow::AnnotatedDeclaration {
                name, annotation, ..
            } => {
                report_misplaced_must_use_annotation(result, file, annotation, imports);
                if let Some(name) = name {
                    marked.remove(name);
                    instances.remove(name);
                }
            }
            Flow::Return {
                value: Some(value), ..
            } => {
                if has_unsupported_nested_marked_call(value, marked, instances, class_scope) {
                    unsupported(
                        result,
                        file,
                        value.range,
                        "must-use result is nested in an unsupported returned expression",
                    );
                }
            }
            Flow::Raise {
                exception, cause, ..
            } => {
                for expression in exception.iter().chain(cause.iter()) {
                    if expression_has_marked_call(expression, marked, instances, class_scope) {
                        unsupported(
                            result,
                            file,
                            expression.range,
                            "must-use result is evaluated in an unsupported raise expression",
                        );
                    }
                }
            }
            Flow::If {
                body, alternate, ..
            } => {
                let mut left_marked = marked.clone();
                let mut left_instances = instances.clone();
                let mut right_marked = marked.clone();
                let mut right_instances = instances.clone();
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut left_marked,
                    &mut left_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                walk_must_use_body(
                    result,
                    file,
                    alternate,
                    &mut right_marked,
                    &mut right_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                merge_equal_bindings(&mut left_marked, &right_marked);
                merge_equal_bindings(&mut left_instances, &right_instances);
                *marked = left_marked;
                *instances = left_instances;
            }
            Flow::Match { cases, .. } => {
                let mut outcomes = Vec::new();
                let mut unmatched_possible = true;
                for case in cases {
                    if !unmatched_possible
                        || matches!(
                            case.guard.as_ref().map(|guard| &guard.kind),
                            Some(AstExprKind::Boolean(false))
                        )
                    {
                        continue;
                    }
                    let mut case_marked = marked.clone();
                    let mut case_instances = instances.clone();
                    let mut names = BTreeSet::new();
                    case.pattern.binding_names(&mut names);
                    for name in names {
                        case_marked.remove(&name);
                        case_instances.remove(&name);
                    }
                    let irrefutable = pattern_is_irrefutable(&case.pattern);
                    let guard_always_true = case
                        .guard
                        .as_ref()
                        .is_none_or(|guard| matches!(guard.kind, AstExprKind::Boolean(true)));
                    walk_must_use_body(
                        result,
                        file,
                        &case.body,
                        &mut case_marked,
                        &mut case_instances,
                        imports,
                        known_classes,
                        class_scope,
                        loop_depth,
                    );
                    if !must_use_sequence_terminates(&case.body, loop_depth) {
                        outcomes.push((case_marked, case_instances));
                    }
                    unmatched_possible = !(irrefutable && guard_always_true);
                }
                if unmatched_possible {
                    outcomes.push((marked.clone(), instances.clone()));
                }
                if outcomes.is_empty() {
                    continue;
                }
                let (mut merged_marked, mut merged_instances) = outcomes.remove(0);
                for (case_marked, case_instances) in outcomes {
                    merge_equal_bindings(&mut merged_marked, &case_marked);
                    merge_equal_bindings(&mut merged_instances, &case_instances);
                }
                *marked = merged_marked;
                *instances = merged_instances;
            }
            Flow::Try {
                body,
                handlers,
                alternate,
                finalbody,
                ..
            } => {
                let mut outcomes = Vec::new();
                let mut body_marked = marked.clone();
                let mut body_instances = instances.clone();
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut body_marked,
                    &mut body_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                outcomes.push((body_marked, body_instances));
                for handler in handlers {
                    let mut handler_marked = marked.clone();
                    let mut handler_instances = instances.clone();
                    walk_must_use_body(
                        result,
                        file,
                        &handler.body,
                        &mut handler_marked,
                        &mut handler_instances,
                        imports,
                        known_classes,
                        class_scope,
                        loop_depth,
                    );
                    outcomes.push((handler_marked, handler_instances));
                }
                let (mut merged_marked, mut merged_instances) = outcomes.remove(0);
                for (candidate_marked, candidate_instances) in outcomes {
                    merge_equal_bindings(&mut merged_marked, &candidate_marked);
                    merge_equal_bindings(&mut merged_instances, &candidate_instances);
                }
                if !alternate.is_empty() {
                    walk_must_use_body(
                        result,
                        file,
                        alternate,
                        &mut merged_marked,
                        &mut merged_instances,
                        imports,
                        known_classes,
                        class_scope,
                        loop_depth,
                    );
                }
                walk_must_use_body(
                    result,
                    file,
                    finalbody,
                    &mut merged_marked,
                    &mut merged_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
                *marked = merged_marked;
                *instances = merged_instances;
            }
            Flow::Loop {
                target,
                body,
                alternate,
                ..
            } => {
                let mut loop_marked = marked.clone();
                let mut loop_instances = instances.clone();
                if let Some(name) = target {
                    loop_marked.remove(name);
                    loop_instances.remove(name);
                }
                walk_must_use_body(
                    result,
                    file,
                    body,
                    &mut loop_marked,
                    &mut loop_instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth + 1,
                );
                merge_equal_bindings(marked, &loop_marked);
                merge_equal_bindings(instances, &loop_instances);
                walk_must_use_body(
                    result,
                    file,
                    alternate,
                    marked,
                    instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
            }
            Flow::With { items, body, .. } => {
                for item in items {
                    if let Some(name) = &item.target {
                        marked.remove(name);
                        instances.remove(name);
                    }
                    if item.unsupported_target {
                        marked.clear();
                        instances.clear();
                        unsupported(
                            result,
                            file,
                            item.range,
                            "must-use binding through a destructuring context-manager target is unsupported",
                        );
                    }
                }
                walk_must_use_body(
                    result,
                    file,
                    body,
                    marked,
                    instances,
                    imports,
                    known_classes,
                    class_scope,
                    loop_depth,
                );
            }
            Flow::BindFunction { name, .. } => {
                marked.remove(name);
                instances.remove(name);
            }
            _ => {}
        }
        if must_use_statement_terminates(statement, loop_depth) {
            break;
        }
    }
}

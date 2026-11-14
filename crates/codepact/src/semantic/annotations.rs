use super::*;

#[derive(Clone, Debug)]
pub(super) struct ParsedAnnotation {
    pub(super) base: Option<String>,
    pub(super) access: Access,
    pub(super) origin: &'static str,
    pub(super) has_resource: bool,
    pub(super) closed_by: Option<String>,
    pub(super) access_span: Option<Span>,
    pub(super) closed_by_span: Option<Span>,
    pub(super) conflict: Option<&'static str>,
    pub(super) conflict_spans: Vec<Span>,
    pub(super) unsupported_reason: Option<&'static str>,
}

impl ParsedAnnotation {
    fn plain(base: Option<String>) -> Self {
        Self {
            base,
            access: Access::Unspecified,
            origin: "python-annotation",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct ResolvedMetadata {
    pub(super) access: Access,
    pub(super) access_span: Option<Span>,
    pub(super) closed_by: Option<String>,
    pub(super) closed_by_span: Option<Span>,
}

pub(super) fn merge_annotation_metadata(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    parsed: &ParsedAnnotation,
    inherited_access: Option<(Access, Span)>,
    inherited_lifecycle: Option<(String, Span)>,
    python_version: &str,
) -> Option<ResolvedMetadata> {
    if parsed.has_resource && parsed.closed_by.is_none() {
        unsupported(
            result,
            file,
            range,
            "Resource metadata requires a literal closed_by method",
        );
        return None;
    }
    if parsed
        .closed_by
        .as_ref()
        .is_some_and(|closed_by| !crate::is_python_identifier(closed_by, python_version))
    {
        declaration_error(
            result,
            file,
            range,
            "TP4002",
            "closed_by must be a valid method identifier",
            Vec::new(),
        );
        return None;
    }
    if let Some((inherited, inherited_span)) = &inherited_access {
        if parsed.access != Access::Unspecified && parsed.access != *inherited {
            metadata_conflict(
                result,
                parsed
                    .access_span
                    .clone()
                    .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range)),
                inherited_span.clone(),
                "conflicting inherited and explicit access qualifiers",
            );
            return None;
        }
    }
    if let (Some(closed_by), Some((inherited, inherited_span))) =
        (&parsed.closed_by, &inherited_lifecycle)
    {
        if closed_by != inherited {
            metadata_conflict(
                result,
                parsed
                    .closed_by_span
                    .clone()
                    .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range)),
                inherited_span.clone(),
                "conflicting inherited and explicit lifecycle declarations",
            );
            return None;
        }
    }

    let (access, access_span) = if parsed.access != Access::Unspecified {
        (parsed.access, parsed.access_span.clone())
    } else {
        match inherited_access {
            Some((access, span)) => (access, Some(span)),
            None => (Access::Unspecified, None),
        }
    };
    let (closed_by, closed_by_span) = if let Some(closed_by) = &parsed.closed_by {
        (Some(closed_by.clone()), parsed.closed_by_span.clone())
    } else {
        inherited_lifecycle
            .map(|(closed_by, span)| (Some(closed_by), Some(span)))
            .unwrap_or((None, None))
    };
    Some(ResolvedMetadata {
        access,
        access_span,
        closed_by,
        closed_by_span,
    })
}

fn metadata_conflict(result: &mut Inspection, primary: Span, related: Span, message: &str) {
    result.coverage.status = "incomplete".into();
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
        code: "TP4002".into(),
        severity: "error".into(),
        message: message.into(),
        primary,
        related: vec![related],
        certainty: "definite".into(),
    });
}

pub(super) fn is_alias_candidate(expression: &AstExpr, imports: &BTreeMap<String, String>) -> bool {
    match &expression.kind {
        AstExprKind::Name(_) | AstExprKind::Attribute(_, _) => true,
        AstExprKind::Subscript(container, _) => {
            canonical(container, imports).is_some_and(|identity| {
                matches!(
                    identity.as_str(),
                    "typing::Annotated"
                        | "codepact::Borrowed"
                        | "codepact::Owned"
                        | "codepact::BorrowedMut"
                )
            })
        }
        _ => false,
    }
}

pub(super) fn report_annotation_problem(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    parsed: &ParsedAnnotation,
) -> bool {
    if let Some(message) = parsed.conflict {
        let mut spans = parsed.conflict_spans.clone();
        let primary = spans
            .first()
            .cloned()
            .unwrap_or_else(|| crate::source_span(&file.path, &file.source, range));
        if !spans.is_empty() {
            spans.remove(0);
        }
        result.coverage.status = "incomplete".into();
        result.diagnostics.push(Diagnostic {
            semantic_identity: Vec::new(),
            code: "TP4002".into(),
            severity: "error".into(),
            message: message.into(),
            primary,
            related: spans,
            certainty: "definite".into(),
        });
        return true;
    }
    if let Some(message) = parsed.unsupported_reason {
        unsupported(result, file, range, message);
        return true;
    }
    false
}

pub(super) fn lower_function_annotation(
    result: &mut Inspection,
    file: &SourceFile,
    expression: &AstExpr,
    display_name: &str,
    state: &FileState,
    symbols: &BTreeMap<String, ClassDecl>,
    bindings: &BTreeMap<String, String>,
) {
    let parsed = parse_annotation(expression, state, file);
    if report_annotation_problem(result, file, expression.range, &parsed) {
        return;
    }
    let Some(base) = parsed.base.as_deref() else {
        if matches!(expression.kind, AstExprKind::String(_)) {
            unsupported(
                result,
                file,
                expression.range,
                "string and future annotations are not resolved",
            );
        } else if parsed.has_resource || parsed.access != Access::Unspecified {
            unsupported(
                result,
                file,
                expression.range,
                "computed annotation base type is unsupported",
            );
        }
        return;
    };
    let Some(symbol) = resolve_symbol(base, &file.module, symbols, &state.aliases, bindings) else {
        if let Some(import) = state.unresolved_imports.get(base) {
            unsupported_related_import(
                result,
                file,
                expression.range,
                &format!(
                    "annotation type {base} comes from unresolved module {}",
                    import.module
                ),
                import.span.clone(),
            );
        } else if parsed.access != Access::Unspecified || parsed.has_resource {
            unsupported(
                result,
                file,
                expression.range,
                &format!("unresolved annotation base type {base}"),
            );
        }
        return;
    };
    let inherited_access = state.alias_access.get(base).cloned();
    let inherited_lifecycle = state.alias_lifecycle.get(base).cloned().or_else(|| {
        symbols.get(&symbol).and_then(|declaration| {
            declaration
                .closed_by
                .as_ref()
                .zip(declaration.closed_by_span.as_ref())
                .map(|(closed_by, span)| (closed_by.clone(), span.clone()))
        })
    });
    let Some(metadata) = merge_annotation_metadata(
        result,
        file,
        expression.range,
        &parsed,
        inherited_access,
        inherited_lifecycle,
        &state.python_version,
    ) else {
        return;
    };
    let declaration = symbols.get(&symbol);
    let lifecycle = metadata.closed_by.map(|closed_by| Lifecycle { closed_by });
    let mut provenance = vec![Provenance {
        origin: parsed.origin.into(),
        trust: "declared-unverified".into(),
    }];
    if declaration.is_some_and(|declaration| declaration.closed_by.is_some()) {
        provenance.push(Provenance {
            origin: "python-resource-decorator".into(),
            trust: "declared-unverified".into(),
        });
    }
    add_contract(
        result,
        Contract {
            schema_version: 0,
            base_symbol: symbol,
            display_name: display_name.to_owned(),
            lifecycle,
            access: metadata.access.as_str().into(),
            source: crate::source_span(&file.path, &file.source, expression.range),
            provenance,
        },
    );
}

pub(super) fn parse_annotation(
    expression: &AstExpr,
    state: &FileState,
    file: &SourceFile,
) -> ParsedAnnotation {
    let imports = &state.imports;
    let AstExprKind::Subscript(container, arguments) = &expression.kind else {
        if let Some(path) = expression_path(expression) {
            if let Some(identity) = canonical(expression, imports) {
                let access = match identity.as_str() {
                    "codepact::Borrowed" => Access::Shared,
                    "codepact::Owned" => Access::Owned,
                    "codepact::BorrowedMut" => Access::Exclusive,
                    _ => Access::Unspecified,
                };
                if access != Access::Unspecified {
                    return ParsedAnnotation {
                        base: None,
                        access,
                        origin: "explicit-access-alias",
                        has_resource: false,
                        closed_by: None,
                        access_span: Some(crate::source_span(
                            &file.path,
                            &file.source,
                            expression.range,
                        )),
                        closed_by_span: None,
                        conflict: None,
                        conflict_spans: Vec::new(),
                        unsupported_reason: None,
                    };
                }
            }
            return ParsedAnnotation::plain(Some(path));
        }
        return ParsedAnnotation {
            base: None,
            access: Access::Unspecified,
            origin: "python-annotation",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: match expression.kind {
                AstExprKind::String(_) => Some("string and future annotations are not resolved"),
                AstExprKind::Call { .. } | AstExprKind::Other | AstExprKind::OtherCalls(..) => {
                    Some("computed annotation expressions are unsupported")
                }
                AstExprKind::Tuple(_) if contains_codepact_contract(expression, state) => {
                    Some("nested CodePact contracts are unsupported in this annotation form")
                }
                _ => None,
            },
        };
    };
    let Some(identity) = canonical(container, imports) else {
        return opaque_annotation(expression, state);
    };
    let base = arguments.first().and_then(expression_path);
    let access = match identity.as_str() {
        "codepact::Borrowed" => Access::Shared,
        "codepact::Owned" => Access::Owned,
        "codepact::BorrowedMut" => Access::Exclusive,
        _ => Access::Unspecified,
    };
    if access != Access::Unspecified {
        let access_span = crate::source_span(&file.path, &file.source, expression.range);
        let has_resolvable_base = base.is_some();
        return ParsedAnnotation {
            base,
            access,
            origin: "explicit-access-alias",
            has_resource: false,
            closed_by: None,
            access_span: Some(access_span),
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: if arguments.len() == 1 && has_resolvable_base {
                None
            } else {
                Some("access annotations require one resolvable base type")
            },
        };
    }
    if identity != "typing::Annotated" {
        return opaque_annotation(expression, state);
    }
    if arguments.len() < 2 {
        return ParsedAnnotation {
            base,
            access: Access::Unspecified,
            origin: "annotated",
            has_resource: false,
            closed_by: None,
            access_span: None,
            closed_by_span: None,
            conflict: None,
            conflict_spans: Vec::new(),
            unsupported_reason: Some(
                "Annotated requires a base type and at least one metadata item",
            ),
        };
    }

    let mut access_values = Vec::<(Access, Span)>::new();
    let mut resources = Vec::<(Option<String>, Span)>::new();
    let mut unsupported_reason = None;
    for metadata in arguments.iter().skip(1) {
        let AstExprKind::Call {
            function,
            arguments: call_arguments,
            keywords,
        } = &metadata.kind
        else {
            continue;
        };
        let Some(identity) = canonical(function, imports) else {
            continue;
        };
        match identity.as_str() {
            "codepact::contracts::Resource" => {
                let closed_by = parse_resource_metadata_call(call_arguments, keywords);
                if closed_by.is_none() {
                    unsupported_reason =
                        Some("Resource metadata supports one literal closed_by value");
                }
                resources.push((
                    closed_by,
                    crate::source_span(&file.path, &file.source, metadata.range),
                ));
            }
            "codepact::contracts::SharedBorrow"
            | "codepact::contracts::OwnedAccess"
            | "codepact::contracts::ExclusiveBorrow" => {
                let access = match identity.as_str() {
                    "codepact::contracts::SharedBorrow" => Access::Shared,
                    "codepact::contracts::OwnedAccess" => Access::Owned,
                    _ => Access::Exclusive,
                };
                access_values.push((
                    access,
                    crate::source_span(&file.path, &file.source, metadata.range),
                ));
                if !call_arguments.is_empty() || !keywords.is_empty() {
                    unsupported_reason = Some("access metadata calls must not have arguments");
                }
            }
            _ => {}
        }
    }
    let base_is_missing = base.is_none();
    if base_is_missing {
        unsupported_reason = Some("Annotated requires a resolvable base type");
    }
    let access = access_values
        .first()
        .map(|(access, _)| *access)
        .unwrap_or(Access::Unspecified);
    let access_conflict = access_values
        .iter()
        .any(|(candidate, _)| *candidate != access);
    let resource_conflict = resources
        .first()
        .is_some_and(|first| resources.iter().any(|resource| resource.0 != first.0));
    let mut conflict_spans = Vec::new();
    let conflict = if access_conflict {
        conflict_spans.extend(access_values.iter().map(|(_, span)| span.clone()));
        Some("multiple incompatible access qualifiers")
    } else if resource_conflict {
        conflict_spans.extend(resources.iter().map(|(_, span)| span.clone()));
        Some("conflicting lifecycle declarations")
    } else {
        None
    };
    let closed_by = resources.first().and_then(|resource| resource.0.clone());
    let access_span = access_values.first().map(|(_, span)| span.clone());
    let closed_by_span = resources.first().map(|(_, span)| span.clone());
    ParsedAnnotation {
        base,
        access,
        origin: if access != Access::Unspecified {
            "annotated-access-metadata"
        } else if !resources.is_empty() {
            "annotated-resource-metadata"
        } else {
            "annotated"
        },
        has_resource: !resources.is_empty(),
        closed_by,
        access_span,
        closed_by_span,
        conflict,
        conflict_spans,
        unsupported_reason,
    }
}

fn parse_resource_metadata_call(
    arguments: &[AstExpr],
    keywords: &[(Option<String>, AstExpr)],
) -> Option<String> {
    match (arguments, keywords) {
        (
            [AstExpr {
                kind: AstExprKind::String(method),
                ..
            }],
            [],
        ) => Some(method.clone()),
        (
            [],
            [(
                Some(name),
                AstExpr {
                    kind: AstExprKind::String(method),
                    ..
                },
            )],
        ) if name == "closed_by" => Some(method.clone()),
        _ => None,
    }
}

pub(super) fn expression_path(expression: &AstExpr) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) => Some(name.clone()),
        AstExprKind::Attribute(value, attr) => {
            Some(format!("{}.{}", expression_path(value)?, attr))
        }
        _ => None,
    }
}

pub(super) fn canonical(
    expression: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> Option<String> {
    canonical_path(&expression_path(expression)?, imports)
}

pub(super) fn canonical_path(path: &str, imports: &BTreeMap<String, String>) -> Option<String> {
    if let Some(value) = imports.get(path).filter(|value| !value.is_empty()) {
        return Some(normalize_canonical_identity(value));
    }
    let components = path.split('.').collect::<Vec<_>>();
    for prefix_len in (1..components.len()).rev() {
        let prefix = components[..prefix_len].join(".");
        let Some(root_identity) = imports.get(&prefix).filter(|value| !value.is_empty()) else {
            continue;
        };
        let suffix = components[prefix_len..].join("::");
        return Some(normalize_canonical_identity(&format!(
            "{root_identity}::{suffix}"
        )));
    }
    None
}

fn normalize_canonical_identity(identity: &str) -> String {
    match identity {
        "codepact::rust::Borrowed" | "codepact::contracts::Borrowed" => "codepact::Borrowed".into(),
        "codepact::rust::Owned" | "codepact::contracts::Owned" => "codepact::Owned".into(),
        "codepact::rust::BorrowedMut" | "codepact::contracts::BorrowedMut" => {
            "codepact::BorrowedMut".into()
        }
        _ => identity.to_owned(),
    }
}

fn opaque_annotation(expression: &AstExpr, state: &FileState) -> ParsedAnnotation {
    let mut parsed = ParsedAnnotation::plain(None);
    if contains_codepact_contract(expression, state) {
        parsed.unsupported_reason =
            Some("nested CodePact contracts are unsupported in this annotation form");
    }
    parsed
}

pub(super) fn contains_codepact_contract(expression: &AstExpr, state: &FileState) -> bool {
    if canonical(expression, &state.imports).is_some_and(|identity| {
        matches!(
            identity.as_str(),
            "codepact::Borrowed"
                | "codepact::Owned"
                | "codepact::BorrowedMut"
                | "codepact::contracts::Resource"
                | "codepact::contracts::SharedBorrow"
                | "codepact::contracts::OwnedAccess"
                | "codepact::contracts::ExclusiveBorrow"
        )
    }) {
        return true;
    }
    if expression_path(expression).is_some_and(|path| {
        state.alias_access.contains_key(&path)
            || state.alias_lifecycle.contains_key(&path)
            || state
                .exports
                .get(&path)
                .is_some_and(|view| view.closed_by.is_some() || view.access != Access::Unspecified)
    }) {
        return true;
    }
    match &expression.kind {
        AstExprKind::Attribute(value, _) => contains_codepact_contract(value, state),
        AstExprKind::Subscript(container, arguments) => {
            contains_codepact_contract(container, state)
                || arguments
                    .iter()
                    .any(|argument| contains_codepact_contract(argument, state))
        }
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            contains_codepact_contract(function, state)
                || arguments
                    .iter()
                    .any(|argument| contains_codepact_contract(argument, state))
                || keywords
                    .iter()
                    .any(|(_, value)| contains_codepact_contract(value, state))
        }
        AstExprKind::Tuple(values) | AstExprKind::Collection(values) => values
            .iter()
            .any(|value| contains_codepact_contract(value, state)),
        _ => false,
    }
}

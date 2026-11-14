use super::*;

pub(super) fn span(file: &SourceFile, range: SourceRange) -> Span {
    crate::source_span(&file.path, &file.source, range)
}

pub(super) fn contract_identity(contract: Option<&Contract>) -> Vec<String> {
    contract
        .into_iter()
        .map(|contract| {
            serde_json::to_string(&(
                &contract.base_symbol,
                &contract.lifecycle,
                &contract.access,
                &contract.provenance,
            ))
            .expect("canonical contract identity serializes")
        })
        .collect()
}

pub(super) fn report(
    result: &mut Inspection,
    code: &str,
    message: &str,
    primary: Span,
    related: Vec<Span>,
    certainty: &str,
    identity: Vec<String>,
) {
    if let Some(existing) = result.diagnostics.iter_mut().find(|diagnostic| {
        diagnostic.code == code
            && diagnostic.primary == primary
            && (code != "TP1002"
                || (diagnostic.message == message && diagnostic.related.first() == related.first()))
    }) {
        // The same source operation can be reached along several exits. Keep a
        // definite finding only when every analyzed path makes it definite.
        if existing.certainty != certainty {
            existing.certainty = "possible".into();
            existing.severity = "warning".into();
        }
        for related_span in related {
            if !existing.related.contains(&related_span) {
                existing.related.push(related_span);
            }
        }
        existing.semantic_identity.extend(identity);
        existing.semantic_identity.sort();
        existing.semantic_identity.dedup();
        return;
    }
    result.diagnostics.push(Diagnostic {
        semantic_identity: identity,
        code: code.into(),
        severity: if certainty == "definite" {
            "error"
        } else {
            "warning"
        }
        .into(),
        message: message.into(),
        primary,
        related,
        certainty: certainty.into(),
    });
}

pub(super) fn report_cleanup_exit(
    result: &mut Inspection,
    file: &SourceFile,
    exit: &FlowExit,
    return_access: Option<&str>,
    functions: &FunctionMap,
    classes_with_unresolved_bases: &BTreeSet<String>,
) {
    report_unawaited_coroutines(&exit.state, file, result);
    let exit_span = span(file, exit.range);
    let mut returned_owned = BTreeSet::new();
    let mut escaped_return = BTreeSet::new();
    if let CompletionKind::Return(value) = &exit.kind {
        let targets = exit
            .state
            .pending_return_targets
            .clone()
            .or_else(|| {
                if exit.state.ownership.is_some() {
                    return None;
                }
                value
                    .as_ref()
                    .and_then(expr_name)
                    .filter(|name| !exit.state.unknown_places.contains(*name))
                    .and_then(|name| exit.state.places.get(name).cloned())
            })
            .unwrap_or_default();
        if return_access == Some("owned") {
            returned_owned.extend(
                targets
                    .iter()
                    .filter(|object| {
                        exit.state.objects.get(*object).copied().unwrap_or(UNKNOWN) == OPEN
                            && !exit.state.borrowed.contains(*object)
                    })
                    .cloned(),
            );
        } else {
            escaped_return.extend(targets);
        }
    }
    if !escaped_return.is_empty() {
        boundary(
            result,
            file,
            exit.range,
            "resource returned without an Owned contract; duty transfer is unknown",
        );
    }
    if matches!(exit.kind, CompletionKind::Return(_)) && return_access == Some("owned") {
        let returned_targets = exit
            .state
            .pending_return_targets
            .clone()
            .or_else(|| match &exit.kind {
                CompletionKind::Return(Some(value)) if exit.state.ownership.is_none() => {
                    expr_name(value)
                        .filter(|name| !exit.state.unknown_places.contains(*name))
                        .and_then(|name| exit.state.places.get(name).cloned())
                }
                _ => None,
            })
            .unwrap_or_default();
        for object in &returned_targets {
            let mask = exit.state.objects.get(object).copied().unwrap_or(UNKNOWN);
            if exit.state.borrowed.contains(object) {
                report(
                    result,
                    "TP1003",
                    "a Borrowed resource cannot be returned as Owned",
                    exit_span.clone(),
                    exit.state
                        .borrowed_origins
                        .get(object)
                        .cloned()
                        .into_iter()
                        .collect(),
                    "definite",
                    contract_identity(exit.state.resources.get(object)),
                );
                continue;
            }
            if mask & CLOSED != 0 {
                report(
                    result,
                    "TP1003",
                    "an Owned return must return an open resource",
                    exit_span.clone(),
                    exit.state
                        .close_origins
                        .get(object)
                        .cloned()
                        .into_iter()
                        .collect(),
                    if mask & (OPEN | UNKNOWN) == 0 {
                        "definite"
                    } else {
                        "possible"
                    },
                    contract_identity(exit.state.resources.get(object)),
                );
            }
            if mask & UNKNOWN != 0 {
                boundary(
                    result,
                    file,
                    exit.range,
                    "Owned return state is unknown; handoff is not established",
                );
            }
        }
    }
    let exit_kind = match &exit.kind {
        CompletionKind::Fallthrough => "fallthrough",
        CompletionKind::Return(_) => "return",
        CompletionKind::Raise(_) => "raise",
        CompletionKind::Cancelled => "cancelled",
        CompletionKind::Break => "break",
        CompletionKind::Continue => "continue",
    };
    for (object, origin) in &exit.state.duty_origins {
        if returned_owned.contains(object) || escaped_return.contains(object) {
            continue;
        }
        let mask = exit.state.objects.get(object).copied().unwrap_or(UNKNOWN);
        if mask & OPEN == 0 {
            if mask & UNKNOWN != 0 {
                boundary(
                    result,
                    file,
                    exit.range,
                    "cleanup duty may escape with unknown resource state",
                );
            }
            continue;
        }
        let resource = exit.state.resources.get(object);
        let has_finalizer = resource
            .is_some_and(|resource| resource_method(resource, "__del__", functions).is_some());
        let has_unresolved_bases = resource
            .is_some_and(|resource| classes_with_unresolved_bases.contains(&resource.base_symbol));
        if has_finalizer || has_unresolved_bases {
            boundary(
                result,
                file,
                exit.range,
                if has_finalizer {
                    "resource finalizer may run implicitly; its cleanup effect is unknown"
                } else {
                    "resource base-class finalizer effects are not modeled"
                },
            );
        }
        let certainty = if mask & (CLOSED | UNKNOWN) == 0 && !has_finalizer && !has_unresolved_bases
        {
            "definite"
        } else {
            "possible"
        };
        let mut related = vec![origin.clone()];
        if let Some(ownership) = exit.state.ownership.as_ref() {
            related.extend(ownership.transfer_sites(object).cloned());
        }
        if let Some(close) = exit.state.close_origins.get(object) {
            related.push(close.clone());
        }
        if let Some(existing) = result.diagnostics.iter_mut().find(|diagnostic| {
            diagnostic.code == "TP1002"
                && diagnostic.primary == exit_span
                && diagnostic.message.ends_with(&format!("on {exit_kind}"))
                && diagnostic.related.first() == Some(origin)
        }) {
            existing
                .semantic_identity
                .extend(contract_identity(resource));
            existing.semantic_identity.sort();
            existing.semantic_identity.dedup();
            for span in related {
                if !existing.related.contains(&span) {
                    existing.related.push(span);
                }
            }
            if certainty == "definite" {
                existing.certainty = "definite".into();
                existing.severity = "error".into();
            }
        } else {
            report(
                result,
                "TP1002",
                &format!("owned resource cleanup obligation remains open on {exit_kind}"),
                exit_span.clone(),
                related,
                certainty,
                contract_identity(resource),
            );
        }
    }
}

pub(super) fn report_module_scope_lifetimes(
    result: &mut Inspection,
    normal: &State,
    outcomes: &FlowResult,
) {
    let mut states = outcomes
        .all_abrupt()
        .map(|exit| &exit.state)
        .collect::<Vec<_>>();
    if normal.alive {
        states.push(normal);
    }

    for state in states {
        let unresolved_origins = state
            .duty_origins
            .iter()
            .filter_map(|(object, origin)| {
                let mask = state.objects.get(object).copied().unwrap_or(UNKNOWN);
                (mask & (OPEN | UNKNOWN) != 0).then(|| origin.clone())
            })
            .collect::<Vec<_>>();
        for origin in unresolved_origins {
            boundary_at_span(
                result,
                origin,
                "module-scope resource lifetime is outside cleanup analysis; module completion is not process shutdown",
            );
        }
    }
}

pub(super) fn grouped_exits(exits: &[FlowExit]) -> Vec<FlowExit> {
    let mut groups = BTreeMap::<(usize, usize, u8), FlowExit>::new();
    for exit in exits {
        let kind = match exit.kind {
            CompletionKind::Fallthrough => 0,
            CompletionKind::Return(_) => 1,
            CompletionKind::Raise(_) => 2,
            CompletionKind::Cancelled => 3,
            CompletionKind::Break => 4,
            CompletionKind::Continue => 5,
        };
        let key = (exit.range.start, exit.range.end, kind);
        if let Some(existing) = groups.get_mut(&key) {
            existing.state = merge(&existing.state, &exit.state).0;
            existing.state.alive = true;
            if let (CompletionKind::Raise(Some(left)), CompletionKind::Raise(Some(right))) =
                (&existing.kind, &exit.kind)
            {
                if left != right {
                    existing.kind = CompletionKind::Raise(None);
                }
            }
        } else {
            groups.insert(key, exit.clone());
        }
    }
    groups.into_values().collect()
}

pub(super) fn boundary(
    result: &mut Inspection,
    file: &SourceFile,
    range: SourceRange,
    message: &str,
) {
    let primary = span(file, range);
    boundary_at_span(result, primary, message);
}

pub(super) fn boundary_at_span(result: &mut Inspection, primary: Span, message: &str) {
    let reason = format!("{}:{}: {message}", primary.file, primary.line);
    result.coverage.status = "incomplete".into();
    if result.coverage.reasons.contains(&reason) {
        return;
    }
    result.coverage.reasons.push(reason);
    result.diagnostics.push(Diagnostic {
        semantic_identity: Vec::new(),
        code: "TP9001".into(),
        severity: "warning".into(),
        message: message.into(),
        primary,
        related: Vec::new(),
        certainty: "unknown".into(),
    });
}

pub(super) fn take_flow_step(
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let within_budget = FLOW_STEPS.with(|steps| {
        let current = steps.get();
        if current >= FLOW_STEP_LIMIT.with(Cell::get) {
            false
        } else {
            steps.set(current + 1);
            true
        }
    });
    if within_budget {
        return true;
    }
    let affected = if let Some(ownership) = env.ownership.as_mut() {
        for group in ownership.groups.values_mut() {
            group.incomplete = true;
        }
        env.objects.keys().cloned().collect::<Vec<_>>()
    } else {
        env.places.values().flatten().cloned().collect::<Vec<_>>()
    };
    weaken(env, affected.into_iter());
    let first = FLOW_BUDGET_REPORTED.with(|reported| {
        if reported.get() {
            false
        } else {
            reported.set(true);
            true
        }
    });
    if first {
        boundary(
            result,
            file,
            range,
            "lifecycle flow work budget exceeded; tracked resources are unknown",
        );
    }
    false
}

pub(crate) fn reset_flow_budget(limit: usize) {
    recursion::reset();
    FLOW_STEPS.with(|steps| steps.set(0));
    FLOW_STEP_LIMIT.with(|budget| budget.set(limit));
    FLOW_BUDGET_REPORTED.with(|reported| reported.set(false));
}

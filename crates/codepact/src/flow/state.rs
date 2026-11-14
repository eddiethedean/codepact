use super::*;

pub(super) type CallKeyword = (Option<String>, AstExpr);
pub(super) type MethodCall<'a> = (&'a AstExpr, &'a str, &'a [AstExpr], &'a [CallKeyword]);
#[derive(Clone)]
pub(super) struct FunctionBody {
    pub(super) file: SourceFile,
    pub(super) parameters: Vec<String>,
    pub(super) signature: CallSignature,
    pub(super) contracts: Arc<[Contract]>,
    pub(super) parameter_contracts: BTreeMap<String, Contract>,
    pub(super) return_access: Option<String>,
    pub(super) statements: Vec<FlowStatement>,
    pub(super) supported_binding: bool,
    pub(super) is_async: bool,
    pub(super) binding_key: Option<String>,
    pub(super) module_bindings: Arc<ModuleBindings>,
}

#[derive(Clone, Default)]
pub(super) struct ModuleBindings {
    pub(super) functions: BTreeMap<String, String>,
    pub(super) classes: BTreeMap<String, Option<Contract>>,
}

pub(super) struct FlowContext<'a> {
    pub(super) functions: &'a FunctionMap,
    pub(super) result: &'a mut Inspection,
}

pub(super) struct MethodInvocation<'a> {
    pub(super) definition: &'a FunctionBody,
    pub(super) object: &'a str,
    pub(super) initial_mask: u8,
    pub(super) arguments: &'a [AstExpr],
    pub(super) keywords: &'a [CallKeyword],
    pub(super) parent: &'a State,
    pub(super) call_key: String,
    pub(super) call_site: String,
    pub(super) caller: &'a SourceFile,
}

pub(super) type ProjectFunctionMap = BTreeMap<(String, String), FunctionBody>;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct State {
    pub(super) ownership: Option<Box<OwnershipState>>,
    pub(super) places: BTreeMap<String, BTreeSet<String>>,
    pub(super) unknown_places: BTreeSet<String>,
    pub(super) possibly_unbound_places: BTreeSet<String>,
    pub(super) definitely_bound_names: BTreeSet<String>,
    pub(super) shadowed_classes: BTreeSet<String>,
    pub(super) shadowed_functions: BTreeSet<String>,
    pub(super) objects: BTreeMap<String, u8>,
    pub(super) resources: BTreeMap<String, Contract>,
    pub(super) borrowed: BTreeSet<String>,
    pub(super) borrowed_origins: BTreeMap<String, Span>,
    pub(super) forbidden_borrowed_closes: BTreeSet<String>,
    pub(super) ambiguous_borrowed_owned_aliases: BTreeSet<String>,
    pub(super) escaped: BTreeSet<String>,
    pub(super) call_stack: BTreeSet<String>,
    pub(super) call_depth: usize,
    pub(super) control_depth: usize,
    pub(super) loop_depth: usize,
    pub(super) function_bindings: BTreeMap<String, String>,
    pub(super) function_bindings_complete: bool,
    pub(super) class_bindings: BTreeMap<String, Option<Contract>>,
    pub(super) module_function_bindings: BTreeMap<String, String>,
    pub(super) module_class_bindings: BTreeMap<String, Option<Contract>>,
    pub(super) imported_classes: BTreeMap<String, Option<Contract>>,
    pub(super) class_bindings_complete: bool,
    pub(super) module_scope: bool,
    pub(super) scope_module: String,
    pub(super) target_python_version: String,
    pub(super) models: Arc<FlowModelCatalog>,
    pub(super) coroutines: BTreeMap<String, CoroutineRecord>,
    pub(super) coroutine_budget_overflow: Option<SourceRange>,
    pub(super) coroutine_aliases: BTreeMap<String, String>,
    pub(super) unknown_coroutine_aliases: BTreeSet<String>,
    pub(super) pending_coroutine_result: Option<String>,
    pub(super) close_origins: BTreeMap<String, Span>,
    pub(super) duty_origins: BTreeMap<String, Span>,
    pub(super) awaited_call: Option<(String, SourceRange)>,
    pub(super) allocation_context: Vec<String>,
    pub(super) pending_return_targets: Option<BTreeSet<String>>,
    pub(super) pending_raises: Vec<FlowExit>,
    pub(super) alive: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CoroutineRecord {
    pub(super) invocation: AstExpr,
    pub(super) file_path: String,
    pub(super) captures: BTreeMap<String, CapturedCoroutinePlace>,
    pub(super) range: SourceRange,
    pub(super) origin: Span,
    pub(super) consumed: bool,
    pub(super) ambiguous: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct CapturedCoroutinePlace {
    pub(super) targets: BTreeSet<String>,
    pub(super) unknown: bool,
    pub(super) transferred: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum CompletionKind {
    Fallthrough,
    Return(Option<AstExpr>),
    Raise(Option<AstExpr>),
    Cancelled,
    Break,
    Continue,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct FlowExit {
    pub(super) state: State,
    pub(super) range: SourceRange,
    pub(super) kind: CompletionKind,
}

#[derive(Clone)]
pub(super) struct ContextCleanup {
    pub(super) receiver: String,
    pub(super) method: &'static str,
    pub(super) asynchronous: bool,
    pub(super) range: SourceRange,
}

#[derive(Clone)]
pub(super) enum ContextCleanupFrame {
    Modeled(ContextCleanup),
    Unknown {
        asynchronous: bool,
        range: SourceRange,
    },
}

#[derive(Default)]
pub(super) struct FlowResult {
    pub(super) returns: Vec<FlowExit>,
    pub(super) raises: Vec<FlowExit>,
    pub(super) breaks: Vec<FlowExit>,
    pub(super) continues: Vec<FlowExit>,
}

impl FlowResult {
    pub(super) fn append(&mut self, mut other: Self) {
        self.returns.append(&mut other.returns);
        self.raises.append(&mut other.raises);
        self.breaks.append(&mut other.breaks);
        self.continues.append(&mut other.continues);
    }

    pub(super) fn all_abrupt(&self) -> impl Iterator<Item = &FlowExit> {
        self.returns
            .iter()
            .chain(&self.raises)
            .chain(&self.breaks)
            .chain(&self.continues)
    }
}

#[derive(Default)]
pub(super) struct JoinEffects {
    pub(super) aliases_over_budget: Vec<String>,
}

pub(super) fn merge(left: &State, right: &State) -> (State, JoinEffects) {
    let mut joined = left.clone();
    if let (Some(left), Some(right)) = (left.ownership.as_deref(), right.ownership.as_deref()) {
        joined.ownership = Some(Box::new(OwnershipState::join(left, right)));
    }
    let mut effects = JoinEffects::default();
    let returned_targets: BTreeSet<_> = left
        .pending_return_targets
        .iter()
        .chain(right.pending_return_targets.iter())
        .flatten()
        .cloned()
        .collect();
    joined.pending_return_targets = if returned_targets.is_empty() {
        None
    } else {
        Some(returned_targets)
    };
    joined.pending_coroutine_result =
        if left.pending_coroutine_result == right.pending_coroutine_result {
            left.pending_coroutine_result.clone()
        } else {
            None
        };
    joined.coroutine_budget_overflow = left
        .coroutine_budget_overflow
        .or(right.coroutine_budget_overflow);
    joined
        .unknown_coroutine_aliases
        .extend(right.unknown_coroutine_aliases.iter().cloned());
    let coroutine_aliases: BTreeSet<_> = left
        .coroutine_aliases
        .keys()
        .chain(right.coroutine_aliases.keys())
        .cloned()
        .collect();
    for alias in coroutine_aliases {
        if left.coroutine_aliases.get(&alias) == right.coroutine_aliases.get(&alias) {
            if left.coroutine_aliases.contains_key(&alias) {
                joined
                    .coroutine_aliases
                    .insert(alias.clone(), left.coroutine_aliases[&alias].clone());
            }
        } else {
            joined.coroutine_aliases.remove(&alias);
            joined.unknown_coroutine_aliases.insert(alias);
        }
    }
    let coroutine_ids: BTreeSet<_> = left
        .coroutines
        .keys()
        .chain(right.coroutines.keys())
        .cloned()
        .collect();
    for id in coroutine_ids {
        match (left.coroutines.get(&id), right.coroutines.get(&id)) {
            (Some(left_record), Some(right_record)) => {
                let record = joined
                    .coroutines
                    .entry(id)
                    .or_insert_with(|| left_record.clone());
                if left_record.consumed != right_record.consumed
                    || left_record.invocation != right_record.invocation
                {
                    record.ambiguous = true;
                }
                record.ambiguous |= right_record.ambiguous;
                record.consumed &= right_record.consumed;
                for (capture, right_place) in &right_record.captures {
                    let place = record.captures.entry(capture.clone()).or_default();
                    place.targets.extend(right_place.targets.iter().cloned());
                    place.unknown |= right_place.unknown;
                    place.transferred &= right_place.transferred;
                }
            }
            (Some(left_record), None) => {
                let mut record = left_record.clone();
                record.ambiguous = true;
                joined.coroutines.insert(id, record);
            }
            (None, Some(right_record)) => {
                let mut record = right_record.clone();
                record.ambiguous = true;
                joined.coroutines.insert(id, record);
            }
            (None, None) => unreachable!(),
        }
    }
    while joined.coroutines.len() > MAX_PENDING_COROUTINES {
        let Some(id) = joined
            .coroutines
            .iter()
            .min_by_key(|(id, record)| (!record.consumed, record.range.start, *id))
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        let Some(record) = joined.coroutines.remove(&id) else {
            continue;
        };
        let mut affected = BTreeSet::new();
        for place in record.captures.values() {
            affected.extend(place.targets.iter().cloned());
            if place.unknown {
                affected.extend(joined.objects.keys().cloned());
            }
        }
        for object in affected {
            *joined.objects.entry(object).or_default() |= UNKNOWN;
        }
        for (alias, coroutine) in &joined.coroutine_aliases {
            if coroutine == &id {
                joined.unknown_coroutine_aliases.insert(alias.clone());
            }
        }
        joined
            .coroutine_aliases
            .retain(|_, coroutine| coroutine != &id);
        joined.coroutine_budget_overflow =
            Some(joined.coroutine_budget_overflow.unwrap_or(record.range));
    }
    joined.pending_raises = merge_pending_raises(&left.pending_raises, &right.pending_raises);
    let binding_names: BTreeSet<_> = left
        .function_bindings
        .keys()
        .chain(right.function_bindings.keys())
        .cloned()
        .collect();
    for name in binding_names {
        if left.function_bindings.get(&name) != right.function_bindings.get(&name) {
            joined.function_bindings.remove(&name);
        }
    }
    let class_names: BTreeSet<_> = left
        .class_bindings
        .keys()
        .chain(right.class_bindings.keys())
        .cloned()
        .collect();
    for name in class_names {
        if left.class_bindings.get(&name) != right.class_bindings.get(&name) {
            joined.class_bindings.remove(&name);
        }
    }
    if left.class_bindings.contains_key(UNKNOWN_EXCEPTION_BINDINGS)
        || right
            .class_bindings
            .contains_key(UNKNOWN_EXCEPTION_BINDINGS)
    {
        joined
            .class_bindings
            .insert(UNKNOWN_EXCEPTION_BINDINGS.to_owned(), None);
    }
    joined
        .unknown_places
        .extend(right.unknown_places.iter().cloned());
    joined
        .possibly_unbound_places
        .extend(right.possibly_unbound_places.iter().cloned());
    joined
        .definitely_bound_names
        .retain(|name| right.definitely_bound_names.contains(name));
    joined
        .shadowed_classes
        .extend(right.shadowed_classes.iter().cloned());
    joined
        .shadowed_functions
        .extend(right.shadowed_functions.iter().cloned());
    let places: BTreeSet<_> = left
        .places
        .keys()
        .chain(right.places.keys())
        .cloned()
        .collect();
    for place in places {
        let left_targets = left.places.get(&place);
        let right_targets = right.places.get(&place);
        let mut targets = left_targets.cloned().unwrap_or_default();
        targets.extend(right_targets.into_iter().flatten().cloned());
        if left_targets.is_some() != right_targets.is_some() {
            joined.unknown_places.insert(place.clone());
            let missing_path_is_unbound = if left_targets.is_some() {
                !right.definitely_bound_names.contains(&place)
                    && !right.unknown_places.contains(&place)
                    && !right
                        .ownership
                        .as_ref()
                        .is_some_and(|ownership| ownership.definitely_moved(&place))
            } else {
                !left.definitely_bound_names.contains(&place)
                    && !left.unknown_places.contains(&place)
                    && !left
                        .ownership
                        .as_ref()
                        .is_some_and(|ownership| ownership.definitely_moved(&place))
            };
            if missing_path_is_unbound {
                joined.possibly_unbound_places.insert(place.clone());
            }
        }
        if targets.len() > MAX_ALIAS_TARGETS {
            joined.places.remove(&place);
            joined.unknown_places.insert(place.clone());
            effects.aliases_over_budget.push(place);
            for object in targets {
                *joined.objects.entry(object).or_default() |= UNKNOWN;
            }
        } else {
            joined.places.insert(place, targets);
        }
    }
    for (object, state) in &right.objects {
        let path_optional = !left.objects.contains_key(object) && state & OPEN != 0;
        *joined.objects.entry(object.clone()).or_default() |=
            *state | if path_optional { UNKNOWN } else { 0 };
    }
    for (object, state) in &left.objects {
        if !right.objects.contains_key(object) && state & OPEN != 0 {
            *joined.objects.entry(object.clone()).or_default() |= UNKNOWN;
        }
    }
    joined.resources.extend(right.resources.clone());
    joined.borrowed.extend(right.borrowed.iter().cloned());
    joined
        .forbidden_borrowed_closes
        .extend(right.forbidden_borrowed_closes.iter().cloned());
    joined
        .ambiguous_borrowed_owned_aliases
        .extend(right.ambiguous_borrowed_owned_aliases.iter().cloned());
    for (object, origin) in &right.borrowed_origins {
        joined
            .borrowed_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.escaped.extend(right.escaped.iter().cloned());
    for (object, origin) in &right.close_origins {
        joined
            .close_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    for (object, origin) in &right.duty_origins {
        joined
            .duty_origins
            .entry(object.clone())
            .or_insert_with(|| origin.clone());
    }
    joined.alive |= right.alive;
    joined.loop_depth = left.loop_depth.max(right.loop_depth);
    (joined, effects)
}

fn completion_tag(kind: &CompletionKind) -> u8 {
    match kind {
        CompletionKind::Fallthrough => 0,
        CompletionKind::Return(_) => 1,
        CompletionKind::Raise(_) => 2,
        CompletionKind::Cancelled => 3,
        CompletionKind::Break => 4,
        CompletionKind::Continue => 5,
    }
}

pub(super) fn merge_pending_raises(left: &[FlowExit], right: &[FlowExit]) -> Vec<FlowExit> {
    let mut merged = left.to_vec();
    for exit in right {
        let key = (exit.range.start, exit.range.end, completion_tag(&exit.kind));
        if let Some(previous) = merged.iter_mut().find(|previous| {
            (
                previous.range.start,
                previous.range.end,
                completion_tag(&previous.kind),
            ) == key
        }) {
            previous.state = merge(&previous.state, &exit.state).0;
            match (&previous.kind, &exit.kind) {
                (CompletionKind::Raise(Some(left)), CompletionKind::Raise(Some(right)))
                    if left != right =>
                {
                    previous.kind = CompletionKind::Raise(None);
                }
                _ => {}
            }
        } else {
            merged.push(exit.clone());
        }
    }
    merged
}

pub(super) fn take_pending_raises(state: &mut State) -> Vec<FlowExit> {
    std::mem::take(&mut state.pending_raises)
}

pub(super) fn queue_raise(state: &mut State, range: SourceRange, exception: Option<AstExpr>) {
    let mut raised_state = state.clone();
    raised_state.alive = true;
    raised_state.pending_raises.clear();
    state.pending_raises.push(FlowExit {
        state: raised_state,
        range,
        kind: CompletionKind::Raise(exception),
    });
}

pub(super) fn project_call_exit(
    parent: &State,
    child: &FlowExit,
    call_range: SourceRange,
) -> FlowExit {
    let mut state = parent.clone();
    state.alive = true;
    state.pending_raises.clear();
    state.pending_return_targets = None;
    if let (Some(parent), Some(child)) = (state.ownership.as_mut(), child.state.ownership.as_ref())
    {
        parent.apply_runtime_effects(child);
    }
    state.coroutine_budget_overflow = child
        .state
        .coroutine_budget_overflow
        .or(parent.coroutine_budget_overflow);
    for object in parent.objects.keys() {
        if let (Some(target), Some(source)) =
            (state.ownership.as_mut(), child.state.ownership.as_ref())
        {
            target.copy_transfers(source, object);
        }
        if child.state.forbidden_borrowed_closes.contains(object) {
            if child.state.escaped.contains(object) {
                state.escaped.insert(object.clone());
            }
            continue;
        }
        if let Some(mask) = child.state.objects.get(object) {
            state.objects.insert(object.clone(), *mask);
        }
        if let Some(resource) = child.state.resources.get(object) {
            state.resources.insert(object.clone(), resource.clone());
        }
        if let Some(origin) = child.state.close_origins.get(object) {
            state.close_origins.insert(object.clone(), origin.clone());
        }
        if child.state.escaped.contains(object) {
            state.escaped.insert(object.clone());
        }
    }
    FlowExit {
        state,
        range: call_range,
        kind: child.kind.clone(),
    }
}

pub(super) fn join_normal_exits(fallthrough: State, returned: Vec<State>) -> State {
    let mut exits = returned.into_iter();
    let initial = if fallthrough.alive {
        fallthrough
    } else if let Some(exit) = exits.next() {
        exit
    } else {
        return fallthrough;
    };
    exits.fold(initial, |joined, exit| merge(&joined, &exit).0)
}

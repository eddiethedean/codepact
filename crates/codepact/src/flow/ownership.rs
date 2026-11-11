use super::*;

pub(super) const MOVE: &str = "external:codepact.move";
pub(super) const TASK_GROUP: &str = "external:asyncio.TaskGroup";
pub(super) const MAX_HANDLES: usize = 4096;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct OwnershipState {
    pub(super) mode: crate::Ownership,
    owners: BTreeMap<String, String>,
    moved: BTreeMap<String, MovedHandle>,
    pub(super) transfer_arguments: BTreeSet<String>,
    pub(super) argument_origins: BTreeMap<String, Span>,
    transfer_origins: BTreeMap<String, Vec<Span>>,
    alias_origins: BTreeMap<String, Span>,
    pub(super) pending_move: bool,
    pub(super) return_owned: bool,
    overflowed: bool,
    closures: BTreeMap<String, BTreeSet<String>>,
    pub(super) groups: BTreeMap<String, TaskGroup>,
    pub(super) group_names: BTreeMap<String, String>,
    pub(super) task_names: BTreeMap<String, (String, String)>,
    pub(super) pending_task: Option<(String, String)>,
    pub(super) incompatible_task_factory: bool,
    pub(super) executing_task: bool,
    pub(super) imported_callables: BTreeMap<(usize, String), String>,
    invalidated_modules: BTreeSet<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct MovedHandle {
    objects: BTreeSet<String>,
    site: Span,
    possible: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct TaskGroup {
    pub(super) tasks: BTreeMap<String, ScheduledTask>,
    pub(super) incomplete: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ScheduledTask {
    pub(super) site: Span,
    pub(super) possible: bool,
    pub(super) cancel_requested: bool,
    pub(super) may_have_started: bool,
    pub(super) cancelled_before_start: bool,
}

impl OwnershipState {
    pub(super) fn new(mode: crate::Ownership, return_owned: bool) -> Option<Box<Self>> {
        mode.is_enabled().then(|| {
            Box::new(Self {
                mode,
                return_owned,
                ..Self::default()
            })
        })
    }

    pub(super) fn for_child(parent: &State, return_owned: bool) -> Option<Box<Self>> {
        let parent = parent.ownership.as_deref()?;
        let mut child = Self::new(parent.mode, return_owned)?;
        child.incompatible_task_factory = parent.incompatible_task_factory;
        child.executing_task = parent.executing_task;
        child.invalidated_modules = parent.invalidated_modules.clone();
        Some(child)
    }

    pub(super) fn join(left: &Self, right: &Self) -> Self {
        let mut joined = left.clone();
        joined
            .owners
            .retain(|name, object| right.owners.get(name) == Some(object));
        joined
            .transfer_arguments
            .retain(|name| right.transfer_arguments.contains(name));
        for (name, moved) in &mut joined.moved {
            moved.possible |= right.moved.get(name).is_none_or(|other| other.possible);
        }
        for (name, other) in &right.moved {
            if let Some(moved) = joined.moved.get_mut(name) {
                moved.objects.extend(other.objects.iter().cloned());
            } else {
                let mut moved = other.clone();
                moved.possible = true;
                joined.moved.insert(name.clone(), moved);
            }
        }
        joined.alias_origins.extend(right.alias_origins.clone());
        for (object, origins) in &right.transfer_origins {
            for origin in origins {
                joined.record_transfer(object, origin.clone());
            }
        }
        joined.pending_move &= right.pending_move;
        joined.overflowed |= right.overflowed;
        joined.incompatible_task_factory |= right.incompatible_task_factory;
        joined
            .invalidated_modules
            .extend(right.invalidated_modules.iter().cloned());
        for (name, captures) in &right.closures {
            joined
                .closures
                .entry(name.clone())
                .or_default()
                .extend(captures.iter().cloned());
        }
        for (group, left_group) in &mut joined.groups {
            if let Some(right_group) = right.groups.get(group) {
                left_group.incomplete |= right_group.incomplete;
                for (id, task) in &mut left_group.tasks {
                    task.possible |= right_group.tasks.get(id).is_none_or(|other| other.possible);
                    task.cancel_requested |= right_group
                        .tasks
                        .get(id)
                        .is_some_and(|other| other.cancel_requested);
                    task.may_have_started |= right_group
                        .tasks
                        .get(id)
                        .is_none_or(|other| other.may_have_started);
                    task.cancelled_before_start &= right_group
                        .tasks
                        .get(id)
                        .is_some_and(|other| other.cancelled_before_start);
                }
                for (id, task) in &right_group.tasks {
                    left_group.tasks.entry(id.clone()).or_insert_with(|| {
                        let mut task = task.clone();
                        task.possible = true;
                        task
                    });
                }
            } else {
                left_group.incomplete = true;
                for task in left_group.tasks.values_mut() {
                    task.possible = true;
                }
            }
        }
        for (id, group) in &right.groups {
            joined.groups.entry(id.clone()).or_insert_with(|| {
                let mut group = group.clone();
                group.incomplete = true;
                group
            });
        }
        joined
            .group_names
            .retain(|name, group| right.group_names.get(name) == Some(group));
        joined
            .task_names
            .retain(|name, task| right.task_names.get(name) == Some(task));
        if joined.pending_task != right.pending_task {
            joined.pending_task = None;
        }
        joined
    }

    pub(super) fn apply_runtime_effects(&mut self, other: &Self) {
        self.incompatible_task_factory |= other.incompatible_task_factory;
        self.invalidated_modules
            .extend(other.invalidated_modules.iter().cloned());
    }

    pub(super) fn invalidate_module(&mut self, binding: String) {
        self.invalidated_modules.insert(binding);
    }

    pub(super) fn rebind(&mut self, name: &str) {
        self.moved.remove(name);
        self.owners.remove(name);
        self.alias_origins.remove(name);
        self.closures.remove(name);
        self.group_names.remove(name);
        self.task_names.remove(name);
    }

    pub(super) fn own(&mut self, name: &str, targets: &BTreeSet<String>) {
        if let Some(object) = targets.first().filter(|_| targets.len() == 1) {
            self.owners.insert(name.to_owned(), object.clone());
        }
    }

    fn record_transfer(&mut self, object: &str, site: Span) {
        let sites = self.transfer_origins.entry(object.to_owned()).or_default();
        if sites.len() < 16 && !sites.contains(&site) {
            sites.push(site);
        }
    }

    pub(super) fn transfer_sites(&self, object: &str) -> impl Iterator<Item = &Span> {
        self.transfer_origins.get(object).into_iter().flatten()
    }

    pub(super) fn copy_transfers(&mut self, from: &Self, object: &str) {
        for site in from.transfer_sites(object) {
            self.record_transfer(object, site.clone());
        }
    }

    pub(super) fn definitely_moved(&self, name: &str) -> bool {
        self.moved.get(name).is_some_and(|moved| !moved.possible)
    }

    pub(super) fn assign(
        &mut self,
        name: &str,
        source: Option<&str>,
        targets: Option<&BTreeSet<String>>,
        receives_owner: bool,
        site: Span,
    ) {
        let moved = source.and_then(|source| self.moved.get(source)).cloned();
        let closure = source.and_then(|source| self.closures.get(source)).cloned();
        let group = source
            .and_then(|source| self.group_names.get(source))
            .cloned();
        let task = self.pending_task.take().or_else(|| {
            source
                .and_then(|source| self.task_names.get(source))
                .cloned()
        });
        if source != Some(name) {
            self.rebind(name);
        }
        if let Some(moved) = moved {
            self.moved.insert(name.to_owned(), moved);
        }
        if let Some(closure) = closure {
            self.closures.insert(name.into(), closure);
        }
        if let Some(group) = group {
            self.group_names.insert(name.into(), group);
        }
        if let Some(task) = task {
            self.task_names.insert(name.into(), task);
        }
        if let Some(targets) = targets {
            if receives_owner && !self.overflowed {
                self.own(name, targets);
            } else if source != Some(name) && !self.overflowed {
                self.alias_origins.insert(name.to_owned(), site);
            }
        }
    }

    pub(super) fn define_closure(&mut self, name: &str, captures: &BTreeSet<String>) {
        self.rebind(name);
        self.closures.insert(name.into(), captures.clone());
    }

    pub(super) fn closure_captures(&self, name: &str) -> impl Iterator<Item = &String> {
        self.closures.get(name).into_iter().flatten()
    }
}

pub(super) fn external_member(module: &str, member: &str) -> Option<&'static str> {
    match (module, member) {
        ("codepact", "move") => Some(MOVE),
        ("asyncio", "TaskGroup") => Some(TASK_GROUP),
        _ => None,
    }
}

pub(super) fn external_identity(expression: &AstExpr, env: &State) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) if !env.shadowed_functions.contains(name) => env
            .function_bindings
            .get(name)
            .filter(|key| key.starts_with("external:"))
            .cloned(),
        AstExprKind::Attribute(base, member) => {
            let base = expr_name(base)?;
            if env.shadowed_functions.contains(base) {
                return None;
            }
            let binding = env.function_bindings.get(base)?;
            if env
                .ownership
                .as_ref()
                .is_some_and(|ownership| ownership.invalidated_modules.contains(binding))
            {
                return None;
            }
            external_member(binding.strip_prefix("external:")?, member).map(str::to_owned)
        }
        _ => None,
    }
}

pub(super) fn check_moved(
    name: &str,
    env: &State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let Some(moved) = env
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.moved.get(name))
    else {
        return false;
    };
    let identity = moved
        .objects
        .iter()
        .flat_map(|object| contract_identity(env.resources.get(object)))
        .collect();
    report(
        result,
        "TP2002",
        &format!("ownership handle {name} is used after move"),
        span(file, range),
        vec![moved.site.clone()],
        if moved.possible {
            "possible"
        } else {
            "definite"
        },
        identity,
    );
    !moved.possible
}

pub(super) fn borrow_escape(
    env: &mut State,
    names: impl IntoIterator<Item = String>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    action: &str,
) {
    let Some(ownership) = env.ownership.as_ref() else {
        return;
    };
    let names = names.into_iter().collect::<BTreeSet<_>>();
    let mut affected = BTreeSet::new();
    let mut definite = false;
    for name in &names {
        check_moved(name, env, file, range, result);
        for name in std::iter::once(name).chain(ownership.closure_captures(name)) {
            if let Some(targets) = env.places.get(name) {
                definite |= !targets.is_empty()
                    && !env.unknown_places.contains(name)
                    && targets.iter().all(|object| env.borrowed.contains(object));
                affected.extend(targets.iter().cloned());
            }
            if let Some(id) = env.coroutine_aliases.get(name) {
                if let Some(record) = env.coroutines.get(id) {
                    affected.extend(
                        record
                            .captures
                            .values()
                            .flat_map(|place| place.targets.iter().cloned()),
                    );
                }
            }
        }
    }
    let borrowed = affected
        .intersection(&env.borrowed)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !borrowed.is_empty() {
        report(
            result,
            "TP2004",
            action,
            span(file, range),
            borrowed
                .iter()
                .filter_map(|object| env.borrowed_origins.get(object).cloned())
                .collect(),
            if definite { "definite" } else { "possible" },
            borrowed
                .iter()
                .flat_map(|object| contract_identity(env.resources.get(object)))
                .collect(),
        );
        env.forbidden_borrowed_closes.extend(borrowed);
    }
    env.escaped.extend(affected.iter().cloned());
    weaken(env, affected.into_iter());
    escape_task_handles(env, &names, file, range, result);
}

pub(super) fn escape_task_handles(
    env: &mut State,
    names: &BTreeSet<String>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(ownership) = env.ownership.as_mut() else {
        return;
    };
    let groups = names
        .iter()
        .filter_map(|name| {
            ownership
                .group_names
                .get(name)
                .or_else(|| ownership.task_names.get(name).map(|(group, _)| group))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    for group in groups {
        if let Some(group) = ownership.groups.get_mut(&group) {
            group.incomplete = true;
        }
        boundary(
            result,
            file,
            range,
            "structured task or group handle escapes the tracked scope",
        );
    }
}

pub(super) fn argument_escapes(
    arguments: &[AstExpr],
    keywords: &[CallKeyword],
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> BTreeSet<String> {
    let names = arguments
        .iter()
        .chain(keywords.iter().map(|(_, value)| value))
        .filter_map(expr_name)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    escape_task_handles(env, &names, file, range, result);
    let mut affected = BTreeSet::new();
    for name in names {
        let captures = env
            .ownership
            .as_ref()
            .into_iter()
            .flat_map(|ownership| ownership.closure_captures(&name));
        affected.extend(
            std::iter::once(&name)
                .chain(captures)
                .filter_map(|name| env.places.get(name))
                .flatten()
                .cloned(),
        );
        if let Some(record) = env
            .coroutine_aliases
            .get(&name)
            .and_then(|id| env.coroutines.get(id))
        {
            affected.extend(
                record
                    .captures
                    .values()
                    .flat_map(|place| place.targets.iter().cloned()),
            );
        }
    }
    affected
}

fn live_capture_sites(env: &State, object: &str) -> Vec<(SourceRange, bool, bool)> {
    let mut sites = Vec::new();
    for (id, coroutine) in &env.coroutines {
        if coroutine.consumed
            || !coroutine
                .captures
                .values()
                .any(|place| place.targets.contains(object))
        {
            continue;
        }
        let task = env.ownership.as_ref().and_then(|ownership| {
            ownership
                .groups
                .values()
                .find_map(|group| group.tasks.get(id))
        });
        sites.push((
            coroutine.range,
            coroutine.ambiguous || task.is_some_and(|task| task.possible),
            task.is_some(),
        ));
    }
    sites
}

pub(super) fn check_close(
    env: &State,
    object: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    possible_receiver: bool,
) {
    if env.ownership.is_none() {
        return;
    }
    let sites = live_capture_sites(env, object);
    if sites.is_empty() {
        return;
    }
    let mut identity = contract_identity(env.resources.get(object));
    if sites.iter().any(|(_, _, scheduled)| *scheduled) {
        if let Some(revision) = crate::model::task_group_revision(&env.target_python_version) {
            identity.push(format!("TPM-TASK-001@{revision}"));
        }
    }
    report(
        result,
        "TP2006",
        "resource closes while a coroutine or task borrow remains outstanding",
        span(file, range),
        sites.iter().map(|(site, _, _)| span(file, *site)).collect(),
        if possible_receiver || sites.iter().all(|(_, possible, _)| *possible) {
            "possible"
        } else {
            "definite"
        },
        identity,
    );
}

pub(super) fn transfer(
    name: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    returning: bool,
) -> Option<BTreeSet<String>> {
    cap_handles(env, file, range, result);
    if check_moved(name, env, file, range, result) {
        return None;
    }
    let ownership = env.ownership.as_ref()?;
    let targets = env.places.get(name).cloned().unwrap_or_default();
    if env.unknown_places.contains(name)
        || ownership.overflowed
        || ownership.moved.contains_key(name)
        || targets.len() > 1
    {
        boundary(
            result,
            file,
            range,
            "ownership transfer cannot establish a single valid source binding",
        );
        return None;
    }
    let Some(object) = targets.first() else {
        if !returning {
            boundary(
                result,
                file,
                range,
                "move source is not a tracked local resource",
            );
        }
        return None;
    };
    let mask = env.objects.get(object).copied().unwrap_or(UNKNOWN);
    let identity = contract_identity(env.resources.get(object));
    let site = span(file, range);
    if env.borrowed.contains(object) {
        report(
            result,
            "TP2004",
            "borrowed view cannot be consumed by an ownership transfer",
            site,
            env.borrowed_origins
                .get(object)
                .cloned()
                .into_iter()
                .collect(),
            "definite",
            identity,
        );
        env.forbidden_borrowed_closes.insert(object.clone());
        return None;
    }
    let conflicts = env
        .places
        .iter()
        .filter(|(alias, values)| alias.as_str() != name && values.contains(object))
        .collect::<Vec<_>>();
    let capture_sites = live_capture_sites(env, object);
    let closure_conflict = ownership
        .closures
        .values()
        .flatten()
        .filter_map(|name| env.places.get(name))
        .flatten()
        .any(|target| target == object);
    let known_conflict = closure_conflict
        || conflicts
            .iter()
            .any(|(alias, values)| values.len() == 1 && !env.unknown_places.contains(*alias))
        || capture_sites.iter().any(|(_, possible, _)| !possible);
    let possible_conflict = !conflicts.is_empty() || !capture_sites.is_empty();
    if known_conflict
        || possible_conflict
        || mask == CLOSED
        || ownership.owners.get(name) != Some(object)
    {
        let mut related = conflicts
            .iter()
            .filter_map(|(alias, _)| {
                ownership
                    .alias_origins
                    .get(*alias)
                    .or_else(|| ownership.argument_origins.get(*alias))
                    .cloned()
            })
            .chain(capture_sites.iter().map(|(range, _, _)| span(file, *range)))
            .collect::<Vec<_>>();
        related.extend(env.duty_origins.get(object).cloned());
        report(
            result,
            "TP2003",
            "ownership transfer requires an open owner without a conflicting live alias or borrow",
            site,
            related,
            if known_conflict || mask == CLOSED || !possible_conflict {
                "definite"
            } else {
                "possible"
            },
            identity,
        );
        return None;
    }
    if mask != OPEN || env.escaped.contains(object) {
        boundary(
            result,
            file,
            range,
            "ownership transfer cannot establish open, unescaped ownership",
        );
        return None;
    }
    let ownership = env.ownership.as_mut().expect("ownership was checked");
    ownership.record_transfer(object, site.clone());
    if !returning {
        ownership.owners.remove(name);
        ownership.moved.insert(
            name.to_owned(),
            MovedHandle {
                objects: targets.clone(),
                site,
                possible: false,
            },
        );
        env.places.remove(name);
    }
    Some(targets)
}

pub(super) fn cap_handles(
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(ownership) = env.ownership.as_mut() else {
        return;
    };
    while ownership.moved.len() > MAX_HANDLES {
        ownership.moved.pop_last();
    }
    if env.places.len().saturating_add(ownership.moved.len()) > MAX_HANDLES && !ownership.overflowed
    {
        ownership.overflowed = true;
        ownership.owners.clear();
        boundary(
            result,
            file,
            range,
            "ownership handle budget exceeded; uniqueness is unknown",
        );
    }
}

pub(super) fn qualified_function_key(name: &str, member: &str, env: &State) -> Option<String> {
    let ownership = env.ownership.as_ref()?;
    let binding = env.function_bindings.get(name)?;
    if env.shadowed_functions.contains(name) || ownership.invalidated_modules.contains(binding) {
        return None;
    }
    let module = binding.strip_prefix("module:")?;
    Some(format!("{module}::{member}"))
}

pub(super) struct PreparedCall {
    pub(super) expression: AstExpr,
    temporaries: Vec<String>,
    pub(super) invalid_receiver: bool,
}

impl PreparedCall {
    pub(super) fn release(self, env: &mut State) {
        fn clear(env: &mut State, names: &[String]) {
            let ownership = env.ownership.as_mut().expect("prepared ownership call");
            for name in names {
                env.places.remove(name);
                env.unknown_places.remove(name);
                env.coroutine_aliases.remove(name);
                env.unknown_coroutine_aliases.remove(name);
                env.function_bindings.remove(name);
                ownership.rebind(name);
                ownership.transfer_arguments.remove(name);
                ownership.argument_origins.remove(name);
            }
        }
        clear(env, &self.temporaries);
        for exit in &mut env.pending_raises {
            clear(&mut exit.state, &self.temporaries);
        }
    }
}

pub(super) fn prepare_call(
    original: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) -> PreparedCall {
    let mut prepared = PreparedCall {
        expression: original.clone(),
        temporaries: Vec::new(),
        invalid_receiver: false,
    };
    let AstExprKind::Call {
        function,
        arguments,
        keywords,
    } = &mut prepared.expression.kind
    else {
        unreachable!("prepare_call requires a call");
    };
    if let AstExprKind::Attribute(receiver, method) = &mut function.kind {
        if let Some(name) = expr_name(receiver).map(str::to_owned) {
            prepared.invalid_receiver = check_moved(&name, env, file, receiver.range, result);
            let qualified = qualified_function_key(&name, method, env)
                .filter(|key| functions.contains_key(key));
            if let Some(qualified) = qualified {
                let name = format!(":callable:{}", original.range.start);
                env.function_bindings.insert(name.clone(), qualified);
                prepared.temporaries.push(name.clone());
                function.kind = AstExprKind::Name(name);
            } else {
                let known_receiver = env.places.contains_key(&name)
                    || env.ownership.as_ref().is_some_and(|ownership| {
                        ownership.group_names.contains_key(&name)
                            || ownership.task_names.contains_key(&name)
                    })
                    || env
                        .function_bindings
                        .get(&name)
                        .is_some_and(|binding| binding.starts_with("external:"));
                if !known_receiver && (!arguments.is_empty() || !keywords.is_empty()) {
                    queue_raise(env, function.range, None);
                }
                if !arguments.is_empty() || !keywords.is_empty() {
                    snapshot_name(receiver, &name, env, file, &mut prepared.temporaries);
                }
            }
        } else {
            consume_expr(receiver, env, file, contracts, functions, result, depth + 1);
            snapshot_result(receiver, env, file, &mut prepared.temporaries);
        }
    } else if let Some(name) = expr_name(function) {
        prepared.invalid_receiver = check_moved(name, env, file, function.range, result);
    } else {
        consume_expr(function, env, file, contracts, functions, result, depth + 1);
    }

    for argument in arguments
        .iter_mut()
        .chain(keywords.iter_mut().map(|(_, value)| value))
    {
        if !env.alive {
            break;
        }
        if let Some(name) = expr_name(argument).map(str::to_owned) {
            check_moved(&name, env, file, argument.range, result);
            snapshot_name(argument, &name, env, file, &mut prepared.temporaries);
        } else if !matches!(
            argument.kind,
            AstExprKind::String(_)
                | AstExprKind::Boolean(_)
                | AstExprKind::NoneLiteral
                | AstExprKind::Primitive
        ) {
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
            snapshot_result(argument, env, file, &mut prepared.temporaries);
        }
    }
    cap_handles(env, file, original.range, result);
    prepared
}

fn argument_name(expression: &AstExpr, temporary_count: usize) -> String {
    // The leading colon keeps these bindings disjoint from Python names.
    format!(":argument:{}:{temporary_count}", expression.range.start)
}

fn snapshot_name(
    expression: &mut AstExpr,
    source: &str,
    env: &mut State,
    file: &SourceFile,
    temporaries: &mut Vec<String>,
) {
    let ownership = env.ownership.as_mut().expect("ownership call");
    let targets = env.places.get(source).cloned();
    let unknown = env.unknown_places.contains(source);
    let coroutine = env.coroutine_aliases.get(source).cloned();
    let closure = ownership.closures.get(source).cloned();
    let group = ownership.group_names.get(source).cloned();
    let task = ownership.task_names.get(source).cloned();
    // Captures restored by the coroutine executor already have value identity.
    if ownership.transfer_arguments.contains(source)
        || targets.is_none()
            && !unknown
            && coroutine.is_none()
            && closure.is_none()
            && group.is_none()
            && task.is_none()
    {
        return;
    }
    let temporary = argument_name(expression, temporaries.len());
    if let Some(targets) = targets {
        env.places.insert(temporary.clone(), targets);
        ownership
            .argument_origins
            .insert(temporary.clone(), span(file, expression.range));
    }
    if unknown {
        env.unknown_places.insert(temporary.clone());
    }
    if let Some(coroutine) = coroutine {
        env.coroutine_aliases.insert(temporary.clone(), coroutine);
    }
    if let Some(closure) = closure {
        ownership.closures.insert(temporary.clone(), closure);
    }
    if let Some(group) = group {
        ownership.group_names.insert(temporary.clone(), group);
    }
    if let Some(task) = task {
        ownership.task_names.insert(temporary.clone(), task);
    }
    expression.kind = AstExprKind::Name(temporary.clone());
    temporaries.push(temporary);
}

fn snapshot_result(
    expression: &mut AstExpr,
    env: &mut State,
    file: &SourceFile,
    temporaries: &mut Vec<String>,
) {
    let ownership = env.ownership.as_mut().expect("ownership call");
    let temporary = argument_name(expression, temporaries.len());
    let targets = env.pending_return_targets.take();
    let coroutine = env.pending_coroutine_result.take();
    let task = ownership.pending_task.take();
    let moved = std::mem::take(&mut ownership.pending_move);
    if let Some(targets) = targets {
        env.places.insert(temporary.clone(), targets);
        ownership
            .argument_origins
            .insert(temporary.clone(), span(file, expression.range));
        if moved {
            ownership.transfer_arguments.insert(temporary.clone());
        }
    } else if let Some(coroutine) = coroutine {
        env.coroutine_aliases.insert(temporary.clone(), coroutine);
    } else if let Some(task) = task {
        ownership.task_names.insert(temporary.clone(), task);
    } else {
        // Keep unsupported argument shapes visible to the existing signature binder.
        if matches!(
            expression.kind,
            AstExprKind::Call { .. } | AstExprKind::Await(_)
        ) {
            expression.kind = AstExprKind::Primitive;
        }
        return;
    }
    expression.kind = AstExprKind::Name(temporary.clone());
    temporaries.push(temporary);
}

pub(super) fn move_call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    contracts: &[Contract],
    functions: &FunctionMap,
    result: &mut Inspection,
    depth: usize,
) {
    let AstExprKind::Call {
        arguments,
        keywords,
        ..
    } = &expression.kind
    else {
        unreachable!("move identity requires a call");
    };
    let argument = match (arguments.as_slice(), keywords.as_slice()) {
        ([value], []) => Some(value),
        ([], [(Some(name), value)]) if name == "value" => Some(value),
        _ => None,
    };
    if let Some(argument) = argument {
        if let Some(name) = expr_name(argument) {
            env.pending_return_targets = transfer(name, env, file, argument.range, result, false);
            env.ownership.as_mut().expect("move call").pending_move =
                env.pending_return_targets.is_some();
        } else {
            consume_expr(argument, env, file, contracts, functions, result, depth + 1);
            env.pending_return_targets = None;
            boundary(
                result,
                file,
                expression.range,
                "explicit move requires a tracked local name",
            );
        }
    } else {
        let prepared = prepare_call(expression, env, file, contracts, functions, result, depth);
        if env.alive {
            binding_failure(BindingFailure::Invalid, env, file, expression.range, result);
        }
        prepared.release(env);
    }
}

pub(super) fn assign_handle(
    name: &str,
    source: Option<&str>,
    receives_owner: bool,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(ownership) = env.ownership.as_mut() else {
        return;
    };
    ownership.assign(
        name,
        source,
        env.places.get(name),
        receives_owner,
        span(file, range),
    );
    cap_handles(env, file, range, result);
}

pub(super) fn return_value(
    value: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) {
    let Some(ownership) = env.ownership.as_ref() else {
        return;
    };
    let returning_owned = ownership.return_owned;
    if let Some(name) = expr_name(value) {
        if returning_owned {
            env.pending_return_targets = transfer(name, env, file, value.range, result, true);
        } else {
            borrow_escape(
                env,
                [name.to_owned()],
                file,
                value.range,
                result,
                "borrowed view escapes through a return",
            );
        }
    }
    if let Some(targets) = env.pending_return_targets.as_ref() {
        if !returning_owned && targets.iter().any(|object| env.borrowed.contains(object)) {
            env.pending_return_targets = None;
        }
    }
}

pub(super) fn verify_call_binding(
    definition: &FunctionBody,
    parameter: &str,
    argument: &AstExpr,
    parent: &State,
    child: &mut State,
    call_site: &Span,
    result: &mut Inspection,
) -> bool {
    let Some(ownership) = parent.ownership.as_ref() else {
        return true;
    };
    let Some(name) = expr_name(argument) else {
        return false;
    };
    let Some(targets) = parent.places.get(name) else {
        return false;
    };
    let explicit = ownership.transfer_arguments.contains(name);
    let expected = definition.parameter_contracts.get(parameter);
    let Some(contract) = expected.filter(|contract| contract.access == "owned") else {
        // A checked function without an Owned parameter may use the view,
        // but its effects cannot acquire the duty.
        if explicit
            || ownership.executing_task
            || targets
                .iter()
                .any(|object| parent.duty_origins.contains_key(object))
        {
            for object in targets {
                child.duty_origins.remove(object);
                if explicit
                    || ownership.executing_task
                    || expected.is_some_and(|contract| contract.access == "shared")
                {
                    child.borrowed.insert(object.clone());
                    child
                        .borrowed_origins
                        .entry(object.clone())
                        .or_insert_with(|| call_site.clone());
                }
            }
        }
        if ownership.executing_task && expected.is_none_or(|contract| contract.access != "shared") {
            boundary_at_span(
                result,
                call_site.clone(),
                "task capture lacks a resolved Borrowed or transferred Owned parameter",
            );
            child.escaped.extend(targets.iter().cloned());
        }
        return false;
    };
    if check_owned_actual(contract, name, targets, parent, call_site, result) {
        child
            .ownership
            .as_mut()
            .expect("ownership call child")
            .own(parameter, targets);
        return true;
    }
    for object in targets {
        child.duty_origins.remove(object);
        // The callee may run in Python, but it cannot certify cleanup from
        // an invalid ownership precondition.
        child.forbidden_borrowed_closes.insert(object.clone());
    }
    false
}

pub(super) fn check_coroutine_arguments(
    definition: &FunctionBody,
    bindings: &BTreeMap<String, &AstExpr>,
    parent: &State,
    call_site: &Span,
    result: &mut Inspection,
) {
    for (parameter, argument) in bindings {
        if let Some(contract) = definition
            .parameter_contracts
            .get(parameter)
            .filter(|contract| contract.access == "owned")
        {
            if let Some((name, targets)) = expr_name(argument)
                .and_then(|name| parent.places.get(name).map(|targets| (name, targets)))
            {
                check_owned_actual(contract, name, targets, parent, call_site, result);
            }
        }
    }
}

fn check_owned_actual(
    contract: &Contract,
    name: &str,
    targets: &BTreeSet<String>,
    parent: &State,
    call_site: &Span,
    result: &mut Inspection,
) -> bool {
    let explicit = parent
        .ownership
        .as_ref()
        .is_some_and(|ownership| ownership.transfer_arguments.contains(name));
    if explicit
        && targets.len() == 1
        && !parent.unknown_places.contains(name)
        && targets.iter().all(|object| {
            parent
                .resources
                .get(object)
                .is_some_and(|resource| resource.base_symbol == contract.base_symbol)
        })
    {
        return true;
    }
    if explicit {
        boundary_at_span(
            result,
            call_site.clone(),
            "moved value does not resolve to the resource type required by the Owned parameter",
        );
        return false;
    }
    let borrowed = targets
        .iter()
        .any(|object| parent.borrowed.contains(object));
    report(
        result,
        if borrowed { "TP2004" } else { "TP2005" },
        if borrowed {
            "borrowed view cannot enter an Owned parameter"
        } else {
            "Owned parameter requires an explicit move of the tracked resource"
        },
        span_for_argument(parent, name, call_site),
        vec![contract.source.clone()],
        if parent.unknown_places.contains(name) {
            "possible"
        } else {
            "definite"
        },
        contract_identity(Some(contract)),
    );
    false
}

fn span_for_argument(parent: &State, name: &str, fallback: &Span) -> Span {
    parent
        .ownership
        .as_ref()
        .and_then(|ownership| ownership.argument_origins.get(name))
        .cloned()
        .unwrap_or_else(|| fallback.clone())
}

pub(super) fn expression_names(expression: &AstExpr) -> BTreeSet<String> {
    fn collect(expression: &AstExpr, names: &mut BTreeSet<String>) {
        match &expression.kind {
            AstExprKind::Name(name) => {
                names.insert(name.clone());
            }
            AstExprKind::Tuple(values) | AstExprKind::Collection(values) => {
                for value in values {
                    collect(value, names);
                }
            }
            AstExprKind::Closure(captures) | AstExprKind::OtherCalls(_, captures) => {
                names.extend(captures.iter().cloned())
            }
            AstExprKind::Attribute(receiver, _)
            | AstExprKind::Await(receiver)
            | AstExprKind::Starred(receiver) => collect(receiver, names),
            AstExprKind::Subscript(base, arguments) => {
                collect(base, names);
                for argument in arguments {
                    collect(argument, names);
                }
            }
            AstExprKind::Call {
                function,
                arguments,
                keywords,
            } => {
                collect(function, names);
                for argument in arguments
                    .iter()
                    .chain(keywords.iter().map(|(_, value)| value))
                {
                    collect(argument, names);
                }
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    collect(expression, &mut names);
    names
}

pub(super) fn stored_names(expression: &AstExpr) -> BTreeSet<String> {
    match expression.kind {
        AstExprKind::Name(_) | AstExprKind::Closure(_) => expression_names(expression),
        _ => BTreeSet::new(),
    }
}

pub(super) fn escape_pending_value(
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mut objects = env.pending_return_targets.take().unwrap_or_default();
    if let Some(record) = env
        .pending_coroutine_result
        .take()
        .and_then(|id| env.coroutines.get(&id))
    {
        objects.extend(
            record
                .captures
                .values()
                .flat_map(|place| place.targets.iter().cloned()),
        );
    }
    if !objects.is_empty() {
        env.escaped.extend(objects.iter().cloned());
        weaken(env, objects.into_iter());
        boundary(
            result,
            file,
            range,
            "evaluated ownership value escapes through an unsupported destination",
        );
    }
}

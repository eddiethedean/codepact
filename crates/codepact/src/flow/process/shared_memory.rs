use super::*;

const SYMBOL: &str = "multiprocessing.shared_memory.SharedMemory";
const MODEL: &str = "TPM-SHM-001";
const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNRESOLVED: u8 = 4;
const MAX_EXPORTS: usize = 8;

#[derive(Clone, PartialEq, Eq)]
enum Alias {
    Mapping(String),
    Name(String),
    View {
        mapping: String,
        export: Option<String>,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Segment {
    name: u8,
    tracker: u8,
    origin: Span,
    removal: Option<Span>,
    owns_unlink: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Mapping {
    segment: String,
    state: u8,
    buffer_property: u8,
    base: u8,
    unknown_exports: bool,
    exports: BTreeMap<String, u8>,
    origin: Span,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct LocalMemory {
    aliases: BTreeMap<String, Alias>,
    segments: BTreeMap<String, Segment>,
    mappings: BTreeMap<String, Mapping>,
}

impl LocalMemory {
    pub(super) fn mapping_count(&self) -> usize {
        self.mappings.len()
    }

    pub(super) fn rebind(&mut self, name: &str) {
        self.aliases.remove(name);
    }

    pub(super) fn bind_alias(&mut self, name: &str, source: Option<&str>) {
        let alias = source.and_then(|source| self.aliases.get(source)).cloned();
        self.rebind(name);
        if let Some(alias) = alias {
            self.aliases.insert(name.into(), alias);
        }
    }

    pub(super) fn has_handle(&self, name: &str) -> bool {
        matches!(
            self.aliases.get(name),
            Some(Alias::Mapping(_) | Alias::View { .. })
        )
    }

    pub(super) fn has_handles(&self) -> bool {
        self.aliases
            .values()
            .any(|alias| !matches!(alias, Alias::Name(_)))
    }

    pub(super) fn join(&mut self, other: &Self) {
        self.aliases
            .retain(|name, alias| other.aliases.get(name) == Some(alias));
        for (id, left) in &mut self.segments {
            if let Some(right) = other.segments.get(id) {
                left.name |= right.name;
                left.tracker |= right.tracker;
                if left.removal != right.removal {
                    left.removal = None;
                }
            } else {
                left.name |= UNRESOLVED;
                left.tracker |= UNRESOLVED;
            }
        }
        for (id, right) in &other.segments {
            self.segments.entry(id.clone()).or_insert_with(|| Segment {
                name: right.name | UNRESOLVED,
                tracker: right.tracker | UNRESOLVED,
                ..right.clone()
            });
        }
        for (id, left) in &mut self.mappings {
            if let Some(right) = other.mappings.get(id) {
                left.state |= right.state;
                left.buffer_property |= right.buffer_property;
                left.base |= right.base;
                left.unknown_exports |= right.unknown_exports;
                for (export, state) in &mut left.exports {
                    *state |= right.exports.get(export).copied().unwrap_or(CLOSED);
                }
                for (export, state) in &right.exports {
                    left.exports.entry(export.clone()).or_insert(CLOSED | state);
                }
            } else {
                left.state |= UNRESOLVED;
            }
        }
        for (id, right) in &other.mappings {
            self.mappings.entry(id.clone()).or_insert_with(|| Mapping {
                state: right.state | UNRESOLVED,
                ..right.clone()
            });
        }
    }

    pub(super) fn escape(
        &mut self,
        names: &BTreeSet<String>,
        file: &SourceFile,
        range: SourceRange,
        result: &mut Inspection,
    ) {
        for name in names {
            let Some(alias) = self.aliases.get(name) else {
                continue;
            };
            match alias {
                Alias::Mapping(id) => {
                    let mapping = self.mappings.get_mut(id).expect("tracked mapping");
                    mapping.state |= UNRESOLVED;
                    mapping.buffer_property |= UNRESOLVED;
                    mapping.base |= UNRESOLVED;
                    mapping.unknown_exports = true;
                    let segment = self
                        .segments
                        .get_mut(&mapping.segment)
                        .expect("tracked name");
                    segment.name |= UNRESOLVED;
                    segment.tracker |= UNRESOLVED;
                }
                Alias::View { mapping, export } => {
                    let mapping = self.mappings.get_mut(mapping).expect("tracked mapping");
                    mapping.unknown_exports = true;
                    match export {
                        Some(export) => {
                            *mapping.exports.get_mut(export).expect("tracked export") |= UNRESOLVED
                        }
                        None => mapping.base |= UNRESOLVED,
                    }
                }
                Alias::Name(_) => continue,
            }
            boundary(
                result,
                file,
                range,
                "shared-memory mapping or view escapes its qualified local protocol",
            );
        }
    }

    pub(super) fn capture_names(
        &self,
        names: &BTreeMap<String, String>,
    ) -> BTreeMap<String, Segment> {
        names
            .values()
            .filter_map(|id| {
                self.segments
                    .get(id)
                    .map(|value| (id.clone(), value.clone()))
            })
            .collect()
    }

    pub(super) fn for_worker(
        &self,
        names: &BTreeMap<String, String>,
        original: &BTreeMap<String, Segment>,
    ) -> Self {
        let mut segments = original.clone();
        for (id, segment) in &mut segments {
            segment.owns_unlink = false;
            if let Some(current) = self.segments.get(id) {
                if segment.name != current.name || segment.tracker != current.tracker {
                    segment.name |= current.name | UNRESOLVED;
                    segment.tracker |= current.tracker | UNRESOLVED;
                }
            }
        }
        Self {
            aliases: names
                .iter()
                .map(|(parameter, id)| (parameter.clone(), Alias::Name(id.clone())))
                .collect(),
            segments,
            ..Self::default()
        }
    }

    pub(super) fn apply_worker(&mut self, before: &Self, after: &Self) {
        for (id, initial) in &before.segments {
            let (Some(current), Some(observed)) =
                (self.segments.get_mut(id), after.segments.get(id))
            else {
                continue;
            };
            if initial.name == observed.name && initial.tracker == observed.tracker {
                continue;
            }
            if current.name == initial.name && current.tracker == initial.tracker {
                current.name = observed.name;
                current.tracker = observed.tracker;
                current.removal.clone_from(&observed.removal);
            } else {
                // An independently established removal cannot be undone.
                if current.name != CLOSED {
                    current.name |= observed.name | UNRESOLVED;
                }
                current.tracker |= observed.tracker | UNRESOLVED;
            }
        }
    }

    pub(super) fn interrupt_worker(&mut self, names: &BTreeMap<String, String>) {
        for id in names.values() {
            if let Some(segment) = self.segments.get_mut(id) {
                if segment.name != CLOSED {
                    segment.name |= UNRESOLVED;
                }
                segment.tracker |= UNRESOLVED;
            }
        }
    }
}

fn memory(env: &State) -> Option<&LocalMemory> {
    Some(&env.ownership.as_ref()?.processes.protocol.memory)
}

fn memory_mut(env: &mut State) -> &mut LocalMemory {
    &mut env
        .ownership
        .as_mut()
        .expect("shared-memory state")
        .processes
        .protocol
        .memory
}

fn name_of(expression: &AstExpr, env: &State) -> Option<String> {
    let memory = memory(env)?;
    match &expression.kind {
        AstExprKind::Name(name) => {
            let Alias::Name(id) = memory.aliases.get(name)? else {
                return None;
            };
            Some(id.clone())
        }
        AstExprKind::Attribute(receiver, member) if member == "name" => {
            let Alias::Mapping(id) = memory.aliases.get(expr_name(receiver)?)? else {
                return None;
            };
            Some(memory.mappings.get(id)?.segment.clone())
        }
        _ => None,
    }
}

pub(super) fn name_argument(expression: &AstExpr, env: &State) -> Option<String> {
    name_of(expression, env)
}

fn constructor(expression: &AstExpr, env: &State) -> bool {
    call_parts(expression).is_some_and(|(function, _, _)| {
        ownership::external_identity(function, env).as_deref()
            == Some("external:multiprocessing.shared_memory.SharedMemory")
    })
}

fn record(
    effect: &str,
    applies: Result<(), &str>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    runtime::use_model(SYMBOL, effect, MODEL, applies, file, range, result)
}

fn finding(
    message: &str,
    related: Vec<Span>,
    certainty: &str,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    report(
        result,
        "TP7003",
        message,
        span(file, range),
        related,
        certainty,
        vec![format!("{MODEL}@{}", crate::foundation::CATALOG_REVISION)],
    );
}

fn identity(file: &SourceFile, range: SourceRange, env: &State) -> String {
    format!(
        "{}:{}:{}",
        file.path,
        range.start,
        env.allocation_context.join("/")
    )
}

fn alias_binding(env: &mut State, name: &str, alias: Alias) {
    env.bind_runtime_handle(name)
        .processes
        .protocol
        .memory
        .aliases
        .insert(name.into(), alias);
}

fn positional_fields<'a>(
    args: &'a [AstExpr],
    kwargs: &'a [CallKeyword],
) -> Option<BTreeMap<&'a str, &'a AstExpr>> {
    let fields = ["name", "create", "size"];
    if args.len() > fields.len()
        || args
            .iter()
            .any(|item| matches!(item.kind, AstExprKind::Starred(_)))
    {
        return None;
    }
    let mut bound: BTreeMap<_, _> = fields.into_iter().zip(args).collect();
    for (name, value) in kwargs {
        let name = name.as_deref()?;
        if !matches!(name, "name" | "create" | "size" | "track")
            || bound.insert(name, value).is_some()
        {
            return None;
        }
    }
    Some(bound)
}

enum Construction {
    Create,
    Attach(String),
}

fn construction(
    expression: &AstExpr,
    env: &State,
    file: &SourceFile,
) -> Result<Construction, &'static str> {
    const UNSUPPORTED: &str =
        "SharedMemory requires a bounded generated allocation or exact-name attachment";
    let (_, args, kwargs) = call_parts(expression).expect("SharedMemory constructor");
    let bound = positional_fields(args, kwargs).ok_or(UNSUPPORTED)?;
    if bound
        .get("track")
        .is_some_and(|arg| !matches!(arg.kind, AstExprKind::Boolean(true)))
    {
        return Err("SharedMemory tracking requires the default or literal track=True");
    }
    let explicit_spawn = env.ownership.as_ref().is_some_and(|ownership| {
        ownership
            .processes
            .contexts
            .values()
            .any(|method| method == "spawn")
            || !ownership.processes.protocol.actor.is_empty()
    });
    if !explicit_spawn {
        return Err("SharedMemory requires a resolved explicit spawn context");
    }
    runtime::runtime_profile(env)?.spawn(&env.target_python_version, "spawn")?;
    match bound.get("create").map(|value| &value.kind) {
        Some(AstExprKind::Boolean(true))
            if bound
                .get("name")
                .is_none_or(|value| matches!(value.kind, AstExprKind::NoneLiteral))
                && bound.get("size").is_some_and(|value| {
                    protocol::nonnegative(file, value).is_some_and(|size| size > 0)
                }) =>
        {
            Ok(Construction::Create)
        }
        None | Some(AstExprKind::Boolean(false))
            if bound
                .get("size")
                .is_none_or(|value| protocol::nonnegative(file, value).is_some()) =>
        {
            bound
                .get("name")
                .and_then(|name| name_of(name, env))
                .map(Construction::Attach)
                .ok_or(UNSUPPORTED)
        }
        _ => Err(UNSUPPORTED),
    }
}

fn record_removal(
    env: &mut State,
    segment: &str,
    tracker: u8,
    file: &SourceFile,
    range: SourceRange,
) {
    let removed = memory_mut(env)
        .segments
        .get_mut(segment)
        .expect("tracked shared name");
    removed.name = CLOSED;
    removed.tracker = tracker;
    removed.removal = Some(span(file, range));
}

fn attach(
    segment: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let original = memory(env).expect("shared-memory state").segments[segment].clone();
    if original.name == CLOSED {
        let mut related = vec![original.origin];
        related.extend(original.removal);
        finding(
            "SharedMemory attachment follows removal of the required name",
            related,
            "definite",
            file,
            range,
            result,
        );
        record(
            "attachment-rejected-name-removed",
            Ok(()),
            file,
            range,
            result,
        );
        raise_named(env, range, "FileNotFoundError");
        env.alive = false;
        return false;
    }
    if original.name != OPEN {
        record(
            "attach-by-name",
            Err("availability of the shared-memory name is unresolved at attachment"),
            file,
            range,
            result,
        );
    }
    // Opening the name and mapping it are separate steps. On the qualified
    // POSIX path a later OSError enters the constructor's unlink handler.
    raise_named(env, range, "OSError");
    record_removal(env, segment, CLOSED | UNRESOLVED, file, range);
    record(
        "attachment-mapping-failure-may-unlink",
        Ok(()),
        file,
        range,
        result,
    );
    raise_named(env, range, "OSError");
    memory_mut(env).segments.insert(segment.into(), original);
    true
}

fn create_or_attach(
    name: Option<&str>,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let range = expression.range;
    let model = match construction(expression, env, file) {
        Ok(model) => model,
        Err(reason) => {
            record("construct-shared-memory", Err(reason), file, range, result);
            return false;
        }
    };
    let id = identity(file, range, env);
    let protocol = &env
        .ownership
        .as_ref()
        .expect("shared-memory state")
        .processes
        .protocol;
    if protocol.primitive_count() >= protocol::MAX_PRIMITIVES
        || protocol.memory.mappings.contains_key(&id)
        || matches!(model, Construction::Create)
            && protocol
                .memory
                .segments
                .values()
                .any(|segment| segment.name != CLOSED)
    {
        record(
            "construct-shared-memory",
            Err("shared-memory mapping or allocation budget exceeded"),
            file,
            range,
            result,
        );
        return false;
    }
    let segment = match model {
        Construction::Create => {
            raise_named(env, range, "OSError");
            memory_mut(env).segments.insert(
                id.clone(),
                Segment {
                    name: OPEN,
                    tracker: OPEN,
                    origin: protocol::allocation_origin(file, range),
                    removal: None,
                    owns_unlink: true,
                },
            );
            record(
                "create-shared-memory-track-name",
                Ok(()),
                file,
                range,
                result,
            );
            id.clone()
        }
        Construction::Attach(segment) => {
            let memory = memory(env).expect("shared-memory state");
            let creator = &memory.segments[&segment];
            let identity = super::super::protocol_guard_identity(file, &creator.origin);
            let ambiguous = memory.segments.iter().any(|(other, candidate)| {
                other != &segment
                    && super::super::protocol_guard_identity(file, &candidate.origin) == identity
            });
            if ambiguous {
                boundary(result, file, range, "normalized allocation anchors do not uniquely identify this shared-memory unlink owner for review");
            } else {
                crate::review::operations::record_protocol(
                    "shared-memory-attachment",
                    span(file, range),
                    vec![
                        format!("{MODEL}@{}", crate::foundation::CATALOG_REVISION),
                        identity,
                    ],
                );
            }
            if !attach(&segment, env, file, range, result) {
                return true;
            }
            record("attach-local-mapping", Ok(()), file, range, result);
            segment
        }
    };
    memory_mut(env).mappings.insert(
        id.clone(),
        Mapping {
            segment,
            state: OPEN,
            buffer_property: OPEN,
            base: OPEN,
            unknown_exports: false,
            exports: BTreeMap::new(),
            origin: span(file, range),
        },
    );
    if let Some(name) = name {
        alias_binding(env, name, Alias::Mapping(id));
    }
    true
}

fn view_of(expression: &AstExpr, env: &State) -> Option<(String, Option<String>)> {
    let memory = memory(env)?;
    match &expression.kind {
        AstExprKind::Name(name) => match memory.aliases.get(name)? {
            Alias::View { mapping, export } => Some((mapping.clone(), export.clone())),
            _ => None,
        },
        AstExprKind::Attribute(receiver, member) if member == "buf" => {
            let Alias::Mapping(id) = memory.aliases.get(expr_name(receiver)?)? else {
                return None;
            };
            Some((id.clone(), None))
        }
        _ => None,
    }
}

fn valid_slice(file: &SourceFile, index: &AstExpr) -> bool {
    let Some(source) = file.source.get(index.range.start..index.range.end) else {
        return false;
    };
    let bounds = source.split(':').collect::<Vec<_>>();
    (2..=3).contains(&bounds.len())
        && bounds.iter().all(|part| {
            let text = part.trim();
            text.is_empty() || text.replace('_', "").parse::<isize>().is_ok()
        })
        && bounds.get(2).is_none_or(|step| {
            step.trim().is_empty()
                || step
                    .trim()
                    .replace('_', "")
                    .parse::<isize>()
                    .ok()
                    .is_some_and(|step| step != 0)
        })
}

fn view_flags(mapping: &Mapping, export: Option<&str>) -> u8 {
    export.map_or(mapping.base, |id| mapping.exports[id])
}

fn access_view(
    view: &(String, Option<String>),
    (source, missing_property_error): (&AstExpr, &str),
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) -> bool {
    let record_view = &memory(env).expect("shared-memory state").mappings[&view.0];
    if matches!(&source.kind, AstExprKind::Attribute(_, member) if member == "buf")
        && record_view.buffer_property != OPEN
    {
        let property = record_view.buffer_property;
        if property & CLOSED != 0 {
            finding(
                "SharedMemory.buf has no base view after local cleanup",
                vec![record_view.origin.clone()],
                if property == CLOSED {
                    "definite"
                } else {
                    "possible"
                },
                file,
                range,
                result,
            );
            raise_named(env, range, missing_property_error);
        }
        if property == CLOSED {
            env.alive = false;
        } else {
            record(
                "access-buffer-view",
                Err("the shared-memory buffer property is unresolved"),
                file,
                range,
                result,
            );
        }
        return false;
    }
    let flags = view_flags(record_view, view.1.as_deref()) | (record_view.state & UNRESOLVED);
    if flags & UNRESOLVED != 0 {
        record(
            "access-buffer-view",
            Err("shared-memory buffer view lifetime is unresolved"),
            file,
            range,
            result,
        );
    }
    if flags & CLOSED != 0 {
        finding(
            "shared-memory buffer view is accessed after release",
            vec![record_view.origin.clone()],
            if flags == CLOSED {
                "definite"
            } else {
                "possible"
            },
            file,
            range,
            result,
        );
        raise_named(env, range, "ValueError");
        if flags == CLOSED {
            env.alive = false;
            return false;
        }
    }
    true
}

pub(super) fn initialize(
    name: &str,
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    if constructor(expression, env) {
        return create_or_attach(Some(name), expression, env, file, result);
    }
    if let AstExprKind::Attribute(_, member) = &expression.kind {
        if member == "name" {
            if let Some(segment) = name_of(expression, env) {
                alias_binding(env, name, Alias::Name(segment));
                record(
                    "capture-shared-memory-name",
                    Ok(()),
                    file,
                    expression.range,
                    result,
                );
                return true;
            }
        } else if let Some((mapping, export)) = view_of(expression, env) {
            let property =
                memory(env).expect("shared-memory state").mappings[&mapping].buffer_property;
            if property == CLOSED {
                // SharedMemory.buf returns None after its base buffer is released.
                env.bind_runtime_handle(name);
                record(
                    "buffer-property-after-close",
                    Ok(()),
                    file,
                    expression.range,
                    result,
                );
            } else if property != OPEN {
                record(
                    "bind-base-buffer-view",
                    Err("the shared-memory buffer property is unresolved"),
                    file,
                    expression.range,
                    result,
                );
                env.bind_runtime_handle(name);
            } else {
                alias_binding(env, name, Alias::View { mapping, export });
                record(
                    "bind-base-buffer-view",
                    Ok(()),
                    file,
                    expression.range,
                    result,
                );
            }
            return true;
        }
    }
    let AstExprKind::Subscript(base, indexes) = &expression.kind else {
        return false;
    };
    let Some(view) = view_of(base, env) else {
        return false;
    };
    let [index] = indexes.as_slice() else {
        record(
            "export-buffer-view",
            Err("shared-memory buffer indexes require a bounded local access"),
            file,
            expression.range,
            result,
        );
        return false;
    };
    if !valid_slice(file, index) {
        return false;
    }
    if !access_view(
        &view,
        (base, "TypeError"),
        env,
        file,
        expression.range,
        result,
    ) {
        return true;
    }
    let id = identity(file, expression.range, env);
    let mapping = memory_mut(env)
        .mappings
        .get_mut(&view.0)
        .expect("tracked mapping");
    if mapping.exports.len() >= MAX_EXPORTS || mapping.exports.contains_key(&id) {
        mapping.state |= UNRESOLVED;
        record(
            "export-buffer-view",
            Err("shared-memory view identity or budget exhausted"),
            file,
            expression.range,
            result,
        );
        return false;
    }
    mapping.exports.insert(id.clone(), OPEN);
    alias_binding(
        env,
        name,
        Alias::View {
            mapping: view.0,
            export: Some(id),
        },
    );
    record("export-buffer-view", Ok(()), file, expression.range, result);
    true
}

pub(super) fn indexed_view(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> bool {
    let AstExprKind::Subscript(base, indexes) = &expression.kind else {
        return false;
    };
    let Some(view) = view_of(base, env) else {
        return false;
    };
    if !matches!(indexes.as_slice(), [index] if protocol::nonnegative(file, index).is_some()) {
        memory_mut(env)
            .mappings
            .get_mut(&view.0)
            .expect("tracked mapping")
            .unknown_exports = true;
        record(
            "access-buffer-view",
            Err("shared-memory buffer access requires a bounded literal index"),
            file,
            expression.range,
            result,
        );
        return false;
    }
    if access_view(
        &view,
        (base, "TypeError"),
        env,
        file,
        expression.range,
        result,
    ) {
        record("access-buffer-view", Ok(()), file, expression.range, result);
    }
    true
}

fn close_mapping(
    id: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let mapping = memory(env).expect("shared-memory state").mappings[id].clone();
    if mapping.state == CLOSED {
        record("mapping-already-closed", Ok(()), file, range, result);
        return;
    }
    if mapping.state & UNRESOLVED != 0
        || mapping.base & UNRESOLVED != 0
        || mapping.unknown_exports
        || mapping
            .exports
            .values()
            .any(|flags| flags & UNRESOLVED != 0)
    {
        record(
            "close-local-mapping",
            Err("exported shared-memory views or the local mapping are unresolved"),
            file,
            range,
            result,
        );
        return;
    }
    let partial = memory_mut(env)
        .mappings
        .get_mut(id)
        .expect("tracked mapping");
    partial.base = CLOSED;
    partial.buffer_property = CLOSED;
    if mapping.exports.values().any(|flags| flags & OPEN != 0) {
        let definite = mapping.exports.values().any(|flags| *flags == OPEN);
        finding(
            "SharedMemory.close cannot close the mapping while an exported view is live",
            vec![mapping.origin],
            if definite { "definite" } else { "possible" },
            file,
            range,
            result,
        );
        record(
            "base-released-mapping-close-rejected",
            Ok(()),
            file,
            range,
            result,
        );
        raise_named(env, range, "BufferError");
        if definite {
            env.alive = false;
            return;
        }
    }
    let mapping = memory_mut(env)
        .mappings
        .get_mut(id)
        .expect("tracked mapping");
    mapping.state = CLOSED;
    mapping
        .exports
        .values_mut()
        .for_each(|state| *state = CLOSED);
    record("close-local-mapping", Ok(()), file, range, result);
}

fn unlink(
    id: &str,
    env: &mut State,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let memory = memory(env).expect("shared-memory state");
    let segment = memory.mappings[id].segment.clone();
    let state = memory.segments[&segment].clone();
    let pending_worker = env
        .ownership
        .as_ref()
        .expect("shared-memory state")
        .processes
        .workers
        .values()
        .any(|worker| {
            worker
                .bindings
                .shared_names
                .values()
                .any(|name| name == &segment)
        });
    if !state.owns_unlink && state.name != CLOSED {
        finding(
            "shared-memory name is unlinked by a handle without the creator's unlink duty",
            vec![state.origin.clone()],
            "definite",
            file,
            range,
            result,
        );
    }
    if state.name & (CLOSED | UNRESOLVED) != 0 || pending_worker {
        let current = memory_mut(env)
            .segments
            .get_mut(&segment)
            .expect("tracked name");
        current.name = CLOSED;
        raise_named(env, range, "FileNotFoundError");
        if state.name == CLOSED {
            env.alive = false;
            record("unlink-rejected-name-removed", Ok(()), file, range, result);
            return;
        }
        if state.name & UNRESOLVED != 0 || pending_worker {
            record(
                "unlink-order",
                Err("shared-memory unlink and attachment ordering is unresolved"),
                file,
                range,
                result,
            );
        } else {
            record(
                "unlink-may-find-name-already-removed",
                Ok(()),
                file,
                range,
                result,
            );
        }
    }
    record_removal(env, &segment, CLOSED, file, range);
    record(
        "unlink-name-request-tracker-unregistration",
        Ok(()),
        file,
        range,
        result,
    );
}

pub(super) fn call(
    expression: &AstExpr,
    env: &mut State,
    file: &SourceFile,
    result: &mut Inspection,
) -> calls::ModelCall {
    use calls::ModelCall::{Applied, Declined, Unrecognized};
    if constructor(expression, env) {
        return if create_or_attach(None, expression, env, file, result) {
            Applied
        } else {
            Declined
        };
    }
    if let Some((_, arguments, keywords)) = call_parts(expression) {
        for argument in arguments
            .iter()
            .chain(keywords.iter().map(|(_, value)| value))
        {
            if !matches!(&argument.kind, AstExprKind::Attribute(..)) {
                continue;
            }
            if let Some((mapping, _)) = view_of(argument, env) {
                memory_mut(env)
                    .mappings
                    .get_mut(&mapping)
                    .expect("tracked mapping")
                    .unknown_exports = true;
                record(
                    "buffer-view-escape",
                    Err("shared-memory base view escapes as a call argument"),
                    file,
                    argument.range,
                    result,
                );
            }
        }
    }
    let Some((receiver, method, arguments, keywords)) = called_method(expression) else {
        return Unrecognized;
    };
    let no_arguments = arguments.is_empty() && keywords.is_empty();
    let range = expression.range;
    if let Some((mapping, export)) = view_of(receiver, env) {
        if method == "release" && no_arguments {
            if matches!(&receiver.kind, AstExprKind::Attribute(..))
                && memory(env).expect("shared-memory state").mappings[&mapping].buffer_property
                    != OPEN
            {
                access_view(
                    &(mapping, export),
                    (receiver, "AttributeError"),
                    env,
                    file,
                    range,
                    result,
                );
                return Applied;
            }
            let mapping = memory_mut(env)
                .mappings
                .get_mut(&mapping)
                .expect("tracked mapping");
            match export {
                Some(export) => *mapping.exports.get_mut(&export).expect("tracked export") = CLOSED,
                None => mapping.base = CLOSED,
            }
            record("release-buffer-view", Ok(()), file, range, result);
            return Applied;
        }
        let mapping = memory_mut(env)
            .mappings
            .get_mut(&mapping)
            .expect("tracked mapping");
        mapping.unknown_exports = true;
        record(
            method,
            Err("operation is outside the bounded shared-memory view profile"),
            file,
            range,
            result,
        );
        return Declined;
    }
    let Some(Alias::Mapping(id)) = expr_name(receiver)
        .and_then(|name| memory(env)?.aliases.get(name))
        .cloned()
    else {
        return Unrecognized;
    };
    match (method, no_arguments) {
        ("close", true) => close_mapping(&id, env, file, range, result),
        ("unlink", true) => unlink(&id, env, file, range, result),
        _ => {
            record(
                method,
                Err("operation is outside the bounded shared-memory mapping profile"),
                file,
                range,
                result,
            );
            return Declined;
        }
    }
    Applied
}

pub(super) fn report_scope(
    memory: &LocalMemory,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    for mapping in memory
        .mappings
        .values()
        .filter(|mapping| mapping.state & OPEN != 0)
    {
        finding(
            "SharedMemory mapping remains locally open at scope exit",
            vec![mapping.origin.clone()],
            if mapping.state == OPEN {
                "definite"
            } else {
                "possible"
            },
            file,
            range,
            result,
        );
    }
    for segment in memory
        .segments
        .values()
        .filter(|segment| segment.owns_unlink && segment.name & OPEN != 0)
    {
        finding(
            "SharedMemory name remains linked at scope exit",
            vec![segment.origin.clone()],
            if segment.name == OPEN {
                "definite"
            } else {
                "possible"
            },
            file,
            range,
            result,
        );
    }
}

//! Persistent outcome duties attached to the ordinary flow environment.

use super::*;
use crate::frontend::AstPattern;
use std::ops::{Deref, DerefMut};

const OK: u8 = 1;
const ERR: u8 = 2;
const SOME: u8 = 4;
const NOTHING: u8 = 8;
const MAX_OUTCOMES: usize = 256;
pub(super) const DISCARD: &str = "external:codepact.discard_outcome";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutcomeContract {
    Result,
    Option,
}

impl OutcomeContract {
    pub(crate) fn capability(self) -> &'static str {
        match self {
            Self::Result => "persistent-result-v1",
            Self::Option => "persistent-option-v1",
        }
    }

    fn variants(self) -> u8 {
        match self {
            Self::Result => OK | ERR,
            Self::Option => SOME | NOTHING,
        }
    }
}

pub(crate) fn member(module: &str, name: &str) -> Option<&'static str> {
    match (module, name) {
        ("codepact" | "codepact.contracts" | "codepact.rust", "MustHandle") => {
            Some("external:codepact.MustHandle")
        }
        ("codepact" | "codepact.contracts", "MustHandleOutcome") => {
            Some("external:codepact.MustHandleOutcome")
        }
        ("codepact" | "codepact.outcomes" | "codepact.rust", "Result") => {
            Some("external:codepact.Result")
        }
        ("codepact" | "codepact.outcomes" | "codepact.rust", "Option") => {
            Some("external:codepact.Option")
        }
        ("codepact", "discard_outcome") => Some(DISCARD),
        ("codepact", "resource") => Some("external:codepact.resource"),
        ("codepact.contracts", "Resource") => Some("external:codepact.Resource"),
        ("codepact.contracts" | "codepact.rust", "Owned") => Some("external:codepact.Owned"),
        ("codepact.contracts" | "codepact.rust", "Borrowed") => Some("external:codepact.Borrowed"),
        ("codepact.contracts" | "codepact.rust", "BorrowedMut") => {
            Some("external:codepact.BorrowedMut")
        }
        _ => None,
    }
}

fn raw_identity(expression: &AstExpr, imports: &BTreeMap<String, String>) -> Option<String> {
    match &expression.kind {
        AstExprKind::Name(name) => imports.get(name).cloned(),
        AstExprKind::Attribute(base, name) => {
            Some(format!("{}.{}", raw_identity(base, imports)?, name))
        }
        _ => None,
    }
}

fn identify(expression: &AstExpr, imports: &BTreeMap<String, String>) -> Option<String> {
    let raw = raw_identity(expression, imports)?;
    if let Some((module, name)) = raw
        .strip_prefix("external:")
        .and_then(|raw| raw.rsplit_once('.'))
    {
        if let Some(identity) =
            member(module, name).or_else(|| ownership::external_member(module, name))
        {
            return Some(identity.into());
        }
    }
    Some(raw)
}

fn identity_in_scope(expression: &AstExpr, env: &State) -> Option<String> {
    let mut base = expression;
    while let AstExprKind::Attribute(parent, _) = &base.kind {
        base = parent;
    }
    if expr_name(base).is_some_and(|name| env.shadowed_functions.contains(name)) {
        return None;
    }
    identify(expression, &env.function_bindings)
}

fn refers_to(
    expression: &AstExpr,
    imports: &BTreeMap<String, String>,
    predicate: &impl Fn(&str) -> bool,
) -> bool {
    if identify(expression, imports).is_some_and(|value| predicate(&value)) {
        return true;
    }
    match &expression.kind {
        AstExprKind::Subscript(base, args) => {
            refers_to(base, imports, predicate)
                || args.iter().any(|arg| refers_to(arg, imports, predicate))
        }
        AstExprKind::Call {
            function,
            arguments,
            keywords,
        } => {
            refers_to(function, imports, predicate)
                || arguments
                    .iter()
                    .any(|arg| refers_to(arg, imports, predicate))
                || keywords
                    .iter()
                    .any(|(_, arg)| refers_to(arg, imports, predicate))
        }
        AstExprKind::Tuple(args) | AstExprKind::Collection(args) => {
            args.iter().any(|arg| refers_to(arg, imports, predicate))
        }
        AstExprKind::String(text) => crate::frontend::quoted_annotation(text, expression.range)
            .filter(|annotation| !matches!(annotation.kind, AstExprKind::String(_)))
            .is_some_and(|annotation| refers_to(&annotation, imports, predicate)),
        _ => false,
    }
}

fn is_marker(identity: &str) -> bool {
    matches!(
        identity,
        "external:codepact.MustHandle"
            | "external:codepact.MustHandleOutcome"
            | "outcome:result"
            | "outcome:option"
            | "outcome:unsupported"
    )
}

fn annotation(
    expr: &AstExpr,
    imports: &BTreeMap<String, String>,
) -> Result<Option<OutcomeContract>, ()> {
    let quoted;
    let expr = if let AstExprKind::String(text) = &expr.kind {
        quoted = crate::frontend::quoted_annotation(text, expr.range);
        let Some(inner) = &quoted else {
            return Ok(None);
        };
        inner
    } else {
        expr
    };
    match identify(expr, imports).as_deref() {
        Some("outcome:result") => return Ok(Some(OutcomeContract::Result)),
        Some("outcome:option") => return Ok(Some(OutcomeContract::Option)),
        _ => {}
    }
    if !refers_to(expr, imports, &is_marker) {
        return Ok(None);
    }
    let AstExprKind::Subscript(wrapper, arguments) = &expr.kind else {
        return Err(());
    };
    let [outer, rest @ ..] = arguments.as_slice() else {
        return Err(());
    };
    let wrapper = identify(wrapper, imports);
    if matches!(
        wrapper.as_deref(),
        Some("external:typing.Annotated" | "external:typing_extensions.Annotated")
    ) && !rest
        .iter()
        .any(|value| refers_to(value, imports, &is_marker))
    {
        return annotation(outer, imports);
    }
    let marked = match wrapper.as_deref() {
        Some("external:codepact.MustHandle") => rest.is_empty(),
        Some("external:typing.Annotated" | "external:typing_extensions.Annotated") => {
            rest.iter().any(|value| {
                matches!(&value.kind, AstExprKind::Call { function, arguments, keywords }
                    if arguments.is_empty() && keywords.is_empty()
                    && identify(function, imports).as_deref() == Some("external:codepact.MustHandleOutcome"))
            })
        }
        _ => false,
    };
    if !marked {
        return Err(());
    }
    outcome_shape(outer, imports).map(Some).ok_or(())
}

fn outcome_shape(shape: &AstExpr, imports: &BTreeMap<String, String>) -> Option<OutcomeContract> {
    match identify(shape, imports).as_deref() {
        Some("plain-outcome:result") => return Some(OutcomeContract::Result),
        Some("plain-outcome:option") => return Some(OutcomeContract::Option),
        _ => {}
    }
    let AstExprKind::Subscript(family, payloads) = &shape.kind else {
        return None;
    };
    if payloads.iter().any(|payload| {
        refers_to(payload, imports, &|identity| {
            is_marker(identity)
                || matches!(
                    identity,
                    "external:codepact.Result"
                        | "external:codepact.Option"
                        | "external:codepact.Resource"
                        | "external:codepact.Owned"
                        | "external:codepact.Borrowed"
                        | "external:codepact.BorrowedMut"
                        | "resource:declared"
                        | "plain-outcome:result"
                        | "plain-outcome:option"
                )
        })
    }) {
        return None;
    }
    match (identify(family, imports).as_deref(), payloads.len()) {
        (Some("external:codepact.Result"), 2) => Some(OutcomeContract::Result),
        (Some("external:codepact.Option"), 1) => Some(OutcomeContract::Option),
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct DeclarationOutcomes {
    pub(crate) returns: BTreeMap<usize, OutcomeContract>,
    issues: Vec<SourceRange>,
}

impl DeclarationOutcomes {
    pub(super) fn report_issues(&self, result: &mut Inspection, file: &SourceFile) {
        for range in &self.issues {
            invalid_declaration(result, file, *range);
        }
    }

    fn note_position(&mut self, value: &AstExpr, bindings: &BTreeMap<String, String>) {
        if refers_to(value, bindings, &is_marker) {
            self.issues.push(value.range);
        }
    }

    fn note_return(
        &mut self,
        value: Option<&AstExpr>,
        bindings: &BTreeMap<String, String>,
        range: SourceRange,
        supported: bool,
    ) {
        let Some(value) = value else { return };
        match annotation(value, bindings) {
            Ok(Some(contract)) if supported => {
                self.returns.insert(range.start, contract);
            }
            Ok(None) => {}
            _ => self.issues.push(value.range),
        }
    }
}

pub(crate) fn scan_declarations(
    declarations: &[Declaration],
    supplied_modules: &BTreeSet<String>,
) -> DeclarationOutcomes {
    let mut resolved = DeclarationOutcomes::default();
    let mut names = BTreeMap::new();
    for declaration in declarations {
        match declaration {
            Declaration::Import { names: imports, .. } => {
                for import in imports {
                    let root = import.name.split('.').next().unwrap_or_default();
                    let local = import.import_binding();
                    if matches!(root, "codepact" | "typing" | "typing_extensions")
                        && !supplied_modules.contains(root)
                        && !supplied_modules.contains(&import.name)
                    {
                        let path = if import.alias.is_some() {
                            &import.name
                        } else {
                            root
                        };
                        names.insert(local.into(), format!("external:{path}"));
                    } else {
                        names.remove(local);
                    }
                }
            }
            Declaration::ImportFrom {
                module,
                level: 0,
                names: imports,
                ..
            } => {
                for import in imports {
                    let local = import.member_binding();
                    if local == "*" {
                        names.clear();
                    } else if !supplied_modules.contains(module) {
                        if let Some(identity) = member(module, &import.name) {
                            names.insert(local.into(), identity.into());
                        } else if matches!(module.as_str(), "typing" | "typing_extensions") {
                            names
                                .insert(local.into(), format!("external:{module}.{}", import.name));
                        } else {
                            names.remove(local);
                        }
                    } else {
                        names.remove(local);
                    }
                }
            }
            Declaration::ImportFrom { names: imports, .. } => {
                for import in imports {
                    names.remove(import.member_binding());
                }
            }
            Declaration::Function {
                name,
                parameters,
                returns,
                decorators,
                must_use_decorator,
                range,
                is_generator,
                unsupported,
                ..
            } => {
                for value in parameters
                    .iter()
                    .filter_map(|parameter| parameter.annotation.as_ref())
                {
                    resolved.note_position(value, &names);
                }
                resolved.note_return(
                    returns.as_ref(),
                    &names,
                    *range,
                    !is_generator && !unsupported && (decorators.is_empty() || *must_use_decorator),
                );
                names.remove(name);
            }
            Declaration::Class {
                name,
                decorators,
                class_annotations,
                methods,
                ..
            } => {
                if decorators.iter().any(|decorator| {
                    matches!(&decorator.kind, AstExprKind::Call { function, .. }
                        if identify(function, &names).as_deref() == Some("external:codepact.resource"))
                }) {
                    names.insert(name.clone(), "resource:declared".into());
                } else {
                    names.remove(name);
                }
                for value in class_annotations {
                    resolved.note_position(value, &names);
                }
                for method in methods {
                    for (_, value) in &method.parameter_annotations {
                        resolved.note_position(value, &names);
                    }
                    resolved.note_return(
                        method.returns.as_ref(),
                        &names,
                        method.range,
                        !method.is_generator && (!method.decorated || method.must_use_decorator),
                    );
                }
            }
            Declaration::Assignment {
                name,
                annotation: annotation_expr,
                value,
                ..
            } => {
                if let Some(value) = annotation_expr {
                    resolved.note_position(value, &names);
                }
                if let Some(name) = name {
                    let alias = value
                        .as_ref()
                        .and_then(|value| match annotation(value, &names) {
                            Ok(Some(OutcomeContract::Result)) => Some("outcome:result".to_owned()),
                            Ok(Some(OutcomeContract::Option)) => Some("outcome:option".to_owned()),
                            Err(()) if matches!(value.kind, AstExprKind::Subscript(..)) => {
                                Some("outcome:unsupported".to_owned())
                            }
                            _ => match outcome_shape(value, &names) {
                                Some(OutcomeContract::Result) => {
                                    Some("plain-outcome:result".into())
                                }
                                Some(OutcomeContract::Option) => {
                                    Some("plain-outcome:option".into())
                                }
                                None => identify(value, &names),
                            },
                        });
                    if let Some(alias) = alias {
                        names.insert(name.clone(), alias);
                    } else {
                        names.remove(name);
                    }
                }
            }
            Declaration::TypeAlias {
                name: Some(name), ..
            } => {
                names.remove(name);
            }
            Declaration::Delete { names: deleted, .. } => {
                for name in deleted.iter().flatten() {
                    names.remove(name);
                }
            }
            Declaration::ModuleFlow { .. } | Declaration::Unsupported { .. } => names.clear(),
            _ => {}
        }
    }
    resolved
}

fn invalid_declaration(result: &mut Inspection, file: &SourceFile, range: SourceRange) {
    report(result, "TP5003", "MustHandle requires a supported callable return with an exact Result or Option and ordinary payloads",
        span(file, range), Vec::new(), "definite", vec![]);
    boundary(
        result,
        file,
        range,
        "unsupported persistent outcome declaration",
    );
}

pub(super) fn local_annotation(
    expr: &AstExpr,
    state: &State,
    result: &mut Inspection,
    file: &SourceFile,
) {
    if refers_to(expr, &state.function_bindings, &is_marker) {
        invalid_declaration(result, file, expr.range);
    }
}

pub(super) type Values = BTreeSet<String>;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) struct Carrier {
    pub(super) ids: Values,
    // A joined value can carry known duties on some paths and none on others.
    includes_untracked: bool,
}

impl Carrier {
    pub(super) fn may_be_untracked(&self) -> bool {
        self.ids.is_empty() || self.includes_untracked
    }

    pub(super) fn merge(&mut self, other: &Self) {
        let includes_untracked = self.may_be_untracked() || other.may_be_untracked();
        self.ids.extend(other.ids.iter().cloned());
        self.includes_untracked = includes_untracked && !self.ids.is_empty();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Duty {
    variant: u8,
    pending: u8,
    conditional: bool,
    uncertain_selection: bool,
    escaped: bool,
    reported: bool,
    waived: bool,
    retained_by_work: bool,
    origin: Span,
    symbol: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct OutcomeState(Option<Box<TrackedOutcomes>>);

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct TrackedOutcomes {
    value: Carrier,
    pub(super) returned: Carrier,
    aliases: BTreeMap<String, Carrier>,
    evaluated_arguments: BTreeMap<(usize, usize), Carrier>,
    duties: BTreeMap<String, Duty>,
    overflowed: bool,
}

static NO_OUTCOMES: TrackedOutcomes = TrackedOutcomes {
    value: Carrier {
        ids: Values::new(),
        includes_untracked: false,
    },
    returned: Carrier {
        ids: Values::new(),
        includes_untracked: false,
    },
    aliases: BTreeMap::new(),
    evaluated_arguments: BTreeMap::new(),
    duties: BTreeMap::new(),
    overflowed: false,
};

impl Deref for OutcomeState {
    type Target = TrackedOutcomes;

    fn deref(&self) -> &Self::Target {
        self.0.as_deref().unwrap_or(&NO_OUTCOMES)
    }
}

impl DerefMut for OutcomeState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.get_or_insert_with(Box::default)
    }
}

impl OutcomeState {
    pub(super) fn take_value(&mut self) -> Values {
        self.take_carrier().ids
    }

    pub(super) fn take_carrier(&mut self) -> Carrier {
        self.0
            .as_mut()
            .map_or_else(Carrier::default, |state| std::mem::take(&mut state.value))
    }

    pub(super) fn clear_value(&mut self) {
        if let Some(state) = &mut self.0 {
            state.value = Carrier::default();
        }
    }

    pub(super) fn set_value(&mut self, value: Carrier) {
        if value.ids.is_empty() {
            self.clear_value();
        } else {
            self.value = value;
        }
    }

    pub(super) fn set_return(&mut self, value: Carrier) {
        if value.ids.is_empty() {
            if let Some(state) = &mut self.0 {
                state.returned = Carrier::default();
            }
        } else {
            self.returned = value;
        }
    }

    pub(super) fn read(&self, name: &str) -> Values {
        self.read_carrier(name).ids
    }

    pub(super) fn read_carrier(&self, name: &str) -> Carrier {
        self.aliases.get(name).cloned().unwrap_or_default()
    }

    pub(super) fn bind(&mut self, name: &str, value: Values) -> Values {
        self.bind_carrier(
            name,
            Carrier {
                ids: value,
                includes_untracked: false,
            },
        )
    }

    pub(super) fn bind_carrier(&mut self, name: &str, value: Carrier) -> Values {
        if value.ids.is_empty() {
            self.0
                .as_mut()
                .and_then(|state| state.aliases.remove(name))
                .unwrap_or_default()
                .ids
        } else {
            self.aliases
                .insert(name.to_owned(), value)
                .unwrap_or_default()
                .ids
        }
    }

    pub(super) fn join(&self, right: &Self) -> Self {
        if self.duties.is_empty()
            && right.duties.is_empty()
            && !self.overflowed
            && !right.overflowed
        {
            return Self::default();
        }
        let mut joined = (**self).clone();
        joined.overflowed |= right.overflowed;
        joined.value.merge(&right.value);
        joined.returned.merge(&right.returned);
        for (name, carrier) in &mut joined.aliases {
            if !right.aliases.contains_key(name) {
                carrier.includes_untracked = true;
            }
        }
        for (name, values) in &right.aliases {
            joined
                .aliases
                .entry(name.clone())
                .or_default()
                .merge(values);
        }
        for (identity, duty) in &mut joined.duties {
            if let Some(other) = right.duties.get(identity) {
                duty.conditional |= other.conditional
                    || (duty.variant & other.variant & (duty.pending ^ other.pending) != 0);
                duty.variant |= other.variant;
                duty.pending |= other.pending;
                duty.escaped |= other.escaped;
                duty.uncertain_selection |= other.uncertain_selection;
                duty.waived |= other.waived;
                duty.retained_by_work |= other.retained_by_work;
                duty.reported &= other.reported;
            } else {
                duty.conditional = true;
            }
        }
        for (identity, duty) in &right.duties {
            if joined.duties.len() >= MAX_OUTCOMES && !joined.duties.contains_key(identity) {
                joined.overflowed = true;
                continue;
            }
            joined
                .duties
                .entry(identity.clone())
                .or_insert_with(|| Duty {
                    conditional: true,
                    ..duty.clone()
                });
        }
        // If a name denotes this object on only one branch, reading or writing
        // that name cannot establish an unconditional effect on the object.
        for (name, values) in &joined.aliases {
            for identity in &values.ids {
                let may_select_different = [self, right].iter().any(|side| {
                    side.duties
                        .get(identity)
                        .is_some_and(|duty| duty.variant != 0)
                        && !side
                            .aliases
                            .get(name)
                            .is_some_and(|values| values.ids.contains(identity))
                });
                if let Some(duty) = joined.duties.get_mut(identity) {
                    duty.uncertain_selection |= may_select_different;
                }
            }
        }
        Self(Some(Box::new(joined)))
    }

    pub(super) fn remember_argument(&mut self, range: SourceRange) {
        let key = (range.start, range.end);
        if self.value.ids.is_empty() {
            if let Some(state) = &mut self.0 {
                state.evaluated_arguments.remove(&key);
            }
        } else if self.evaluated_arguments.len() < MAX_OUTCOMES
            || self.evaluated_arguments.contains_key(&key)
        {
            let value = self.value.clone();
            self.evaluated_arguments.insert(key, value);
        }
    }

    pub(super) fn import_argument(&mut self, parent: &Self, name: &str, values: Carrier) {
        for identity in &values.ids {
            if let Some(duty) = parent.duties.get(identity) {
                self.duties.insert(identity.clone(), duty.clone());
            }
        }
        self.bind_carrier(name, values);
    }

    pub(super) fn apply_call(&mut self, child: &Self, copy_return: bool) {
        if self.0.is_none() && child.0.is_none() {
            return;
        }
        for (identity, duty) in &mut self.duties {
            if let Some(effect) = child.duties.get(identity) {
                *duty = effect.clone();
            }
        }
        if copy_return {
            for identity in &child.returned.ids {
                if self.duties.len() >= MAX_OUTCOMES && !self.duties.contains_key(identity) {
                    self.overflowed = true;
                    continue;
                }
                if let Some(duty) = child.duties.get(identity) {
                    self.duties.insert(identity.clone(), duty.clone());
                }
            }
            self.set_value(child.returned.clone());
        }
        self.overflowed |= child.overflowed;
    }

    fn mark_escape(&mut self, values: &Values) -> bool {
        let mut unresolved = false;
        for value in values {
            if let Some(duty) = self.duties.get_mut(value).filter(|duty| duty.pending != 0) {
                duty.escaped = true;
                unresolved = true;
            }
        }
        unresolved
    }
}

fn new_duty(
    env: &mut State,
    id: String,
    contract: OutcomeContract,
    file: &SourceFile,
    range: SourceRange,
    symbol: &str,
    result: &mut Inspection,
) {
    if env.outcomes.duties.len() >= MAX_OUTCOMES && !env.outcomes.duties.contains_key(&id) {
        env.outcomes.overflowed = true;
        boundary(
            result,
            file,
            range,
            "persistent outcome tracking budget exceeded",
        );
        return;
    }
    env.outcomes
        .duties
        .entry(id.clone())
        .or_insert_with(|| Duty {
            variant: contract.variants(),
            pending: contract.variants(),
            conditional: false,
            uncertain_selection: false,
            escaped: false,
            reported: false,
            waived: false,
            retained_by_work: false,
            origin: span(file, range),
            symbol: symbol.to_owned(),
        });
    env.outcomes.value.ids.insert(id);
}

pub(super) fn deliver_work(
    env: &mut State,
    work_id: &str,
    contract: OutcomeContract,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let identity = format!("work:{work_id}");
    new_duty(
        env,
        identity.clone(),
        contract,
        file,
        range,
        "observed work result",
        result,
    );
    if let Some(duty) = env.outcomes.duties.get_mut(&identity) {
        duty.retained_by_work = true;
    }
}

pub(super) fn retain_for_work(env: &mut State, values: &Values) {
    for identity in values {
        if let Some(duty) = env.outcomes.duties.get_mut(identity) {
            duty.retained_by_work = true;
        }
    }
}

pub(super) struct Delivery<'a> {
    pub(super) contract: OutcomeContract,
    pub(super) file: &'a SourceFile,
    pub(super) range: SourceRange,
    pub(super) symbol: &'a str,
    pub(super) identity: String,
}

pub(super) fn delivery_identity(env: &State, file: &SourceFile, range: SourceRange) -> String {
    let base = format!("{}:{}:{:?}", file.path, range.start, env.allocation_context);
    if !env.outcomes.duties.contains_key(&base) {
        return base;
    }
    // With a bounded duty map, at least one of these generations is unused.
    (1..=MAX_OUTCOMES)
        .map(|generation| format!("{base}:{generation}"))
        .find(|id| !env.outcomes.duties.contains_key(id))
        .expect("bounded outcome duty map")
}

pub(super) fn deliver_return(state: &mut State, delivery: &Delivery<'_>, result: &mut Inspection) {
    let may_return_untracked = state.outcomes.returned.may_be_untracked();
    let mut renewed_variants = 0;
    if let Some(outcomes) = &mut state.outcomes.0 {
        outcomes.returned.ids.retain(|identity| {
            let Some(duty) = outcomes.duties.get(identity) else {
                return false;
            };
            renewed_variants |= duty.variant & !duty.pending;
            duty.pending != 0
        });
    }
    if may_return_untracked || state.outcomes.returned.ids.is_empty() || renewed_variants != 0 {
        state.outcomes.clear_value();
        new_duty(
            state,
            delivery.identity.clone(),
            delivery.contract,
            delivery.file,
            delivery.range,
            delivery.symbol,
            result,
        );
        if !may_return_untracked && renewed_variants != 0 {
            if let Some(duty) = state.outcomes.duties.get_mut(&delivery.identity) {
                duty.variant &= renewed_variants;
                duty.pending &= renewed_variants;
            }
        }
        let value = state.outcomes.take_value();
        if !value.is_empty() {
            state.outcomes.returned.ids.extend(value);
            state.outcomes.returned.includes_untracked = false;
        }
    }
}

pub(super) fn argument_values(env: &State, expression: &AstExpr) -> Values {
    argument_carrier(env, expression).ids
}

pub(super) fn argument_carrier(env: &State, expression: &AstExpr) -> Carrier {
    expr_name(expression).map_or_else(
        || {
            env.outcomes
                .evaluated_arguments
                .get(&(expression.range.start, expression.range.end))
                .cloned()
                .unwrap_or_default()
        },
        |name| env.outcomes.read_carrier(name),
    )
}

pub(super) fn escaping_expression(env: &State, expression: &AstExpr) -> Values {
    ownership::expression_names(expression)
        .iter()
        .flat_map(|name| env.outcomes.read(name))
        .collect()
}

pub(super) fn escape(
    env: &mut State,
    values: &Values,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    if env.outcomes.mark_escape(values) {
        boundary(
            result,
            file,
            range,
            "unresolved helper or carrier retains a persistent outcome duty",
        );
    }
}

pub(super) fn lose(
    env: &mut State,
    values: Values,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    for identity in values {
        if env.outcomes.returned.ids.contains(&identity)
            || env
                .outcomes
                .aliases
                .values()
                .any(|live| live.ids.contains(&identity))
        {
            continue;
        }
        let Some(duty) = env.outcomes.duties.get_mut(&identity) else {
            continue;
        };
        if !duty.retained_by_work {
            report_loss(duty, file, range, result);
        }
    }
}

fn report_loss(duty: &mut Duty, file: &SourceFile, range: SourceRange, result: &mut Inspection) {
    if duty.pending == 0 || duty.escaped || duty.reported {
        return;
    }
    report(
        result,
        "TP5002",
        "persistent outcome leaves its last tracked carrier before acknowledgment",
        span(file, range),
        vec![duty.origin.clone()],
        if duty.conditional || duty.pending != duty.variant {
            "possible"
        } else {
            "definite"
        },
        vec![format!("persistent-outcome:{}", duty.symbol)],
    );
    duty.reported = true;
}

pub(super) fn report_exit(
    state: &State,
    kind: &CompletionKind,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    marked: bool,
    inherited: &Values,
) {
    if state.outcomes.overflowed {
        boundary(
            result,
            file,
            range,
            "persistent outcome tracking budget exceeded",
        );
    }
    for (id, duty) in &state.outcomes.duties {
        if inherited.contains(id) || duty.variant == 0 {
            continue;
        }
        let completion = match kind {
            CompletionKind::Fallthrough => "fallthrough",
            CompletionKind::Return(_) => "return",
            CompletionKind::Raise(_) => "raise",
            CompletionKind::Cancelled(_) => "cancelled",
            CompletionKind::Break | CompletionKind::Continue => "outcome",
        };
        crate::review::operations::record(|| crate::review::operations::CleanupSite {
            kind: "TP5002",
            origin: duty.origin.clone(),
            contract: vec![format!("persistent-outcome:{}", duty.symbol)],
            completion: completion.into(),
            closed: duty.pending == 0 && !duty.escaped && !duty.waived,
        });
        if duty.pending == 0 || duty.reported || duty.escaped {
            continue;
        }
        if matches!(kind, CompletionKind::Return(_)) && state.outcomes.returned.ids.contains(id) {
            if !marked {
                report(
                    result,
                    "TP5004",
                    "pending outcome escapes through a return without a MustHandle contract",
                    span(file, range),
                    vec![duty.origin.clone()],
                    "definite",
                    vec![format!("persistent-outcome:{}", duty.symbol)],
                );
            }
        } else {
            report_loss(&mut duty.clone(), file, range, result);
        }
    }
}

pub(super) fn tracked(state: &State) -> Values {
    state.outcomes.duties.keys().cloned().collect()
}

pub(super) fn report_scope(
    state: &State,
    flow: &FlowResult,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
    marked: bool,
    inherited: &Values,
) {
    if state.alive {
        report_exit(
            state,
            &CompletionKind::Fallthrough,
            file,
            range,
            result,
            marked,
            inherited,
        );
    }
    for exit in flow.returns.iter().chain(&flow.raises) {
        report_exit(
            &exit.state,
            &exit.kind,
            file,
            exit.range,
            result,
            marked,
            inherited,
        );
    }
}

pub(super) fn resolved_discard(function: &AstExpr, env: &State) -> bool {
    identity_in_scope(function, env).as_deref() == Some(DISCARD)
}

pub(super) fn call_contract(
    expression: &AstExpr,
    env: &State,
    functions: &FunctionMap,
) -> Option<OutcomeContract> {
    let AstExprKind::Call { function, .. } = &expression.kind else {
        return None;
    };
    let binding = if let Some(name) = expr_name(function) {
        if env.shadowed_functions.contains(name) {
            return None;
        }
        env.function_bindings.get(name)?.clone()
    } else {
        let public = providers::public_name(function, env)?;
        format!("provider:{public}")
    };
    functions.get(&binding)?.outcome_return
}

pub(super) fn discard(
    env: &mut State,
    values: &Values,
    reason: Option<&AstExpr>,
    file: &SourceFile,
    range: SourceRange,
    result: &mut Inspection,
) {
    let Some(reason) = reason.and_then(|value| match &value.kind {
        AstExprKind::String(reason) if !reason.trim().is_empty() => Some(reason.trim()),
        _ => None,
    }) else {
        report(
            result,
            "TP5003",
            "discard_outcome requires a nonempty literal reason",
            span(file, range),
            Vec::new(),
            "definite",
            vec![],
        );
        return;
    };
    let mut origins = Vec::new();
    for identity in values {
        if let Some(duty) = env.outcomes.duties.get_mut(identity) {
            if duty.pending != 0 && !duty.uncertain_selection {
                origins.push(duty.origin.clone());
                duty.pending = 0;
                duty.waived = true;
            }
        }
    }
    if !origins.is_empty() {
        let primary = span(file, range);
        let message = format!("persistent outcome intentionally discarded: {reason}");
        if !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TP5005" && diagnostic.primary == primary)
        {
            result.diagnostics.push(Diagnostic {
                code: "TP5005".into(),
                severity: "info".into(),
                certainty: "definite".into(),
                semantic_identity: vec![format!("outcome-waiver:{reason}")],
                primary,
                message,
                related: origins,
            });
        }
    }
}

fn variant(identity: &str) -> Option<u8> {
    match identity {
        "external:codepact.Ok" => Some(OK),
        "external:codepact.Err" => Some(ERR),
        "external:codepact.Some" => Some(SOME),
        "external:codepact.Nothing" => Some(NOTHING),
        _ => None,
    }
}

fn capture(pattern: &AstPattern) -> bool {
    matches!(
        pattern,
        AstPattern::Capture { .. } | AstPattern::As { pattern: None, .. }
    )
}

pub(super) fn select_pattern(pattern: &AstPattern, env: &State) -> Option<(u8, bool)> {
    match pattern {
        AstPattern::Capture { .. } | AstPattern::As { pattern: None, .. } => Some((u8::MAX, true)),
        AstPattern::As {
            pattern: Some(pattern),
            ..
        } => select_pattern(pattern, env),
        AstPattern::Or { patterns, .. } => {
            let mut mask = 0;
            for pattern in patterns {
                let (selected, wildcard) = select_pattern(pattern, env)?;
                if wildcard {
                    return None;
                }
                mask |= selected;
            }
            Some((mask, false))
        }
        AstPattern::Class {
            class,
            positional,
            keywords,
            ..
        } if positional.iter().all(capture)
            && keywords.iter().all(|(_, pattern)| capture(pattern)) =>
        {
            let kind = variant(&identity_in_scope(class, env)?)?;
            let field = if kind == ERR { "error" } else { "value" };
            let supported = match (kind, positional.as_slice(), keywords.as_slice()) {
                (_, [], []) => true,
                (NOTHING, _, _) => false,
                (_, [_], []) => true,
                (_, [], [(name, _)]) => name == field,
                _ => false,
            };
            supported.then_some((kind, false))
        }
        AstPattern::Class { .. } | AstPattern::Unsupported { .. } => None,
    }
}

pub(super) fn narrow(env: &mut State, values: &Values, mask: u8) -> Option<u8> {
    if values.is_empty()
        || values.iter().any(|identity| {
            env.outcomes
                .duties
                .get(identity)
                .is_none_or(|duty| duty.uncertain_selection)
        })
    {
        return None;
    }
    let mut variants = 0;
    for identity in values {
        if let Some(duty) = env.outcomes.duties.get_mut(identity) {
            duty.variant &= mask;
            duty.pending &= mask;
            variants |= duty.variant;
        }
    }
    Some(variants)
}

pub(super) struct WildcardEvidence {
    variant: &'static str,
    origins: BTreeSet<Span>,
}

impl WildcardEvidence {
    pub(super) fn report(self, file: &SourceFile, range: SourceRange, result: &mut Inspection) {
        let primary = span(file, range);
        let message = format!(
            "wildcard acknowledged {} because it is the only remaining outcome variant",
            self.variant
        );
        let mut related = self.origins;
        if let Some(previous) = result.diagnostics.iter_mut().find(|diagnostic| {
            diagnostic.code == "TP6003"
                && diagnostic.primary == primary
                && diagnostic.message == message
        }) {
            related.extend(std::mem::take(&mut previous.related));
            previous.related.extend(related);
        } else {
            result.diagnostics.push(Diagnostic {
                semantic_identity: vec![format!("singleton-wildcard:{}", self.variant)],
                code: "TP6003".into(),
                severity: "info".into(),
                message,
                primary,
                related: related.into_iter().collect(),
                certainty: "definite".into(),
            });
        }
    }
}

pub(super) fn acknowledge(
    env: &mut State,
    values: &Values,
    matched: u8,
    wildcard: bool,
) -> Option<WildcardEvidence> {
    let variants = values
        .iter()
        .filter_map(|id| env.outcomes.duties.get(id))
        .fold(0, |mask, duty| mask | duty.variant);
    let mut evidence = if wildcard {
        let variant = match variants {
            OK => "Ok",
            ERR => "Err",
            SOME => "Some",
            NOTHING => "Nothing",
            _ => return None,
        };
        Some(WildcardEvidence {
            variant,
            origins: BTreeSet::new(),
        })
    } else {
        None
    };
    for identity in values {
        if let Some(duty) = env
            .outcomes
            .duties
            .get_mut(identity)
            .filter(|duty| !duty.uncertain_selection)
        {
            if duty.pending & matched != 0 {
                if let Some(evidence) = &mut evidence {
                    evidence.origins.insert(duty.origin.clone());
                }
            }
            duty.pending &= !matched;
        }
    }
    evidence.filter(|item| !item.origins.is_empty())
}

pub(super) fn bind_capture(pattern: &AstPattern, carrier: &Carrier, env: &mut State) {
    let name = match pattern {
        AstPattern::Capture { name, .. } | AstPattern::As { name, .. } => name.as_deref(),
        _ => None,
    };
    if let Some(name) = name {
        env.outcomes.bind_carrier(name, carrier.clone());
    }
}

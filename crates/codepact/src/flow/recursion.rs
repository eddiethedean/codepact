use super::*;
use crate::Coverage;
use std::cell::RefCell;

const MAX_PASSES: usize = 16;

#[derive(Clone, PartialEq, Eq)]
struct Summary {
    normal: Option<Box<State>>,
    returns: Vec<FlowExit>,
    raises: Vec<FlowExit>,
}

impl Summary {
    fn bottom() -> Self {
        Self {
            normal: None,
            returns: Vec::new(),
            raises: Vec::new(),
        }
    }

    fn from_execution(normal: State, outcomes: FlowResult) -> Self {
        Self {
            normal: normal.alive.then(|| Box::new(normal)),
            returns: grouped_exits(&outcomes.returns),
            raises: grouped_exits(&outcomes.raises),
        }
    }

    fn join(&self, other: Self) -> Self {
        let normal = match (&self.normal, other.normal) {
            (Some(left), Some(right)) => Some(Box::new(merge(left, &right).0)),
            (Some(left), None) => Some(left.clone()),
            (None, right) => right,
        };
        Self {
            normal,
            returns: grouped_exits(
                &self
                    .returns
                    .iter()
                    .chain(&other.returns)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            raises: grouped_exits(
                &self
                    .raises
                    .iter()
                    .chain(&other.raises)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        }
    }

    fn instantiate(&self, entry: &mut State) -> FlowResult {
        fn project(input: &State, summary: &State) -> State {
            let mut projected = input.clone();
            let targets = summary.pending_return_targets.clone().unwrap_or_default();
            apply_call_effects(&mut projected, summary, &targets);
            projected.forbidden_borrowed_closes.extend(
                summary
                    .forbidden_borrowed_closes
                    .iter()
                    .filter(|object| input.objects.contains_key(*object))
                    .cloned(),
            );
            projected.pending_return_targets = summary.pending_return_targets.clone();
            projected.alive = summary.alive;
            projected.pending_raises.clear();
            projected
        }
        let input = entry.clone();
        if let Some(normal) = &self.normal {
            *entry = project(&input, normal);
        } else {
            entry.alive = false;
        }
        let exits = |summaries: &[FlowExit]| {
            summaries
                .iter()
                .map(|exit| FlowExit {
                    state: project(&input, &exit.state),
                    range: exit.range,
                    kind: exit.kind.clone(),
                })
                .collect()
        };
        FlowResult {
            returns: exits(&self.returns),
            raises: exits(&self.raises),
            ..FlowResult::default()
        }
    }
}

struct Frame {
    key: String,
    recursive: bool,
    approximation: Summary,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn reset() {
    FRAMES.with_borrow_mut(Vec::clear);
}

pub(super) fn execute_function(
    definition: &FunctionBody,
    env: &mut State,
    functions: &FunctionMap,
    result: &mut Inspection,
) -> FlowResult {
    if env.ownership.is_none() {
        return execute(
            &definition.statements,
            env,
            &definition.file,
            &definition.contracts,
            functions,
            result,
            0,
        );
    }
    // Input identity includes the actual instance/alias partition and lifecycle
    // facts. Approximations are shared only inside an active recursive call.
    let inputs = definition
        .parameters
        .iter()
        .map(|parameter| {
            (
                parameter,
                env.places.get(parameter),
                env.unknown_places.contains(parameter),
            )
        })
        .collect::<Vec<_>>();
    let key = format!(
        "{:?}:{:?}:{:?}:{:?}:{:?}",
        definition.binding_key, inputs, env.objects, env.borrowed, env.escaped
    );
    let recursive = FRAMES.with_borrow_mut(|frames| {
        let frame = frames.iter_mut().find(|frame| frame.key == key)?;
        frame.recursive = true;
        Some(frame.approximation.clone())
    });
    if let Some(summary) = recursive {
        return summary.instantiate(env);
    }
    FRAMES.with_borrow_mut(|frames| {
        frames.push(Frame {
            key,
            recursive: false,
            approximation: Summary::bottom(),
        })
    });
    let entry = env.clone();
    let mut last_report = None;
    let mut converged = false;
    let mut summary = Summary::bottom();
    for _ in 0..MAX_PASSES {
        let mut state = entry.clone();
        let mut report = Inspection {
            schema_version: result.schema_version,
            assurance: result.assurance.clone(),
            contracts: Vec::new(),
            diagnostics: Vec::new(),
            coverage: Coverage {
                status: "complete-for-supported-subset".into(),
                reasons: Vec::new(),
                source_bytes: 0,
                declarations: 0,
            },
        };
        let outcomes = execute(
            &definition.statements,
            &mut state,
            &definition.file,
            &definition.contracts,
            functions,
            &mut report,
            0,
        );
        let recursive =
            FRAMES.with_borrow(|frames| frames.last().expect("active recursive frame").recursive);
        if !recursive {
            FRAMES.with_borrow_mut(|frames| frames.pop());
            merge_report(result, report);
            *env = state;
            return outcomes;
        }
        let next = summary.join(Summary::from_execution(state, outcomes));
        converged = next == summary;
        summary = next;
        FRAMES.with_borrow_mut(|frames| {
            frames
                .last_mut()
                .expect("active recursive frame")
                .approximation = summary.clone()
        });
        let exhausted = FLOW_STEPS.with(Cell::get) >= FLOW_STEP_LIMIT.with(Cell::get);
        last_report = Some(report);
        if exhausted && !converged {
            break;
        }
        if converged {
            break;
        }
    }
    let frame = FRAMES.with_borrow_mut(|frames| frames.pop().expect("active recursive frame"));
    merge_report(
        result,
        last_report.expect("recursive summary attempted at least one pass"),
    );
    let mut outcomes = summary.instantiate(env);
    if !converged
        || frame.recursive
            && summary.normal.is_none()
            && summary.returns.is_empty()
            && summary.raises.is_empty()
    {
        let mut unknown = entry;
        let objects = unknown.objects.keys().cloned().collect::<Vec<_>>();
        weaken(&mut unknown, objects.into_iter());
        unknown.alive = true;
        boundary_at_span(
            result,
            span(
                &definition.file,
                definition
                    .statements
                    .first()
                    .map(statement_range)
                    .unwrap_or(SourceRange { start: 0, end: 0 }),
            ),
            if converged {
                "recursive component has no usable completion summary"
            } else {
                "recursive summary fixed-point budget exceeded; ownership effects are unknown"
            },
        );
        *env = merge(env, &unknown).0;
        outcomes.raises.push(FlowExit {
            state: unknown,
            range: definition
                .statements
                .first()
                .map(statement_range)
                .unwrap_or(SourceRange { start: 0, end: 0 }),
            kind: CompletionKind::Raise(None),
        });
    }
    outcomes
}

fn merge_report(result: &mut Inspection, report: Inspection) {
    for diagnostic in report.diagnostics {
        if !result.diagnostics.contains(&diagnostic) {
            result.diagnostics.push(diagnostic);
        }
    }
    for reason in report.coverage.reasons {
        if !result.coverage.reasons.contains(&reason) {
            result.coverage.reasons.push(reason);
        }
    }
    if report.coverage.status == "incomplete" {
        result.coverage.status = "incomplete".into();
    }
}

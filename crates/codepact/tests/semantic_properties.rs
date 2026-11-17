//! Independent bounded path oracles and reproducible session edit histories.
//!
//! The oracle below models only two locally acquired resources, aliases, and
//! mutually exclusive close operations. It does not call analyzer transfer or
//! join functions. Enumerating this small language is evidence for that scope,
//! not a claim that arbitrary Python paths have been exhaustively checked.

use codepact::{check_report, AnalysisSession, CheckResult, SourceFile};

const HEADER: &str = "from codepact import resource\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def status(self): pass\n";
const TARGETS: [&str; 3] = ["3.11", "3.12", "3.13"];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Finding {
    resource: usize,
    code: &'static str,
    certainty: &'static str,
}

/// Execute each concrete branch assignment with a separate resource store.
/// A bit in a leaf selects a handle to close; handles may alias either object.
fn reference_findings(leaves: [u8; 4], handles: [usize; 2]) -> Vec<Finding> {
    let mut closed_paths = [0; 2];
    for leaf in leaves {
        let mut closed = [false; 2];
        for (handle, resource) in handles.iter().copied().enumerate() {
            if leaf & (1 << handle) != 0 {
                closed[resource] = true;
            }
        }
        for resource in 0..2 {
            closed_paths[resource] += usize::from(closed[resource]);
        }
    }
    let mut expected = Vec::new();
    for (resource, count) in closed_paths.into_iter().enumerate() {
        if count > 0 {
            expected.push(Finding {
                resource,
                code: "TP1001",
                certainty: if count == 4 { "definite" } else { "possible" },
            });
        }
        if count < 4 {
            expected.push(Finding {
                resource,
                code: "TP1002",
                certainty: if count == 0 { "definite" } else { "possible" },
            });
        }
    }
    expected.sort();
    expected
}

fn leaf_source(mask: u8) -> String {
    let mut statements = String::new();
    for handle in 0..2 {
        if mask & (1 << handle) != 0 {
            statements.push_str(&format!("            view_{handle}.close()\n"));
        }
    }
    if statements.is_empty() {
        statements.push_str("            pass\n");
    }
    statements
}

fn branch_source(leaves: [u8; 4], handles: [usize; 2], names: [&str; 2]) -> String {
    format!(
        "{HEADER}def run(left, right):\n    {} = C()\n    {} = C()\n    view_0 = {}\n    view_1 = {}\n    if left:\n        if right:\n{}        else:\n{}    else:\n        if right:\n{}        else:\n{}    {}.status()\n    {}.status()\n",
        names[0], names[1], names[handles[0]], names[handles[1]],
        leaf_source(leaves[0]), leaf_source(leaves[1]),
        leaf_source(leaves[2]), leaf_source(leaves[3]), names[0], names[1],
    )
}

fn source_line(source: &str, statement: &str) -> usize {
    source
        .lines()
        .position(|line| line.trim() == statement)
        .unwrap_or_else(|| panic!("missing {statement:?} in:\n{source}"))
        + 1
}

fn assert_branch_oracle(
    report: &CheckResult,
    source: &str,
    names: [&str; 2],
    handles: [usize; 2],
    expected: &[Finding],
) {
    assert_eq!(
        report.coverage.status, "complete-for-supported-subset",
        "the reference language must remain fully analyzed:\n{source}\n{report:#?}"
    );
    assert!(report.coverage.reasons.is_empty(), "{report:#?}");
    let mut actual = Vec::new();
    for diagnostic in &report.diagnostics {
        assert_eq!(diagnostic.primary.file, "generated.py", "{diagnostic:#?}");
        assert_eq!(diagnostic.trust, "declared-unverified", "{diagnostic:#?}");
        assert!(diagnostic.coverage_reason.is_none(), "{diagnostic:#?}");
        let resource = match diagnostic.code.as_str() {
            "TP1001" => {
                assert!(diagnostic.completion.is_none(), "{diagnostic:#?}");
                assert!(!diagnostic.related.is_empty(), "{diagnostic:#?}");
                (0..2)
                    .find(|resource| {
                        diagnostic.primary.line
                            == source_line(source, &format!("{}.status()", names[*resource]))
                    })
                    .expect("use-after-close finding must point to the observed resource")
            }
            "TP1002" => {
                assert_eq!(diagnostic.completion.as_deref(), Some("fallthrough"));
                assert!(!diagnostic.related.is_empty(), "{diagnostic:#?}");
                assert_eq!(diagnostic.related[0].file, "generated.py");
                (0..2)
                    .find(|resource| {
                        diagnostic.related[0].line
                            == source_line(source, &format!("{} = C()", names[*resource]))
                    })
                    .expect("cleanup duty must retain its acquisition identity")
            }
            _ => panic!("unexpected finding in the reference language:\n{source}\n{diagnostic:#?}"),
        };
        let acquisition = source_line(source, &format!("{} = C()", names[resource]));
        for related in &diagnostic.related {
            assert_eq!(related.file, "generated.py", "{diagnostic:#?}");
            let statement = source.lines().nth(related.line - 1).unwrap().trim();
            let is_close = handles.iter().enumerate().any(|(handle, identity)| {
                *identity == resource && statement == format!("view_{handle}.close()")
            });
            assert!(
                is_close || (diagnostic.code == "TP1002" && related.line == acquisition),
                "related evidence belongs to another operation or resource: {diagnostic:#?}"
            );
        }
        let certainty = match diagnostic.certainty.as_str() {
            "definite" => "definite",
            "possible" => "possible",
            _ => panic!("unexpected certainty: {diagnostic:#?}"),
        };
        actual.push(Finding {
            resource,
            code: if diagnostic.code == "TP1001" {
                "TP1001"
            } else {
                "TP1002"
            },
            certainty,
        });
    }
    actual.sort();
    assert_eq!(
        actual, expected,
        "independent path oracle disagrees:\n{source}\n{report:#?}"
    );
}

#[test]
fn exhaustive_alias_branch_combinations_match_an_independent_path_oracle() {
    // Four leaves, each with four distinct close subsets: all 4^4 programs.
    for encoded in 0_u16..256 {
        let leaves = std::array::from_fn(|leaf| ((encoded >> (leaf * 2)) & 3) as u8);
        for handles in [[0, 1], [1, 0]] {
            let expected = reference_findings(leaves, handles);
            // Alternate identifiers without changing object identity or duties.
            let names = if encoded % 2 == 0 {
                ["first", "second"]
            } else {
                ["renamed_a", "renamed_b"]
            };
            let source = branch_source(leaves, handles, names);
            let files = [SourceFile::new("generated.py", "generated", &source)];
            for target in TARGETS {
                let report = check_report(&files, target);
                assert_branch_oracle(&report, &source, names, handles, &expected);
            }
        }
    }
}

#[test]
fn generated_finally_cleanup_keeps_return_and_raise_obligations_separate() {
    for target in TARGETS {
        for exit in ["return", "raise"] {
            for cleanup in [
                "pass",
                "alias.close()",
                "if flag:\n            alias.close()",
            ] {
                let terminal = if exit == "return" {
                    "return None"
                } else {
                    "raise ValueError()"
                };
                let source = format!("{HEADER}def run(flag):\n    value = C()\n    alias = value\n    try:\n        {terminal}\n    finally:\n        {cleanup}\n");
                let report = check_report(
                    &[SourceFile::new("finally.py", "finally_case", &source)],
                    target,
                );
                assert_eq!(
                    report.coverage.status, "complete-for-supported-subset",
                    "{source}\n{report:#?}"
                );
                assert!(report.coverage.reasons.is_empty(), "{report:#?}");
                if cleanup == "alias.close()" {
                    assert!(
                        report.diagnostics.is_empty(),
                        "cleanup on {exit}: {report:#?}"
                    );
                } else {
                    assert_eq!(report.diagnostics.len(), 1, "{source}\n{report:#?}");
                    let finding = &report.diagnostics[0];
                    assert_eq!(finding.code, "TP1002");
                    assert_eq!(finding.completion.as_deref(), Some(exit));
                    assert_eq!(
                        finding.certainty,
                        if cleanup == "pass" {
                            "definite"
                        } else {
                            "possible"
                        }
                    );
                    assert_eq!(finding.related.len(), if cleanup == "pass" { 1 } else { 2 });
                    assert_eq!(finding.related[0].line, source_line(&source, "value = C()"));
                    if cleanup != "pass" {
                        assert_eq!(
                            finding.related[1].line,
                            source_line(&source, "alias.close()")
                        );
                    }
                    assert_eq!(finding.trust, "declared-unverified");
                }
            }
        }
    }
}

#[test]
fn unknown_effects_preserve_the_affected_duty_and_other_resource_identity() {
    for target in TARGETS {
        for affected in ["first", "second"] {
            let other = if affected == "first" {
                "second"
            } else {
                "first"
            };
            for close_other in [false, true] {
                let cleanup = if close_other {
                    format!("    {other}.close()\n")
                } else {
                    String::new()
                };
                let source = format!("{HEADER}def run():\n    first = C()\n    second = C()\n    alias = {affected}\n    opaque(alias)\n{cleanup}");
                let report = check_report(
                    &[SourceFile::new("unknown.py", "unknown_case", &source)],
                    target,
                );
                assert_eq!(report.coverage.status, "incomplete", "{report:#?}");
                assert!(
                    report.diagnostics.iter().any(|finding| {
                        finding.code == "TP9001"
                            && finding.certainty == "unknown"
                            && finding.primary.line == source_line(&source, "opaque(alias)")
                    }),
                    "unknown calls must retain an explicit boundary: {report:#?}"
                );
                let leaks = report
                    .diagnostics
                    .iter()
                    .filter(|finding| finding.code == "TP1002")
                    .collect::<Vec<_>>();
                let mut actual_duties = Vec::new();
                for finding in leaks {
                    assert_eq!(finding.related.len(), 1, "{finding:#?}");
                    assert_eq!(finding.related[0].file, "unknown.py");
                    actual_duties.push((
                        finding.related[0].line,
                        finding.certainty.as_str(),
                        finding
                            .completion
                            .as_deref()
                            .expect("cleanup has an exit kind"),
                    ));
                    assert_eq!(finding.trust, "declared-unverified");
                }
                let mut expected_duties = Vec::new();
                // The opaque operation may return or raise. On either exit it
                // may affect only the passed resource. A later close of the
                // other object cannot discharge its earlier exceptional duty.
                for completion in ["fallthrough", "raise"] {
                    expected_duties.push((
                        source_line(&source, &format!("{affected} = C()")),
                        "possible",
                        completion,
                    ));
                    if !close_other || completion == "raise" {
                        expected_duties.push((
                            source_line(&source, &format!("{other} = C()")),
                            "definite",
                            completion,
                        ));
                    }
                }
                actual_duties.sort();
                expected_duties.sort();
                assert_eq!(
                    actual_duties, expected_duties,
                    "each resource keeps its own duty: {report:#?}"
                );
                assert_eq!(
                    report.diagnostics.len(),
                    expected_duties.len() + 1,
                    "one explicit unknown boundary is required: {report:#?}"
                );
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .all(|finding| matches!(finding.code.as_str(), "TP1002" | "TP9001")),
                    "{report:#?}"
                );
            }
        }
    }
}

fn bounded_setting(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => {
            let value = value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("{name} must be an integer"));
            assert!(
                (minimum..=maximum).contains(&value),
                "{name} must be in {minimum}..={maximum}"
            );
            value
        }
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => panic!("invalid {name}: {error}"),
    }
}

#[derive(Clone)]
struct Project {
    close: bool,
    renamed: bool,
    offset: usize,
    provider_present: bool,
    raising_close: bool,
    target: usize,
    reversed: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            close: false,
            renamed: false,
            offset: 0,
            provider_present: true,
            raising_close: false,
            target: 0,
            reversed: false,
        }
    }
}

impl Project {
    fn files(&self) -> Vec<SourceFile> {
        let name = if self.renamed { "renamed" } else { "value" };
        let close = if self.close {
            format!("    {name}.close()\n")
        } else {
            String::new()
        };
        let mut files = vec![
            SourceFile::new("main.py", "main", format!("{}from facade import C\ndef run():\n    {name} = C()\n{close}    {name}.status()\n", "\n".repeat(self.offset))),
            SourceFile::new("facade.py", "facade", "from provider import C as C\n"),
        ];
        if self.provider_present {
            files.push(SourceFile::new(
                "provider.py",
                "provider",
                if self.raising_close {
                    HEADER.replace(
                        "def close(self): pass",
                        "def close(self): raise ValueError()",
                    )
                } else {
                    HEADER.into()
                },
            ));
        }
        if self.reversed {
            files.reverse();
        }
        files
    }
}

#[test]
fn reproducible_edit_histories_match_fresh_analysis_after_every_step() {
    let histories = bounded_setting("CODEPACT_EDIT_HISTORIES", 8, 1, 256);
    let steps = bounded_setting("CODEPACT_EDIT_STEPS", 24, 10, 256);
    for seed in 0..histories {
        let mut random = (seed as u64).wrapping_add(1);
        let mut project = Project::default();
        let mut session = AnalysisSession::default();
        for step in 0..steps {
            random = random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            // The first ten steps guarantee every edit category is exercised.
            let operation = if step < 10 {
                step
            } else {
                ((random >> 32) % 10) as usize
            };
            match operation {
                0 => project.close = !project.close,
                1 => project.renamed = !project.renamed,
                2 => project.offset = (project.offset + 1) % 4,
                3 => project.raising_close = !project.raising_close,
                4 => project.provider_present = false,
                5 => project.provider_present = true,
                6 => project.target = (project.target + 1) % TARGETS.len(),
                7 => project.reversed = !project.reversed,
                8 => {
                    session.cancellation_token().cancel();
                    assert!(
                        session
                            .check_report_cancellable(&project.files(), TARGETS[project.target])
                            .is_err(),
                        "seed={seed} step={step}: cancellation published a result"
                    );
                }
                9 => session.reset(),
                _ => unreachable!(),
            }
            let files = project.files();
            let target = TARGETS[project.target];
            let reused = session.check_report(&files, target);
            let fresh = check_report(&files, target);
            assert_eq!(
                reused, fresh,
                "seed={seed} step={step} operation={operation} target={target}"
            );
            assert_eq!(
                reused,
                session.check_report(&files, target),
                "seed={seed} step={step}: no-op changed the report"
            );
            if !project.provider_present {
                assert_eq!(
                    reused.coverage.status, "incomplete",
                    "missing provider lost its boundary: {reused:#?}"
                );
                assert!(reused
                    .diagnostics
                    .iter()
                    .any(|finding| finding.code == "TP9001"));
            } else if !project.raising_close {
                assert_eq!(
                    reused.coverage.status, "complete-for-supported-subset",
                    "{reused:#?}"
                );
                assert_eq!(reused.diagnostics.len(), 1, "{reused:#?}");
                let finding = &reused.diagnostics[0];
                assert_eq!(
                    finding.code,
                    if project.close { "TP1001" } else { "TP1002" }
                );
                assert_eq!(finding.certainty, "definite");
                assert_eq!(finding.primary.file, "main.py");
            }
        }
    }
}

#[test]
fn syntax_perturbations_remain_deterministic_and_serialize() {
    let source = format!("{HEADER}# Unicode offsets: café λ\ndef run():\n    value = C()\n    value.close()\n    value.status()\n");
    // Every character boundary includes valid prefixes and malformed snippets;
    // there is no Python execution path in this source-only library test.
    for end in source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([source.len()])
    {
        let files = [SourceFile::new("partial.py", "partial", &source[..end])];
        let mut session = AnalysisSession::default();
        for target in TARGETS {
            let fresh = check_report(&files, target);
            assert_eq!(
                session.check_report(&files, target),
                fresh,
                "offset={end} target={target}"
            );
            let serialized = serde_json::to_string(&fresh).unwrap();
            let restored = serde_json::from_str::<CheckResult>(&serialized).unwrap();
            // Internal semantic identities are intentionally not public JSON.
            assert_eq!(serde_json::to_string(&restored).unwrap(), serialized);
            if fresh
                .diagnostics
                .iter()
                .any(|finding| finding.code == "TP9001")
            {
                assert_eq!(fresh.coverage.status, "incomplete", "{fresh:#?}");
            }
        }
    }
}

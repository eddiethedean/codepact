use codepact::adoption::{
    build_report, serialize_report, EffectiveConfig, ADVANCED_RULES, DEFAULT_RULES, OWNERSHIP_RULES,
};
use codepact::{AnalysisSession, Ownership, SourceFile};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

const PYTHON_VERSION: &str = "3.13";
const TASK_LIMIT: usize = 128;

fn task_source(count: usize) -> String {
    let mut source = String::from(
        "from asyncio import TaskGroup\nasync def worker(): pass\nasync def run():\n    async with TaskGroup() as group:\n",
    );
    for index in 0..count {
        source.push_str(&format!(
            "        pending_{index} = worker()\n        group.create_task(pending_{index})\n"
        ));
    }
    source
}

fn config() -> EffectiveConfig {
    EffectiveConfig {
        source: None,
        root: PathBuf::from("."),
        python_version: PYTHON_VERSION.into(),
        include: vec!["**/*.py".into()],
        exclude: Vec::new(),
        enabled_rules: DEFAULT_RULES
            .iter()
            .chain(OWNERSHIP_RULES)
            .chain(ADVANCED_RULES)
            .map(|rule| (*rule).into())
            .collect::<BTreeSet<_>>(),
        mode: "advisory".into(),
        ownership: Ownership::AdvancedAssumeDefaultTaskFactory,
        finite_matches: false,
        fail_on: "error".into(),
        baseline: None,
        dependency_roots: Vec::new(),
        required_providers: Vec::new(),
        severity: Default::default(),
        settings_origins: Default::default(),
    }
}

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn measure() {
    let at_limit_body = task_source(TASK_LIMIT);
    let over_limit_body = task_source(TASK_LIMIT + 1);
    let main = SourceFile::new("main.py", "main", at_limit_body.clone());
    let independent = SourceFile::new(
        "independent.py",
        "independent",
        "def independent(): return 1\n",
    );
    let at_limit_files = [main.clone(), independent.clone()];
    let over_limit_files = [
        SourceFile::new("main.py", "main", over_limit_body.clone()),
        independent.clone(),
    ];
    let unrelated_edit = SourceFile::new(
        "independent.py",
        "independent",
        "def independent(): return 2\n",
    );
    let unrelated_files = [main.clone(), unrelated_edit.clone()];
    let location_edit = SourceFile::new(
        "main.py",
        "main",
        format!("# source location edit\n{at_limit_body}"),
    );
    let location_files = [location_edit, unrelated_edit];

    let mut cold_ms = Vec::new();
    let mut warm_ms = Vec::new();
    let mut over_limit_edit_ms = Vec::new();
    let mut recovery_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    let mut location_edit_ms = Vec::new();
    let mut report_ms = Vec::new();
    let mut source_bytes = 0usize;
    let mut over_limit_reason_count = 0usize;

    for _ in 0..7 {
        let mut session = AnalysisSession::default();

        let start = Instant::now();
        let first = session.check_with_ownership(
            &at_limit_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        cold_ms.push(milliseconds(start));
        source_bytes = main.source.len();
        assert_eq!(first.coverage.status, "complete-for-supported-subset");
        assert!(first.diagnostics.is_empty(), "{first:#?}");

        let start = Instant::now();
        let warm = session.check_with_ownership(
            &at_limit_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        warm_ms.push(milliseconds(start));
        assert_eq!(first, warm);

        let start = Instant::now();
        let over_limit = session.check_with_ownership(
            &over_limit_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        over_limit_edit_ms.push(milliseconds(start));
        assert_eq!(over_limit.coverage.status, "incomplete", "{over_limit:#?}");
        let task_boundaries = over_limit
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "TP9001"
                    && diagnostic
                        .message
                        .contains("pending coroutine budget exceeded")
            })
            .count();
        assert_eq!(task_boundaries, 1, "{over_limit:#?}");
        over_limit_reason_count = task_boundaries;

        let start = Instant::now();
        let recovered = session.check_with_ownership(
            &at_limit_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        recovery_edit_ms.push(milliseconds(start));
        assert_eq!(first, recovered);

        let start = Instant::now();
        let unrelated = session.check_with_ownership(
            &unrelated_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        unrelated_edit_ms.push(milliseconds(start));
        assert_eq!(
            unrelated,
            codepact::check_with_ownership(
                &unrelated_files,
                PYTHON_VERSION,
                Ownership::AdvancedAssumeDefaultTaskFactory
            )
        );

        let start = Instant::now();
        let located = session.check_with_ownership(
            &location_files,
            PYTHON_VERSION,
            Ownership::AdvancedAssumeDefaultTaskFactory,
        );
        location_edit_ms.push(milliseconds(start));
        assert_eq!(located.coverage.status, "complete-for-supported-subset");
        assert!(located.diagnostics.is_empty(), "{located:#?}");

        let start = Instant::now();
        let report = build_report(located, &location_files, &config(), None, None).unwrap();
        assert!(!serialize_report(&report).unwrap().is_empty());
        report_ms.push(milliseconds(start));
    }

    println!(
        "{}",
        serde_json::json!({
            "workload": "taskgroup-coroutine-budget-recovery",
            "coroutine_limit": 128,
            "size_at_limit": TASK_LIMIT,
            "size_over_limit": TASK_LIMIT + 1,
            "python_version": PYTHON_VERSION,
            "ownership": Ownership::AdvancedAssumeDefaultTaskFactory,
            "source_bytes_at_limit": source_bytes,
            "over_limit_boundary_count": over_limit_reason_count,
            "cold_ms": cold_ms,
            "warm_ms": warm_ms,
            "over_limit_edit_ms": over_limit_edit_ms,
            "recovery_edit_ms": recovery_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms,
            "location_edit_ms": location_edit_ms,
            "report_ms": report_ms
        })
    );
}

fn main() {
    measure();
}

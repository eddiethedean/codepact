use codepact::adoption::{
    build_report, serialize_report, EffectiveConfig, DEFAULT_RULES, OWNERSHIP_RULES,
};
use codepact::{AnalysisSession, Ownership, SourceFile};
use std::path::PathBuf;
use std::time::Instant;

const HEADER: &str = "from codepact import move, resource\nfrom codepact.rust import Owned, Borrowed\n@resource(closed_by='close')\nclass C:\n    def read(self): pass\n    def close(self): pass\n";

fn source(workload: &str, size: usize) -> String {
    let mut source = String::from(HEADER);
    match workload {
        "legacy" => {
            source.push_str("def use(value: Borrowed[C]):\n    value.read()\n");
            for index in 0..size {
                source.push_str(&format!(
                    "def run_{index}():\n    value = C()\n    use(value)\n    value.close()\n"
                ));
            }
        }
        "alias" => {
            source.push_str("def run():\n    value = C()\n");
            for index in 0..size {
                source.push_str(&format!("    alias_{index} = value\n"));
            }
            for index in 0..size {
                source.push_str(&format!("    alias_{index} = None\n"));
            }
            source.push_str("    receiver = move(value)\n    receiver.close()\n");
        }
        "recursive" => {
            source.push_str("def take(value: Owned[C], again):\n    if again:\n        take(move(value), False)\n    else:\n        value.close()\n");
            for index in 0..size {
                source.push_str(&format!(
                    "def run_{index}():\n    value = C()\n    take(move(value), True)\n"
                ));
            }
        }
        "tasks" => {
            source.push_str("from asyncio import TaskGroup\nasync def use(value: Borrowed[C]):\n    value.read()\nasync def run():\n    value = C()\n    try:\n        async with TaskGroup() as outer:\n            outer.create_task(use(value))\n            async with TaskGroup() as inner:\n");
            for _ in 0..size {
                source.push_str("                inner.create_task(use(value))\n");
            }
            source.push_str("    finally:\n        value.close()\n");
        }
        _ => panic!("workload must be legacy, alias, recursive or tasks"),
    }
    source
}

fn measure(workload: &str, size: usize, mode: Ownership) {
    let body = source(workload, size);
    let files = [SourceFile::new("main.py", "main", body.clone())];
    let edited = [SourceFile::new("main.py", "main", format!("\n{body}"))];
    let config = EffectiveConfig {
        source: None,
        root: PathBuf::from("."),
        python_version: "3.11".into(),
        include: vec!["**/*.py".into()],
        exclude: Vec::new(),
        enabled_rules: DEFAULT_RULES
            .iter()
            .chain(OWNERSHIP_RULES.iter().filter(|_| mode.is_enabled()))
            .map(|rule| (*rule).into())
            .collect(),
        mode: "advisory".into(),
        ownership: mode,
        fail_on: "error".into(),
        baseline: None,
        severity: Default::default(),
        settings_origins: Default::default(),
    };
    let mut cold_ms = Vec::new();
    let mut warm_ms = Vec::new();
    let mut edit_ms = Vec::new();
    let mut report_ms = Vec::new();
    let milliseconds = |start: Instant| start.elapsed().as_secs_f64() * 1_000.0;
    for _ in 0..7 {
        let mut session = AnalysisSession::default();
        let start = Instant::now();
        let cold = session.check_with_ownership(&files, "3.11", mode);
        cold_ms.push(milliseconds(start));
        assert!(
            cold.diagnostics.is_empty(),
            "{workload} {mode:?}: {:?}",
            cold.diagnostics.first()
        );
        let start = Instant::now();
        let warm = session.check_with_ownership(&files, "3.11", mode);
        warm_ms.push(milliseconds(start));
        assert_eq!(cold, warm);
        let start = Instant::now();
        let result = session.check_with_ownership(&edited, "3.11", mode);
        edit_ms.push(milliseconds(start));
        assert!(
            result.diagnostics.is_empty(),
            "{workload} {mode:?}: {:?}",
            result.diagnostics.first()
        );
        let start = Instant::now();
        let report = build_report(result, &edited, &config, None, None).unwrap();
        assert!(!serialize_report(&report).unwrap().is_empty());
        report_ms.push(milliseconds(start));
    }
    println!(
        "{}",
        serde_json::json!({"workload":workload,"size":size,"mode":mode,"source_bytes":files[0].source.len(),"cold_ms":cold_ms,"warm_ms":warm_ms,"edit_ms":edit_ms,"report_ms":report_ms})
    );
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let workload = arguments.next().unwrap_or_else(|| "legacy".into());
    let size = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(32);
    if workload == "legacy" {
        measure(&workload, size, Ownership::Disabled);
    }
    let mode = if workload == "tasks" {
        Ownership::AssumeDefaultTaskFactory
    } else {
        Ownership::Enabled
    };
    measure(&workload, size, mode);
}

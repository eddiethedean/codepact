use codepact::adoption::{
    build_report, serialize_report, EffectiveConfig, ADVANCED_RULES, DEFAULT_RULES, OWNERSHIP_RULES,
};
use codepact::{AnalysisSession, Ownership, SourceFile};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

const PYTHON_VERSION: &str = "3.13";

fn source(workload: &str, size: usize) -> String {
    match workload {
        "state" => {
            let mut source = String::from(
                "from typing import Literal\nfrom codepact import state_machine, transition\nfrom codepact.rust import State\n@state_machine(states=('ready', 'active', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='active')\n    def start(self): pass\n    def send(self: State['Channel', Literal['active']]): pass\n    @transition(from_state='active', to_state='closed')\n    def close(self): pass\n",
            );
            for index in 0..size {
                source.push_str(&format!(
                    "def run_{index}():\n    channel = Channel()\n    channel.start()\n    channel.send()\n    channel.close()\n"
                ));
            }
            source
        }
        "exclusive" => {
            let mut source = String::from(
                "from codepact import resource\nfrom codepact.rust import BorrowedMut\n@resource(closed_by='close')\nclass C:\n    def write(self): pass\n    def close(self): pass\ndef edit(value: BorrowedMut[C]):\n    value.write()\n",
            );
            for index in 0..size {
                source.push_str(&format!(
                    "def run_{index}():\n    value = C()\n    edit(value)\n    value.close()\n"
                ));
            }
            source
        }
        "executor" => {
            let mut source = String::from(
                "from concurrent.futures import ThreadPoolExecutor\ndef worker(): return 1\ndef run():\n    pool = ThreadPoolExecutor()\n",
            );
            for _ in 0..size {
                source.push_str("    pool.submit(worker)\n");
            }
            source.push_str("    pool.shutdown()\n");
            source
        }
        _ => panic!("workload must be state, exclusive or executor"),
    }
}

fn affected_source(workload: &str, source: &str) -> String {
    let (old, new) = match workload {
        "state" => (
            "def send(self: State['Channel', Literal['active']]): pass",
            "def send(self: State['Channel', Literal['active']]): return None",
        ),
        "exclusive" => ("def write(self): pass", "def write(self): return None"),
        "executor" => ("def worker(): return 1", "def worker(): return 2"),
        _ => unreachable!(),
    };
    assert!(
        source.contains(old),
        "affected edit anchor exists for {workload}"
    );
    source.replacen(old, new, 1)
}

fn measure(workload: &str, size: usize) {
    let body = source(workload, size);
    let main = SourceFile::new("main.py", "main", body.clone());
    let independent = SourceFile::new(
        "independent.py",
        "independent",
        "def independent(): return 1\n",
    );
    let files = [main.clone(), independent.clone()];
    let affected_main = SourceFile::new("main.py", "main", affected_source(workload, &body));
    let affected_files = [affected_main.clone(), independent.clone()];
    let unrelated_edit = SourceFile::new(
        "independent.py",
        "independent",
        "def independent(): return 2\n",
    );
    let unrelated_files = [affected_main.clone(), unrelated_edit.clone()];
    let located_main = SourceFile::new(
        "main.py",
        "main",
        format!("# location-only edit\n{}", affected_main.source),
    );
    let located_files = [located_main, unrelated_edit];
    let config = EffectiveConfig {
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
        ownership: Ownership::Advanced,
        finite_matches: false,
        fail_on: "error".into(),
        baseline: None,
        dependency_roots: Vec::new(),
        required_providers: Vec::new(),
        runtime_profile: None,
        experimental_foundations: false,
        project_contracts: Vec::new(),
        async_guards: Vec::new(),
        uv_lock: None,
        severity: Default::default(),
        settings_origins: Default::default(),
        declaration_policy: Vec::new(),
    };
    let mut cold_ms = Vec::new();
    let mut warm_ms = Vec::new();
    let mut affected_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    let mut location_edit_ms = Vec::new();
    let mut report_ms = Vec::new();
    let milliseconds = |start: Instant| start.elapsed().as_secs_f64() * 1_000.0;
    for _ in 0..7 {
        let mut session = AnalysisSession::default();
        let start = Instant::now();
        let cold = session.check_with_ownership(&files, PYTHON_VERSION, Ownership::Advanced);
        cold_ms.push(milliseconds(start));
        assert!(
            cold.diagnostics.is_empty(),
            "{workload}: {:?}",
            cold.diagnostics.first()
        );
        assert_eq!(cold.coverage.status, "complete-for-supported-subset");

        let start = Instant::now();
        let warm = session.check_with_ownership(&files, PYTHON_VERSION, Ownership::Advanced);
        warm_ms.push(milliseconds(start));
        assert_eq!(cold, warm);

        let start = Instant::now();
        let updated =
            session.check_with_ownership(&affected_files, PYTHON_VERSION, Ownership::Advanced);
        affected_edit_ms.push(milliseconds(start));
        assert!(
            updated.diagnostics.is_empty(),
            "{workload}: {:?}",
            updated.diagnostics.first()
        );
        assert_eq!(updated.coverage.status, "complete-for-supported-subset");
        assert_eq!(
            updated,
            codepact::check_with_ownership(&affected_files, PYTHON_VERSION, Ownership::Advanced)
        );

        let start = Instant::now();
        let unrelated =
            session.check_with_ownership(&unrelated_files, PYTHON_VERSION, Ownership::Advanced);
        unrelated_edit_ms.push(milliseconds(start));
        assert!(unrelated.diagnostics.is_empty());
        assert_eq!(unrelated.coverage.status, "complete-for-supported-subset");
        assert_eq!(
            unrelated,
            codepact::check_with_ownership(&unrelated_files, PYTHON_VERSION, Ownership::Advanced)
        );

        let start = Instant::now();
        let located =
            session.check_with_ownership(&located_files, PYTHON_VERSION, Ownership::Advanced);
        location_edit_ms.push(milliseconds(start));
        assert!(
            located.diagnostics.is_empty(),
            "{workload}: {:?}",
            located.diagnostics.first()
        );
        assert_eq!(located.coverage.status, "complete-for-supported-subset");
        assert_eq!(
            located,
            codepact::check_with_ownership(&located_files, PYTHON_VERSION, Ownership::Advanced)
        );

        let start = Instant::now();
        let report = build_report(located, &located_files, &config, None, None).unwrap();
        report_ms.push(milliseconds(start));
        assert!(!serialize_report(&report).unwrap().is_empty());
    }
    println!(
        "{}",
        serde_json::json!({
            "workload": workload,
            "size": size,
            "ownership": Ownership::Advanced,
            "python_version": PYTHON_VERSION,
            "source_bytes": files[0].source.len(),
            "cold_ms": cold_ms,
            "warm_ms": warm_ms,
            "affected_edit_ms": affected_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms,
            "location_edit_ms": location_edit_ms,
            "report_ms": report_ms,
        })
    );
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let workload = arguments.next().unwrap_or_else(|| "state".into());
    let size = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(128);
    measure(&workload, size);
}

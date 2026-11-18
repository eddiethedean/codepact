use codepact::{
    check_with_foundations,
    foundation::{export_manifest, FoundationInputs, ProviderPackage},
    AnalysisSession, CheckOptions, SourceFile,
};
use std::{fmt::Write, time::Instant};

const TARGET: &str = "3.13";
const REPEATS: usize = 7;

fn inputs(marked: bool) -> FoundationInputs {
    let mut provider = ProviderPackage {
        distribution: "benchmark-provider".into(),
        version: if marked { "1.0.0" } else { "2.0.0" }.into(),
        import_scope: "provider".into(),
        marked: true,
        manifest: None,
        sources: vec![SourceFile::new(
            "provider/__init__.py",
            "provider",
            if marked {
                "from codepact import must_use\n@must_use\ndef operation(): return 1\n"
            } else {
                "def operation(): return 2\n"
            },
        )],
    };
    provider.manifest =
        Some(serde_json::to_string(&export_manifest(&provider, TARGET).unwrap()).unwrap());
    FoundationInputs {
        providers: vec![provider],
        ..FoundationInputs::default()
    }
}

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    assert_eq!(arguments.next().as_deref(), Some("manifest"));
    let count = arguments.next().unwrap().parse::<usize>().unwrap();
    let mut caller = String::from("from provider import operation\n");
    for index in 0..count {
        writeln!(caller, "def check_{index}():\n    operation()").unwrap();
    }
    let files = [
        SourceFile::new("caller.py", "caller", &caller),
        SourceFile::new(
            "independent.py",
            "independent",
            "def independent(): return 1\n",
        ),
    ];
    let mut unrelated = files.clone();
    unrelated[1].source = "def independent(): return 2\n".into();
    let installed = inputs(true);
    let upgraded = inputs(false);
    let options = CheckOptions::default();
    let mut cold_ms = Vec::new();
    let mut unchanged_ms = Vec::new();
    let mut affected_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    for _ in 0..REPEATS {
        let mut session = AnalysisSession::default();
        let start = Instant::now();
        let cold =
            session.check_with_foundations(&files, TARGET, options.clone(), installed.clone());
        cold_ms.push(milliseconds(start));
        assert_eq!(
            cold.diagnostics
                .iter()
                .filter(|d| d.code == "TP3001")
                .count(),
            count
        );

        let start = Instant::now();
        let unchanged =
            session.check_with_foundations(&files, TARGET, options.clone(), installed.clone());
        unchanged_ms.push(milliseconds(start));
        assert_eq!(cold, unchanged);

        let start = Instant::now();
        let affected =
            session.check_with_foundations(&files, TARGET, options.clone(), upgraded.clone());
        affected_edit_ms.push(milliseconds(start));
        assert!(affected.diagnostics.iter().all(|d| d.code != "TP3001"));
        assert_eq!(
            affected,
            check_with_foundations(&files, TARGET, options.clone(), upgraded.clone())
        );

        let start = Instant::now();
        let independent =
            session.check_with_foundations(&unrelated, TARGET, options.clone(), upgraded.clone());
        unrelated_edit_ms.push(milliseconds(start));
        assert_eq!(
            independent,
            check_with_foundations(&unrelated, TARGET, options.clone(), upgraded.clone())
        );
    }
    println!(
        "{}",
        serde_json::json!({
            "workload": "manifest",
            "caller_functions": count,
            "cold_ms": cold_ms,
            "unchanged_ms": unchanged_ms,
            "affected_edit_ms": affected_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms,
        })
    );
}

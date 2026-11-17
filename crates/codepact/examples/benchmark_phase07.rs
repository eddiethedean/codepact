use codepact::{AnalysisSession, CheckOptions, ProviderSource, SourceFile};
use std::time::Instant;

const PYTHON_VERSION: &str = "3.11";
const REPEATS: usize = 7;

struct Input {
    files: Vec<SourceFile>,
    options: CheckOptions,
    discovered_files: usize,
    source_bytes: usize,
}

fn input(workload: &str, size: usize, changed: bool, unrelated_edit: bool) -> Input {
    let unrelated = SourceFile::new(
        "independent.py",
        "independent",
        if unrelated_edit {
            "def independent(): return 2\n"
        } else {
            "def independent(): return 1\n"
        },
    );
    let mut options = CheckOptions::default();
    let mut files = Vec::new();
    let mut provider_source_bytes = 0;
    let discovered_files = match workload {
        "matches" => {
            options.experimental_finite_matches = true;
            let mut main = String::from("from codepact import Err, Ok, Result\n");
            for index in 0..size {
                main.push_str(&format!(
                    "def inspect_{index}(value: Result[int, str], flag: bool):\n    match value:\n        case Ok(_) if flag: pass\n"
                ));
                if !changed {
                    main.push_str("        case Ok(_): pass\n");
                }
                main.push_str("        case Err(_): pass\n");
            }
            files.push(SourceFile::new("main.py", "main", main));
            files.push(unrelated);
            files.len()
        }
        "provider" => {
            let mut provider = String::from("from codepact import must_use\n");
            let mut main = String::new();
            for index in 0..size {
                let declaration = if changed {
                    format!("def operation_{index}() -> int: return {index}\n")
                } else {
                    format!("@must_use\ndef operation_{index}() -> int: return {index}\n")
                };
                provider.push_str(&declaration);
                main.push_str(&format!(
                    "from service import operation_{index}\noperation_{index}()\n"
                ));
            }
            provider_source_bytes = provider.len();
            files.push(SourceFile::new("main.py", "main", main));
            files.push(unrelated);
            options.provider_sources.push(
                ProviderSource::new(
                    "benchmark-provider",
                    "1.0.0",
                    SourceFile::new("service.py", "service", provider),
                )
                .expect("benchmark provider identity is safe"),
            );
            files.len() + options.provider_sources.len()
        }
        _ => panic!("workload must be matches or provider"),
    };
    let source_bytes =
        files.iter().map(|file| file.source.len()).sum::<usize>() + provider_source_bytes;
    Input {
        files,
        options,
        discovered_files,
        source_bytes,
    }
}

fn count(result: &codepact::CheckResult, code: &str) -> usize {
    result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

fn measure(workload: &str, size: usize) {
    let cold_input = input(workload, size, false, false);
    let affected_input = input(workload, size, true, false);
    let unrelated_input = input(workload, size, true, true);
    let mut cold_ms = Vec::new();
    let mut unchanged_ms = Vec::new();
    let mut affected_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    let mut cold_diagnostics = None;
    let mut affected_diagnostics = None;
    let mut coverage = None;

    for _ in 0..REPEATS {
        let mut cold_session = AnalysisSession::default();
        let start = Instant::now();
        let cold = cold_session.check_with_options(
            &cold_input.files,
            PYTHON_VERSION,
            cold_input.options.clone(),
        );
        cold_ms.push(milliseconds(start));
        cold_diagnostics = Some(cold.diagnostics.len());
        coverage = Some(cold.coverage.status.clone());

        let start = Instant::now();
        let unchanged = cold_session.check_with_options(
            &cold_input.files,
            PYTHON_VERSION,
            cold_input.options.clone(),
        );
        unchanged_ms.push(milliseconds(start));
        assert_eq!(cold, unchanged, "unchanged result must be stable");

        let start = Instant::now();
        let affected = cold_session.check_with_options(
            &affected_input.files,
            PYTHON_VERSION,
            affected_input.options.clone(),
        );
        affected_edit_ms.push(milliseconds(start));
        assert_eq!(
            affected,
            codepact::check_with_options(
                &affected_input.files,
                PYTHON_VERSION,
                affected_input.options.clone()
            ),
            "affected edit must match a fresh run"
        );
        affected_diagnostics = Some(affected.diagnostics.len());

        let start = Instant::now();
        let unrelated = cold_session.check_with_options(
            &unrelated_input.files,
            PYTHON_VERSION,
            unrelated_input.options.clone(),
        );
        unrelated_edit_ms.push(milliseconds(start));
        assert_eq!(
            unrelated,
            codepact::check_with_options(
                &unrelated_input.files,
                PYTHON_VERSION,
                unrelated_input.options.clone()
            ),
            "unrelated edit must match a fresh run"
        );

        match workload {
            "matches" => {
                assert_eq!(count(&cold, "TP5001"), 0);
                assert_eq!(count(&affected, "TP5001"), size);
            }
            "provider" => {
                assert_eq!(count(&cold, "TP3001"), size);
                assert_eq!(count(&affected, "TP3001"), 0);
            }
            _ => unreachable!(),
        }
    }

    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1,
            "workload": workload,
            "size": size,
            "target_python": PYTHON_VERSION,
            "source_execution": false,
            "experimental_finite_matches": workload == "matches",
            "provider_source_count": cold_input.options.provider_sources.len(),
            "discovered_input_files": cold_input.discovered_files,
            "source_bytes": cold_input.source_bytes,
            "selected_scope": if workload == "matches" { "Result/Option finite matches" } else { "marked installed provider summaries" },
            "disabled_scope": if workload == "matches" { "lifecycle and must-use" } else { "finite matches" },
            "repeats": REPEATS,
            "cold_ms": cold_ms,
            "unchanged_session_ms": unchanged_ms,
            "affected_edit_ms": affected_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms,
            "median_ms": {
                "cold": median(&cold_ms),
                "unchanged": median(&unchanged_ms),
                "affected_edit": median(&affected_edit_ms),
                "unrelated_edit": median(&unrelated_edit_ms)
            },
            "diagnostics": {
                "cold": cold_diagnostics,
                "affected_edit": affected_diagnostics
            },
            "coverage": coverage
        })
    );
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let workload = arguments.next().unwrap_or_else(|| "matches".into());
    let size = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(64)
        .clamp(1, 512);
    measure(&workload, size);
}

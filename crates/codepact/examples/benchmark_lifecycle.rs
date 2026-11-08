use codepact::{AnalysisSession, SourceFile};
use std::process::Command;
use std::time::{Duration, Instant};

const REPEATS: usize = 7;

fn source_files(objects: usize, provider: &str) -> Vec<SourceFile> {
    let mut client = String::from("from provider import Connection, shutdown\n");
    for index in 0..objects {
        client.push_str(&format!(
            "connection_{index} = Connection()\nshutdown(connection_{index})\nconnection_{index}.status()\n"
        ));
    }
    vec![
        SourceFile::new("provider.py", "provider", provider),
        SourceFile::new("client.py", "client", client),
    ]
}

fn provider(effect: &str, prefix_lines: usize, unused_value: Option<usize>) -> String {
    let mut source = String::from(
        "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def status(self): return 'ready'\n",
    );
    for _ in 0..prefix_lines {
        source.push('\n');
    }
    source.push_str("def shutdown(value):\n");
    source.push_str(effect);
    if let Some(value) = unused_value {
        source.push_str(&format!("\ndef unused():\n    return {value}\n"));
    }
    source
}

fn median_duration(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn current_rss_kib() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

fn main() {
    let objects = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(250)
        .clamp(1, 1_000);
    let close_effect = "    value.close()\n";
    let initial = source_files(objects, &provider(close_effect, 0, None));

    if std::env::var_os("CODEPACT_RETAINED_MEMORY_PROBE").is_some() {
        let Some(baseline_rss) = current_rss_kib() else {
            eprintln!("Retained-memory probe requires `ps` and a Unix-like host.");
            return;
        };
        let mut session = AnalysisSession::default();
        let result = session.check(&initial, "3.11");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1001")
                .count(),
            objects
        );
        drop(result);
        std::thread::sleep(Duration::from_secs(2));
        let Some(retained_rss) = current_rss_kib() else {
            eprintln!("Could not sample retained RSS with `ps`.");
            return;
        };
        println!("Objects: {objects}; diagnostics dropped; analysis session retained");
        println!("Baseline RSS: {baseline_rss} KiB");
        println!("Retained-session RSS: {retained_rss} KiB");
        println!(
            "Incremental RSS: {} KiB",
            retained_rss.saturating_sub(baseline_rss)
        );
        return;
    }

    let cold_samples = (0..REPEATS)
        .map(|_| {
            let mut cold_session = AnalysisSession::default();
            let start = Instant::now();
            let result = cold_session.check(&initial, "3.11");
            let elapsed = start.elapsed();
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == "TP1001")
                    .count(),
                objects
            );
            elapsed
        })
        .collect::<Vec<_>>();

    let mut session = AnalysisSession::default();
    let cold = session.check(&initial, "3.11");
    assert_eq!(
        cold.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP1001")
            .count(),
        objects
    );

    let warm_samples = (0..REPEATS)
        .map(|_| {
            let start = Instant::now();
            let result = session.check(&initial, "3.11");
            assert_eq!(result, cold);
            start.elapsed()
        })
        .collect::<Vec<_>>();

    let body_samples = (0..REPEATS)
        .map(|index| {
            let files = source_files(objects, &provider(close_effect, 0, Some(index)));
            let start = Instant::now();
            let result = session.check(&files, "3.11");
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == "TP1001")
                    .count(),
                objects
            );
            start.elapsed()
        })
        .collect::<Vec<_>>();

    let effect_samples = (0..REPEATS)
        .map(|index| {
            let effect = format!("    if condition_{index}:\n        value.close()\n");
            let files = source_files(objects, &provider(&effect, 0, None));
            let start = Instant::now();
            let result = session.check(&files, "3.11");
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == "TP1001")
                    .count(),
                objects
            );
            assert!(result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TP1001")
                .all(|diagnostic| diagnostic.certainty == "possible"));
            start.elapsed()
        })
        .collect::<Vec<_>>();

    let location_samples = (0..REPEATS)
        .map(|index| {
            let files = source_files(objects, &provider(close_effect, index + 1, None));
            let start = Instant::now();
            let result = session.check(&files, "3.11");
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == "TP1001")
                    .count(),
                objects
            );
            start.elapsed()
        })
        .collect::<Vec<_>>();

    println!("Objects: {objects}; repeats per incremental scenario: {REPEATS}");
    println!(
        "Source bytes: {}",
        initial.iter().map(|file| file.source.len()).sum::<usize>()
    );
    println!("\n| Scenario | Median ms | Min ms | Max ms |");
    println!("| --- | ---: | ---: | ---: |");
    let cold_min = cold_samples
        .iter()
        .map(|sample| milliseconds(*sample))
        .reduce(f64::min)
        .unwrap_or_default();
    let cold_max = cold_samples
        .iter()
        .map(|sample| milliseconds(*sample))
        .reduce(f64::max)
        .unwrap_or_default();
    println!(
        "| Cold in-memory session check | {:.3} | {:.3} | {:.3} |",
        milliseconds(median_duration(cold_samples)),
        cold_min,
        cold_max
    );
    for (name, samples) in [
        ("Warm no-op session check", warm_samples),
        ("Unrelated body edit", body_samples),
        ("Imported effect edit", effect_samples),
        ("Location-only edit", location_samples),
    ] {
        let values = samples.iter().map(|sample| milliseconds(*sample));
        let minimum = values.clone().reduce(f64::min).unwrap_or_default();
        let maximum = values.clone().reduce(f64::max).unwrap_or_default();
        println!(
            "| {name} | {:.3} | {:.3} | {:.3} |",
            milliseconds(median_duration(samples)),
            minimum,
            maximum
        );
    }
    println!(
        "\nRust: {}; {}; target 3.11",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

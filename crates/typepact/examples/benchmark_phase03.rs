use std::time::{Duration, Instant};
use typepact::{AnalysisSession, Inspection, SourceFile};

const REPEATS: usize = 7;
const MAX_RESOURCES: usize = 512;

fn provider() -> SourceFile {
    SourceFile::new(
        "provider.py",
        "provider",
        r#"from typepact import resource
@resource(closed_by='close')
class Connection:
    def close(self): pass
@resource(closed_by='aclose')
class AsyncConnection:
    async def aclose(self): pass
"#,
    )
}

fn sync_client(resources: usize) -> SourceFile {
    let mut source =
        String::from("from contextlib import closing\nfrom provider import Connection\n");
    for index in 0..resources {
        source.push_str(&format!(
            "def cleanup_{index}(flag):\n    first = Connection()\n    second = Connection()\n    try:\n        while flag:\n            raise RuntimeError()\n    except RuntimeError:\n        pass\n    else:\n        pass\n    finally:\n        with closing(first), closing(second):\n            pass\n"
        ));
    }
    SourceFile::new("sync_client.py", "sync_client", source)
}

fn async_client(resources: usize) -> SourceFile {
    let mut source = String::from("from provider import AsyncConnection\n");
    for index in 0..resources {
        source.push_str(&format!(
            "async def cleanup_{index}():\n    resource = AsyncConnection()\n    await resource.aclose()\n"
        ));
    }
    SourceFile::new("async_client.py", "async_client", source)
}

fn median_duration(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn verify_sync(result: &Inspection) {
    assert!(
        result.diagnostics.is_empty(),
        "sync cleanup workload should be clean: {result:#?}"
    );
    assert_eq!(result.coverage.status, "complete-for-supported-subset");
}

fn verify_async(result: &Inspection) {
    assert!(
        result.diagnostics.iter().all(|diagnostic| {
            diagnostic.code != "TP1002" || diagnostic.certainty != "definite"
        }),
        "cancellation-aware workload must not claim a definite leak: {result:#?}"
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.code == "TP1002" && diagnostic.certainty == "possible" }));
    assert_eq!(result.coverage.status, "incomplete");
}

fn measure(name: &str, files: &[SourceFile], verify: fn(&Inspection)) {
    let cold_samples = (0..REPEATS)
        .map(|_| {
            let mut session = AnalysisSession::default();
            let start = Instant::now();
            let result = session.check(files, "3.11");
            let elapsed = start.elapsed();
            verify(&result);
            elapsed
        })
        .collect::<Vec<_>>();

    let mut session = AnalysisSession::default();
    let cold = session.check(files, "3.11");
    verify(&cold);
    let warm_samples = (0..REPEATS)
        .map(|_| {
            let start = Instant::now();
            let result = session.check(files, "3.11");
            assert_eq!(result, cold);
            start.elapsed()
        })
        .collect::<Vec<_>>();

    println!(
        "{name}: cold {:.3} ms ({} repeats), warm {:.3} ms, source {} bytes",
        milliseconds(median_duration(cold_samples)),
        REPEATS,
        milliseconds(median_duration(warm_samples)),
        files.iter().map(|file| file.source.len()).sum::<usize>()
    );
}

fn main() {
    let resources = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16)
        .clamp(1, MAX_RESOURCES);
    let common = provider();
    let sync_files = [common.clone(), sync_client(resources)];
    let async_files = [common, async_client(resources)];

    println!("Resources per scenario: {resources}; target Python 3.11");
    measure(
        "try/except/else/finally + while + multiple closing managers",
        &sync_files,
        verify_sync,
    );
    measure(
        "direct awaited async close + cancellation",
        &async_files,
        verify_async,
    );
}

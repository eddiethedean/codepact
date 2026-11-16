use codepact::{AnalysisSession, SourceFile};
use serde_json::json;
use std::{env, time::Instant};

fn parse_usize(value: Option<String>, name: &str) -> Result<usize, String> {
    value
        .ok_or_else(|| format!("missing {name}"))?
        .parse::<usize>()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        ((u128::from(sorted[middle - 1]) + u128::from(sorted[middle])) / 2) as u64
    } else {
        sorted[middle]
    }
}

fn samples_json(values: &[u64]) -> serde_json::Value {
    json!({
        "samples_ns": values,
        "median_ns": median(values),
    })
}

fn independent_files(count: usize) -> Vec<SourceFile> {
    (0..count)
        .map(|index| {
            SourceFile::new(
                format!("module_{index}.py"),
                format!("module_{index}"),
                "def value():\n    return 1\n",
            )
        })
        .collect()
}

fn provider_files(consumer_count: usize) -> Vec<SourceFile> {
    let mut files = Vec::with_capacity(consumer_count + 1);
    files.push(SourceFile::new(
        "provider.py",
        "provider",
        "def value():\n    return 1\n",
    ));
    files.extend((0..consumer_count).map(|index| {
        SourceFile::new(
            format!("consumer_{index}.py"),
            format!("consumer_{index}"),
            "from provider import value\ndef run():\n    return value()\n",
        )
    }));
    files
}

fn run_session_workload(
    workload: &str,
    files: &[SourceFile],
    edited_index: usize,
    sample_count: usize,
) -> Result<serde_json::Value, String> {
    if files.is_empty() || edited_index >= files.len() {
        return Err("workload requires at least one valid source file".into());
    }
    if files.len() > codepact::DEFAULT_MAX_INPUT_FILES
        || files.iter().map(|file| file.source.len()).sum::<usize>()
            > codepact::DEFAULT_MAX_SOURCE_BYTES
    {
        return Err("workload exceeds the checker input limits".into());
    }

    let mut cold_samples = Vec::with_capacity(sample_count);
    let mut warm_samples = Vec::with_capacity(sample_count);
    let mut edit_samples = Vec::with_capacity(sample_count);
    let source_bytes = files.iter().map(|file| file.source.len()).sum::<usize>();
    let edit_cache_reset_primed = files
        .len()
        .is_multiple_of(codepact::DEFAULT_MAX_SESSION_MUTATIONS);

    for _ in 0..sample_count {
        let mut session = AnalysisSession::default();
        let started = Instant::now();
        let baseline = session.check(files, "3.13");
        cold_samples.push(started.elapsed().as_nanos() as u64);

        let started = Instant::now();
        let reused = session.check(files, "3.13");
        warm_samples.push(started.elapsed().as_nanos() as u64);
        assert_eq!(reused, baseline, "warm result changed for {workload}");

        if edit_cache_reset_primed {
            let mut reset_files = files.to_vec();
            reset_files[edited_index]
                .source
                .push_str("# prime cache after automatic reset\n");
            let reset_result = session.check(&reset_files, "3.13");
            assert_eq!(
                reset_result,
                codepact::check(&reset_files, "3.13"),
                "reset result differs from fresh analysis for {workload}"
            );
            let restored = session.check(files, "3.13");
            assert_eq!(restored, baseline, "restored result changed for {workload}");
        }

        let mut edited_files = files.to_vec();
        edited_files[edited_index]
            .source
            .push_str("# benchmark source edit\n");
        let started = Instant::now();
        let edited = session.check(&edited_files, "3.13");
        edit_samples.push(started.elapsed().as_nanos() as u64);
        assert_eq!(
            edited,
            codepact::check(&edited_files, "3.13"),
            "incremental result differs from a fresh result for {workload}"
        );
    }

    Ok(json!({
        "workload": workload,
        "files": files.len(),
        "source_bytes": source_bytes,
        "samples": sample_count,
        "edit_cache_reset_primed": edit_cache_reset_primed,
        "cold": samples_json(&cold_samples),
        "warm": samples_json(&warm_samples),
        "edited": samples_json(&edit_samples),
    }))
}

fn run_function_memory_workload(
    padding_bytes: usize,
    sample_count: usize,
) -> Result<serde_json::Value, String> {
    let mut source = String::new();
    for index in 0..250 {
        source.push_str(&format!("def function_{index}():\n    return {index}\n"));
    }
    if source.len().saturating_add(1).saturating_add(padding_bytes)
        > codepact::DEFAULT_MAX_SOURCE_BYTES
    {
        return Err("memory workload exceeds the per-file source limit".into());
    }
    source.push('#');
    source.push_str(&"x".repeat(padding_bytes));

    let file = SourceFile::new("functions.py", "functions", source);
    let mut cold_samples = Vec::with_capacity(sample_count);
    let mut expected = None;
    for _ in 0..sample_count {
        let started = Instant::now();
        let result = codepact::check(std::slice::from_ref(&file), "3.13");
        cold_samples.push(started.elapsed().as_nanos() as u64);
        if let Some(expected) = &expected {
            assert_eq!(&result, expected, "function memory result changed");
        } else {
            expected = Some(result);
        }
    }

    Ok(json!({
        "workload": "function_source_sharing",
        "functions": 250,
        "padding_bytes": padding_bytes,
        "source_bytes": file.source.len(),
        "samples": sample_count,
        "cold": samples_json(&cold_samples),
    }))
}

fn run() -> Result<serde_json::Value, String> {
    let mut arguments = env::args().skip(1);
    let workload = arguments.next().ok_or_else(|| {
        "usage: benchmark_rust_optimizations <memory|session|provider> <size> [samples]".to_owned()
    })?;
    let size = parse_usize(arguments.next(), "size")?;
    let sample_count = arguments
        .next()
        .map(|value| parse_usize(Some(value), "samples"))
        .transpose()?
        .unwrap_or(7);
    if sample_count == 0 {
        return Err("samples must be greater than zero".into());
    }

    match workload.as_str() {
        "memory" => run_function_memory_workload(size, sample_count),
        "session" => {
            if size > codepact::DEFAULT_MAX_INPUT_FILES {
                return Err("session workload exceeds the file count limit".into());
            }
            run_session_workload(
                "independent_modules",
                &independent_files(size),
                0,
                sample_count,
            )
        }
        "provider" => {
            if size >= codepact::DEFAULT_MAX_INPUT_FILES {
                return Err("provider workload exceeds the file count limit".into());
            }
            run_session_workload("shared_provider", &provider_files(size), 0, sample_count)
        }
        _ => Err(format!("unknown workload {workload:?}")),
    }
}

fn main() {
    match run() {
        Ok(result) => println!("{result}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

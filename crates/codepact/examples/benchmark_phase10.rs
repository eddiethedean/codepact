use codepact::{
    adoption::{resolve_config, EffectiveConfig, Overrides},
    foundation::{FoundationInputs, RuntimeProfile},
    review::{capture, compare, ReviewSnapshot},
    AnalysisSession, CheckOptions, Ownership, SourceFile,
};
use std::{collections::BTreeSet, fmt::Write, time::Instant};

struct Workload {
    imports: &'static str,
    declaration: &'static str,
    original: &'static str,
    repaired: &'static str,
    operation_symbol: &'static str,
    operation_effect: &'static str,
}

fn workload(name: &str) -> Workload {
    match name {
        "queue" => Workload {
            imports: "from asyncio import Queue\n",
            declaration: "async def",
            original: "    work = Queue(1)
    await work.put(7)
    await work.get()
    await work.join()
",
            repaired: "    work = Queue(1)
    await work.put(7)
    await work.get()
    work.task_done()
    await work.join()
",
            operation_symbol: "asyncio.Queue",
            operation_effect: "task_done",
        },
        "ipc" => Workload {
            imports: "from multiprocessing import get_context\n",
            declaration: "def",
            original: "    ctx = get_context('spawn')
    writer, reader = ctx.Pipe()
    writer.send(7)
    reader.recv()
    reader.close()
",
            repaired: "    ctx = get_context('spawn')
    writer, reader = ctx.Pipe()
    writer.send(7)
    reader.recv()
    writer.close()
    reader.close()
",
            operation_symbol: "multiprocessing.Connection",
            operation_effect: "receive",
        },
        "guard" => Workload {
            imports: "from multiprocessing import get_context\n",
            declaration: "def",
            original: "    ctx = get_context('spawn')
    value = ctx.Value('i', 0)
    value.value += 1
",
            repaired: "    ctx = get_context('spawn')
    value = ctx.Value('i', 0)
    with value.get_lock():
        value.value += 1
",
            operation_symbol: "multiprocessing.Value",
            operation_effect: "guarded-compound-update",
        },
        "async-stream" => Workload {
            imports: "from asyncio import open_connection\n",
            declaration: "async def",
            original: "    reader, writer = await open_connection('127.0.0.1', 9000)\n    writer.close()\n",
            repaired: "    reader, writer = await open_connection('127.0.0.1', 9000)\n    writer.close()\n    await writer.wait_closed()\n",
            operation_symbol: "asyncio.StreamWriter",
            operation_effect: "observe-closure",
        },
        "async-subprocess" => Workload {
            imports: "from asyncio import create_subprocess_exec\n",
            declaration: "async def",
            original: "    child = await create_subprocess_exec('/usr/bin/true')\n    child.terminate()\n",
            repaired: "    child = await create_subprocess_exec('/usr/bin/true')\n    child.terminate()\n    await child.wait()\n",
            operation_symbol: "asyncio.subprocess.Process",
            operation_effect: "observe-process-exit",
        },
        "executor-bridge" => Workload {
            imports: "from asyncio import get_running_loop\ndef worker(value):\n    return value\n",
            declaration: "async def",
            original: "    loop = get_running_loop()\n    job = loop.run_in_executor(None, worker, 1)\n",
            repaired: "    loop = get_running_loop()\n    job = loop.run_in_executor(None, worker, 1)\n    await job\n",
            operation_symbol: "asyncio.AbstractEventLoop.run_in_executor",
            operation_effect: "dispatch",
        },
        "shared-memory" => Workload {
            imports: "from multiprocessing import get_context\nfrom multiprocessing.shared_memory import SharedMemory\n",
            declaration: "def",
            original: "    ctx = get_context('spawn')\n    memory = SharedMemory(create=True, size=8)\n    memory.close()\n",
            repaired: "    ctx = get_context('spawn')\n    memory = SharedMemory(create=True, size=8)\n    memory.close()\n    memory.unlink()\n",
            operation_symbol: "multiprocessing.shared_memory.SharedMemory",
            operation_effect: "close-local-mapping",
        },
        _ => panic!("unknown protocol workload: {name}"),
    }
}

fn source(workload: &Workload, body: &str, count: usize) -> SourceFile {
    let mut caller = workload.imports.to_owned();
    for index in 0..count {
        writeln!(caller, "{} check_{index}():\n{body}", workload.declaration).unwrap();
    }
    SourceFile::new("caller.py", "caller", caller)
}

fn run(
    session: &mut AnalysisSession,
    files: &[SourceFile],
    config: &EffectiveConfig,
) -> ReviewSnapshot {
    let options = CheckOptions {
        ownership: config.ownership,
        ..CheckOptions::default()
    };
    let foundations = FoundationInputs {
        runtime: config.runtime_profile.clone(),
        ..FoundationInputs::default()
    };
    capture(session, files, config, options, Some(&foundations), None).unwrap()
}

fn timed<T>(samples: &mut Vec<f64>, measure: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = measure();
    samples.push(started.elapsed().as_secs_f64() * 1000.0);
    result
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let name = arguments.next().expect("workload is required");
    let count = arguments
        .next()
        .expect("caller count is required")
        .parse::<usize>()
        .expect("caller count must be a positive integer");
    assert!(count > 0);

    let workload = workload(&name);
    let mut config = resolve_config(
        &std::env::current_dir().unwrap(),
        &Overrides {
            no_config: true,
            python_version: Some("3.13".into()),
            experimental_ownership: Some(true),
            ..Overrides::default()
        },
    )
    .unwrap();
    config.ownership = Ownership::AdvancedAssumeDefaultTaskFactory;
    config.runtime_profile = Some(RuntimeProfile {
        implementation: "cpython".into(),
        python_version: "3.13.16".into(),
        os: "linux".into(),
        architecture: "x86_64".into(),
        event_loop: Some("default".into()),
        start_method: Some("spawn".into()),
        spawn_main_script: Some("caller.py".into()),
        assume_standard_process_wait: true,
    });
    let mut unsupported = config.clone();
    unsupported.runtime_profile.as_mut().unwrap().architecture = "aarch64".into();

    let original = [
        source(&workload, workload.original, count),
        SourceFile::new("extra.py", "extra", "value = 1\n"),
    ];
    let fixed = [
        source(&workload, workload.repaired, count),
        original[1].clone(),
    ];
    let unrelated = [
        fixed[0].clone(),
        SourceFile::new("extra.py", "extra", "value = 2\n"),
    ];
    let mut cold_ms = Vec::new();
    let mut unchanged_ms = Vec::new();
    let mut affected_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    let mut profile_edit_ms = Vec::new();
    let mut comparison_ms = Vec::new();
    let mut verdicts = BTreeSet::new();
    let prefix = format!("{}.", workload.operation_symbol);
    for _ in 0..7 {
        let mut session = AnalysisSession::default();
        let before = timed(&mut cold_ms, || run(&mut session, &original, &config));
        let unchanged = timed(&mut unchanged_ms, || run(&mut session, &original, &config));
        let after = timed(&mut affected_edit_ms, || run(&mut session, &fixed, &config));
        let independent = timed(&mut unrelated_edit_ms, || {
            run(&mut session, &unrelated, &config)
        });
        let invalidated = timed(&mut profile_edit_ms, || {
            run(&mut session, &unrelated, &unsupported)
        });
        let comparison = timed(&mut comparison_ms, || {
            compare(&before, &after, &BTreeSet::new(), None).unwrap()
        });
        assert_eq!(before, unchanged);
        assert_eq!(after, run(&mut AnalysisSession::default(), &fixed, &config));
        assert_eq!(
            independent,
            run(&mut AnalysisSession::default(), &unrelated, &config)
        );
        assert_eq!(
            invalidated,
            run(&mut AnalysisSession::default(), &unrelated, &unsupported)
        );
        assert_eq!(independent, run(&mut session, &unrelated, &config));
        let applicable_sites = |snapshot: &ReviewSnapshot| {
            snapshot
                .foundations
                .as_ref()
                .unwrap()
                .model_uses
                .iter()
                .filter(|site| {
                    site.applicable
                        && site.effect == workload.operation_effect
                        && (site.symbol == workload.operation_symbol
                            || site.symbol.starts_with(prefix.as_str()))
                })
                .count()
        };
        assert!(
            applicable_sites(&after) >= count,
            "missing applicable model facts for {name}"
        );
        assert_eq!(applicable_sites(&invalidated), 0);
        assert_eq!(invalidated.coverage.status, "incomplete");
        verdicts.insert(comparison.verdict.as_str());
    }
    println!(
        "{}",
        serde_json::json!({
            "workload": name,
            "caller_functions": count,
            "cold_ms": cold_ms,
            "unchanged_ms": unchanged_ms,
            "affected_edit_ms": affected_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms,
            "profile_edit_ms": profile_edit_ms,
            "comparison_ms": comparison_ms,
            "comparison_verdicts": verdicts,
        })
    );
}

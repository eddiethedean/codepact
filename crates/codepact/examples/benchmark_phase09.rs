use codepact::{
    adoption::{resolve_config, Overrides},
    foundation::{FoundationInputs, RuntimeProfile},
    review::{capture, compare},
    AnalysisSession, CheckOptions, Ownership, SourceFile,
};
use std::{collections::BTreeSet, fmt::Write, time::Instant};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let workload = arguments.next().expect("workload is required");
    let count = arguments.next().unwrap().parse::<usize>().unwrap();
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
    let runtime = FoundationInputs {
        runtime: Some(RuntimeProfile {
            implementation: "cpython".into(),
            python_version: "3.13.16".into(),
            os: "linux".into(),
            architecture: "x86_64".into(),
            event_loop: Some("default".into()),
            start_method: Some("spawn".into()),
            assume_standard_process_wait: true,
        }),
        ..FoundationInputs::default()
    };
    config.runtime_profile = runtime.runtime.clone();
    let (mut caller, body, old, new, diagnostic) = match workload.as_str() {
        "review" => (
            String::from("from codepact import MustHandle, Result, Ok, Err\ndef produce() -> MustHandle[Result[int, str]]:\n    return Ok(1)\n"),
            "    item = produce()\n    alias = item\n",
            "    alias = item",
            "    alias = item\n    match alias:\n        case Ok(value): pass\n        case Err(error): pass",
            "TP5002",
        ),
        "work" => {
            config.ownership = Ownership::AdvancedAssumeDefaultTaskFactory;
            config.enabled_rules.insert("TP3003".into());
            (
                String::from("from asyncio import TaskGroup\nasync def worker(): return 1\n"),
                "    async with TaskGroup() as group:\n        first = group.create_task(worker())\n        second = group.create_task(worker())\n    first.done()\n    second.done()\n",
                "    first.done()\n    second.done()",
                "    try:\n        first.result()\n    finally:\n        second.result()",
                "TP3003",
            )
        }
        _ => panic!("workload must be review or work"),
    };
    let options = CheckOptions {
        ownership: config.ownership,
        ..CheckOptions::default()
    };
    let keyword = if workload == "work" {
        "async def"
    } else {
        "def"
    };
    for index in 0..count {
        writeln!(caller, "{keyword} check_{index}():\n{body}").unwrap();
    }
    let files = [
        SourceFile::new("caller.py", "caller", &caller),
        SourceFile::new("extra.py", "extra", "value = 1\n"),
    ];
    let mut fixed = files.clone();
    fixed[0].source = caller.replace(old, new);
    let mut unrelated = fixed.clone();
    unrelated[1].source = "value = 2\n".into();
    let run = |session: &mut AnalysisSession, files: &[SourceFile]| {
        capture(
            session,
            files,
            &config,
            options.clone(),
            Some(&runtime),
            None,
        )
        .unwrap()
    };
    let mut cold_ms = Vec::new();
    let mut unchanged_ms = Vec::new();
    let mut affected_edit_ms = Vec::new();
    let mut unrelated_edit_ms = Vec::new();
    let mut comparison_ms = Vec::new();
    for _ in 0..7 {
        let mut session = AnalysisSession::default();
        let start = Instant::now();
        let before = run(&mut session, &files);
        cold_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        let unchanged = run(&mut session, &files);
        unchanged_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(before, unchanged);
        let start = Instant::now();
        let after = run(&mut session, &fixed);
        affected_edit_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(after, run(&mut AnalysisSession::default(), &fixed));
        let start = Instant::now();
        let changed = run(&mut session, &unrelated);
        unrelated_edit_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(changed, run(&mut AnalysisSession::default(), &unrelated));
        let repair_targets = if workload == "review" {
            before
                .findings
                .iter()
                .filter(|finding| finding.code == diagnostic)
                .map(|finding| finding.fingerprint.clone())
                .collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        assert_eq!(
            before
                .findings
                .iter()
                .filter(|finding| finding.code == diagnostic)
                .count(),
            if workload == "review" {
                count
            } else {
                count * 2
            },
        );
        if workload == "review" {
            assert!(after
                .findings
                .iter()
                .all(|finding| finding.code != diagnostic));
        } else {
            assert!(
                after
                    .findings
                    .iter()
                    .all(|finding| finding.code != diagnostic || !finding.actionable),
                "{:?}",
                after.findings.iter().take(5).collect::<Vec<_>>(),
            );
            let model_uses = &after.foundations.as_ref().expect("work profile").model_uses;
            assert_eq!(
                model_uses
                    .iter()
                    .filter(|site| site.symbol == "asyncio.Task"
                        && site.effect == "result"
                        && site.applicable)
                    .count(),
                count * 2,
            );
        }
        let start = Instant::now();
        let review = compare(&before, &after, &repair_targets, None).unwrap();
        comparison_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(before.check_exit, 1);
        assert_eq!(review.after_check_exit, 0);
        assert_eq!(
            review.verdict,
            codepact::review::ReviewVerdict::Passed,
            "{review:?}"
        );
    }
    println!(
        "{}",
        serde_json::json!({
            "workload": workload, "caller_functions": count,
            "cold_ms": cold_ms, "unchanged_ms": unchanged_ms, "affected_edit_ms": affected_edit_ms,
            "unrelated_edit_ms": unrelated_edit_ms, "comparison_ms": comparison_ms,
        })
    );
}

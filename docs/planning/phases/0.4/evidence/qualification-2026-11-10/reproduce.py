"""Reproduce CLI/corpus, incremental and operational release qualification.

Run from the repository root after building optimized candidate examples:
python reproduce.py /absolute/candidate/codepact /absolute/evidence-directory
The binary may be an installed candidate wheel; tests compare it with the
current source API. Outputs are engineering controls, not field precision.
"""

import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import tarfile
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def execute(command, destination, *, cwd=None, env=None):
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True)
    destination.write_bytes(result.stdout + b"\nSTDERR\n" + result.stderr)
    assert result.returncode == 0, (command, result.returncode, destination)
    return result


def main():
    repository = Path.cwd()
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["PATH"] = (
        str(Path(sys.executable).parent) + os.pathsep + environment.get("PATH", "")
    )
    environment["CODEPACT_QUALIFICATION_BINARY"] = str(binary)
    environment["CODEPACT_QUALIFICATION_EVIDENCE"] = str(output / "cli")
    environment["CODEPACT_SYNTHETIC_RESULTS_DIR"] = str(output / "synthetic")
    records = []
    execute(
        [
            "cargo",
            "test",
            "-p",
            "codepact",
            "--test",
            "release_qualification",
            "--test",
            "synthetic_projects",
            "--locked",
            "--",
            "--nocapture",
        ],
        output / "controls.log",
        env=environment,
    )
    execute(
        [sys.executable, "scripts/check_schema_fixtures.py"],
        output / "schemas.log",
        env=environment,
    )
    execute(
        [
            "cargo",
            "test",
            "-p",
            "codepact",
            "--release",
            "--lib",
            "v04_20_21_policy",
            "--locked",
            "--",
            "--nocapture",
        ],
        output / "sessions.log",
        env=environment,
    )
    session_text = (output / "sessions.log").read_text()
    session = json.loads(
        next(
            line.removeprefix("SESSION_QUALIFICATION ")
            for line in session_text.splitlines()
            if line.startswith("SESSION_QUALIFICATION ")
        )
    )
    assert len(session) == 7
    for scenario in session:
        assert len(scenario["semantic_ms"]) == len(scenario["reporting_ms"]) == 7
        assert scenario["flow_query_executions"] == 0
    (output / "session-samples.json").write_text(json.dumps(session, indent=2) + "\n")
    # Both examples use identical corpus generators and seven samples in the
    # released 0.3 crate and candidate. Only harness output was supplemented.
    baseline = (
        Path(sys.argv[3]).resolve(strict=True)
        if len(sys.argv) > 3
        else Path("/private/tmp/codepact-qualified-03-target/release/examples")
    )
    for revision, examples in [
        ("candidate", repository / "target/release/examples"),
        ("published-0.3", baseline),
    ]:
        for count in [1, 250, 1000]:
            command = [str(examples / "benchmark_lifecycle"), str(count)]
            result = execute(
                ["/usr/bin/time", "-l", *command],
                output / f"{revision}-lifecycle-{count}.log",
            )
            sample = json.loads(
                next(
                    line.removeprefix("SAMPLES ")
                    for line in result.stdout.decode().splitlines()
                    if line.startswith("SAMPLES ")
                )
            )
            for key, values in sample.items():
                if key.endswith("_ms"):
                    assert len(values) == 7
            peak = re.search(rb"(\d+)\s+maximum resident set size", result.stderr)
            assert peak is not None
            records.append(
                {
                    "revision": revision,
                    "kind": "lifecycle",
                    "objects": count,
                    "samples": sample,
                    "peak_rss_bytes": int(peak[1]),
                }
            )
        for count in [16, 128]:
            result = execute(
                [
                    "/usr/bin/time",
                    "-l",
                    str(examples / "benchmark_phase03"),
                    str(count),
                ],
                output / f"{revision}-cleanup-async-{count}.log",
            )
            samples = [
                json.loads(line.removeprefix("SAMPLES "))
                for line in result.stdout.decode().splitlines()
                if line.startswith("SAMPLES ")
            ]
            assert len(samples) == 2
            for sample in samples:
                assert len(sample["cold_ms"]) == len(sample["warm_ms"]) == 7
            peak = re.search(rb"(\d+)\s+maximum resident set size", result.stderr)
            assert peak is not None
            records.append(
                {
                    "revision": revision,
                    "kind": "cleanup-async",
                    "objects": count,
                    "samples": samples,
                    "peak_rss_bytes": int(peak[1]),
                }
            )
    memory_env = environment.copy()
    memory_env["CODEPACT_RETAINED_MEMORY_PROBE"] = "1"
    retained = execute(
        [str(repository / "target/release/examples/benchmark_lifecycle"), "1000"],
        output / "retained-session-memory.log",
        env=memory_env,
    )
    assert b"Retained-session RSS:" in retained.stdout
    old_retained = execute(
        [str(baseline / "benchmark_lifecycle"), "1000"],
        output / "published-03-retained-session-memory.log",
        env=memory_env,
    )
    assert b"Retained-session RSS:" in old_retained.stdout
    # Measure every preview variant; exact semantic oracles are independently
    # enforced by the test command above for all three target versions.
    corpus = repository / "crates/codepact/tests/projects"
    manifest = json.loads((corpus / "manifest.json").read_text())
    for project in manifest["projects"]:
        variants = ["buggy", "fixed"] if project["kind"] == "repair" else ["boundary"]
        for variant in variants:
            root = corpus / project["id"] / variant
            before = {
                str(p.relative_to(root)): sha(p) for p in root.rglob("*") if p.is_file()
            }
            command = [str(binary), "check", "--json", "--python-version", "3.11"]
            samples, hashes = [], []
            for _ in range(7):
                start = time.perf_counter_ns()
                result = subprocess.run(command, cwd=root, capture_output=True)
                samples.append((time.perf_counter_ns() - start) / 1_000_000)
                assert result.returncode in {0, 1} and not result.stderr
                report = json.loads(result.stdout)
                assert (
                    report["execution_status"] == "completed"
                    and not report["details_truncated"]
                )
                hashes.append(hashlib.sha256(result.stdout).hexdigest())
            assert len(set(hashes)) == 1
            measured = subprocess.run(
                ["/usr/bin/time", "-l", *command], cwd=root, capture_output=True
            )
            peak = re.search(rb"(\d+)\s+maximum resident set size", measured.stderr)
            assert peak is not None and measured.returncode in {0, 1}
            assert {
                str(p.relative_to(root)): sha(p) for p in root.rglob("*") if p.is_file()
            } == before
            records.append(
                {
                    "kind": "preview-corpus",
                    "project": project["id"],
                    "variant": variant,
                    "samples_ms": samples,
                    "median_ms": statistics.median(samples),
                    "min_ms": min(samples),
                    "max_ms": max(samples),
                    "output_sha256": hashes[0],
                    "output_bytes": len(result.stdout),
                    "peak_rss_bytes": int(peak[1]),
                    "coverage": report["coverage"],
                    "counts": report["counts"],
                    "exit": result.returncode,
                    "input_sha256": before,
                }
            )
    (output / "measurements.json").write_text(json.dumps(records, indent=2) + "\n")
    execute(
        [sys.executable, "scripts/check_synthetic_static_tools.py"],
        output / "pyright.log",
        env=environment,
    )
    execute(
        ["ruff", "check", "crates/codepact/tests/projects"],
        output / "ruff.log",
        env=environment,
    )
    execute(
        [
            sys.executable,
            "-m",
            "unittest",
            "discover",
            "-s",
            "tests/python",
            "-p",
            "test_report_consumer.py",
            "-v",
        ],
        output / "consumer.log",
        env=environment,
    )
    execute(
        [
            sys.executable,
            str(Path(__file__).with_name("reproduce_adoption.py")),
            str(binary),
            str(output / "adoption-cli.json"),
        ],
        output / "adoption-cli.log",
        env=environment,
    )
    config_runner = (
        repository
        / "docs/planning/phases/0.4/evidence/config-qualification-2026-11-10/reproduce.py"
    )
    execute(
        [
            sys.executable,
            str(config_runner),
            str(binary),
            str(output / "configuration.json"),
        ],
        output / "configuration.log",
        env=environment,
    )
    metadata = {
        "base_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "binary_sha256": sha(binary),
        "binary_version": subprocess.check_output(
            [str(binary), "--version"], text=True
        ).strip(),
        "host": platform.platform(),
        "python": platform.python_version(),
        "cpu": subprocess.check_output(
            ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
        ).strip(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "manifest_sha256": sha(corpus / "manifest.json"),
        "source_sha256": {
            str(p): sha(p) for p in sorted(repository.glob("crates/codepact/src/*.rs"))
        },
        "baseline_examples_sha256": {
            p.name: sha(p)
            for p in [baseline / "benchmark_lifecycle", baseline / "benchmark_phase03"]
        },
        "candidate_examples_sha256": {
            p.name: sha(p)
            for p in [
                repository / "target/release/examples/benchmark_lifecycle",
                repository / "target/release/examples/benchmark_phase03",
            ]
        },
        "published_03_crate_sha256": sha(
            Path("/private/tmp/codepact-0.3-published/codepact-0.3.0.crate")
        ),
        "runner_sha256": sha(Path(__file__)),
        "real_world_usefulness": "unqualified",
        "method": "Seven optimized samples; source API and full CLI oracles; no OS cache flush. /usr/bin/time -l per-workload peak RSS; ps reports retained session RSS after diagnostics are dropped. Corpus process timings include launch/output capture. No field precision or recall claim.",
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w") as tar:
        for path in sorted(output.rglob("*")):
            if not path.is_file():
                continue
            raw = path.read_bytes()
            info = tarfile.TarInfo(str(path.relative_to(output)))
            info.size = len(raw)
            info.mtime = 0
            tar.addfile(info, io.BytesIO(raw))
    (output.parent / (output.name + ".tar.gz")).write_bytes(
        gzip.compress(archive.getvalue(), mtime=0)
    )
    print(
        f"Qualified controls and collected {len(records)} operational records in {output}"
    )


if __name__ == "__main__":
    main()

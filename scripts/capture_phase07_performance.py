"""Capture the 0.6.0 and 0.7.0 phase benchmark comparison as JSON."""

from __future__ import annotations

import argparse
from datetime import UTC, datetime
import json
import os
import platform
from pathlib import Path
import subprocess
import sys
from typing import cast

BASELINE_COMMIT = "2d638e761a26211e5c98f1792731103dd9856994"
COMPARABLE_WORKLOADS = ("state", "exclusive", "executor")
COMPARABLE_SIZE = 250
NEW_WORKLOADS = (("matches", 64), ("matches", 256), ("provider", 64), ("provider", 256))
PROCESS_REPEATS = int(os.environ.get("CODEPACT_PHASE07_PROCESS_REPEATS", "3"))
if PROCESS_REPEATS < 1:
    raise ValueError("CODEPACT_PHASE07_PROCESS_REPEATS must be positive")


class Arguments(argparse.Namespace):
    baseline_root: Path
    candidate_root: Path
    output: Path


def command_peak_rss_bytes(command: list[str]) -> dict[str, object]:
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    try:
        import resource

        peak_rss = int(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
        if sys.platform != "darwin":
            peak_rss *= 1024
    except ImportError:
        peak_rss = -1
    print(
        json.dumps(
            {
                "return_code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "peak_rss_bytes": peak_rss,
            }
        )
    )
    return {"return_code": result.returncode}


def measure_once(binary: Path, workload: str, size: int) -> dict[str, object]:
    command = [str(binary), workload, str(size)]
    wrapper = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).resolve()),
            "--measure-child",
            "--",
            *command,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if wrapper.returncode != 0:
        raise RuntimeError(f"benchmark failed: {command}\n{wrapper.stderr}")
    child = cast(dict[str, object], json.loads(wrapper.stdout))
    if child["return_code"] != 0:
        raise RuntimeError(
            f"benchmark failed: {command}\n{child['stderr']}\n{child['stdout']}"
        )
    raw = cast(dict[str, object], json.loads(cast(str, child["stdout"])))
    return {
        "command": command,
        "peak_rss_bytes": None
        if child["peak_rss_bytes"] == -1
        else child["peak_rss_bytes"],
        "process_stderr": child["stderr"],
        "raw": raw,
    }


def median_ms(measurement: dict[str, object], field: str) -> float:
    replicates = cast(list[dict[str, object]], measurement["replicates"])
    values = [
        float(value)
        for replicate in replicates
        for value in cast(list[float], cast(dict[str, object], replicate["raw"])[field])
    ]
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def median_replicate_metric(
    measurement: dict[str, object], field: str
) -> dict[str, float]:
    replicates = cast(list[dict[str, object]], measurement["replicates"])
    samples = [
        cast(dict[str, object], cast(dict[str, object], replicate["raw"])[field])
        for replicate in replicates
    ]
    return {
        metric: sorted(cast(float, sample[metric]) for sample in samples)[
            len(samples) // 2
        ]
        for metric in samples[0]
    }


def measure(binary: Path, workload: str, size: int) -> dict[str, object]:
    replicates = [measure_once(binary, workload, size) for _ in range(PROCESS_REPEATS)]
    peak_values = [
        cast(int, item["peak_rss_bytes"])
        for item in replicates
        if item["peak_rss_bytes"] is not None
    ]
    return {
        "command": [str(binary), workload, str(size)],
        "peak_rss_bytes": max(peak_values) if peak_values else None,
        "replicates": replicates,
    }


def revision(root: Path) -> str:
    return subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def rust_version(root: Path) -> str:
    return subprocess.run(
        ["rustc", "-V"], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()


def capture(baseline_root: Path, candidate_root: Path) -> dict[str, object]:
    baseline_binary = baseline_root / "target/release/examples/benchmark_phase06"
    candidate_phase06 = candidate_root / "target/release/examples/benchmark_phase06"
    candidate_phase07 = candidate_root / "target/release/examples/benchmark_phase07"
    for binary in (baseline_binary, candidate_phase06, candidate_phase07):
        if not binary.is_file():
            raise FileNotFoundError(f"build the benchmark example first: {binary}")

    comparable: list[dict[str, object]] = []
    for workload in COMPARABLE_WORKLOADS:
        baseline = measure(baseline_binary, workload, COMPARABLE_SIZE)
        candidate = measure(candidate_phase06, workload, COMPARABLE_SIZE)
        fields = {
            "cold": "cold_ms",
            "unchanged_session": "warm_ms",
            "affected_edit": "affected_edit_ms",
            "unrelated_edit": "unrelated_edit_ms",
        }
        comparable.append(
            {
                "workload": workload,
                "size": COMPARABLE_SIZE,
                "baseline": {
                    **baseline,
                    "median_ms": {
                        label: median_ms(baseline, field)
                        for label, field in fields.items()
                    },
                },
                "candidate": {
                    **candidate,
                    "median_ms": {
                        label: median_ms(candidate, field)
                        for label, field in fields.items()
                    },
                },
            }
        )

    phase07: list[dict[str, object]] = []
    for workload, size in NEW_WORKLOADS:
        measurement = measure(candidate_phase07, workload, size)
        first_replicate = cast(list[dict[str, object]], measurement["replicates"])[0]
        raw = cast(dict[str, object], first_replicate["raw"])
        phase07.append(
            {
                "workload": workload,
                "size": size,
                **measurement,
                "median_ms": median_replicate_metric(measurement, "median_ms"),
                "source_bytes": raw["source_bytes"],
                "discovered_input_files": raw["discovered_input_files"],
                "selected_scope": raw["selected_scope"],
                "disabled_scope": raw["disabled_scope"],
                "diagnostics": raw["diagnostics"],
                "coverage": raw["coverage"],
            }
        )

    return {
        "schema_version": 1,
        "qualification": "phase-0.7-optimized-performance",
        "generated_at_utc": datetime.now(UTC).isoformat(),
        "baseline_commit": BASELINE_COMMIT,
        "candidate_commit": revision(candidate_root),
        "environment": {
            "platform": platform.platform(),
            "python": sys.version,
            "rustc": rust_version(candidate_root),
            "peak_rss_method": "isolated benchmark subprocess resource.getrusage",
            "peak_rss_unit": "bytes",
        },
        "repeats_per_benchmark": 7,
        "process_repeats_per_benchmark": PROCESS_REPEATS,
        "comparable_workloads": comparable,
        "phase07_workloads": phase07,
    }


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "--measure-child":
        child = sys.argv[2:]
        command = child[1:] if child and child[0] == "--" else child
        command_peak_rss_bytes(command)
        return

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", type=Path, required=True)
    parser.add_argument("--candidate-root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    arguments = Arguments()
    parser.parse_args(namespace=arguments)
    record = capture(
        arguments.baseline_root.resolve(), arguments.candidate_root.resolve()
    )
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded phase 0.7 performance evidence at {arguments.output}")


if __name__ == "__main__":
    main()

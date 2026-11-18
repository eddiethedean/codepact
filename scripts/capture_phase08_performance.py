"""Measure the inherited and installed-manifest analysis workloads."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import platform
from statistics import median
from typing import cast

from capture_phase07_performance import (
    COMPARABLE_WORKLOADS,
    PROCESS_REPEATS,
    measure_comparison,
    measure_once,
    measurement_record,
    revision,
    rust_version,
    trial_times,
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--baseline", required=True, type=Path)
    _ = parser.add_argument("--candidate", default=Path.cwd(), type=Path)
    _ = parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    baseline_root = cast(Path, args.baseline).resolve()
    candidate_root = cast(Path, args.candidate).resolve()
    baseline = baseline_root / "target/release/examples/benchmark_phase06"
    candidate = candidate_root / "target/release/examples/benchmark_phase06"
    manifest = candidate.with_name("benchmark_phase08")
    comparison = [
        measure_comparison(baseline, candidate, workload)
        for workload in COMPARABLE_WORKLOADS
    ]
    workload = [measure_once(manifest, "manifest", 64) for _ in range(PROCESS_REPEATS)]
    report = {
        "schema_version": 1,
        "baseline_tag": "v0.7.0",
        "candidate_base_revision": revision(candidate_root),
        "platform": platform.platform(),
        "rustc": rust_version(candidate_root),
        "binary_sha256": {
            name: hashlib.sha256(binary.read_bytes()).hexdigest()
            for name, binary in (
                ("baseline", baseline),
                ("candidate", candidate),
                ("manifest", manifest),
            )
        },
        "comparable_workloads": comparison,
        "manifest_workload": {
            **measurement_record(workload),
            "median_ms": {
                field: median(
                    time for sample in workload for time in trial_times(sample, field)
                )
                for field in (
                    "cold_ms",
                    "unchanged_ms",
                    "affected_edit_ms",
                    "unrelated_edit_ms",
                )
            },
        },
    }
    destination = cast(Path, args.output)
    destination.parent.mkdir(parents=True, exist_ok=True)
    _ = destination.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded phase 0.8 performance evidence at {destination}")


if __name__ == "__main__":
    main()

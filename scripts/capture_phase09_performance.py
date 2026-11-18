"""Capture Phase 0.9 paired timings and review-workload costs."""

from __future__ import annotations

import argparse
from datetime import UTC, datetime
import hashlib
import json
from pathlib import Path
import platform
from statistics import median
import subprocess
from typing import cast

from capture_phase07_performance import (
    COMPARABLE_WORKLOADS,
    PROCESS_REPEATS,
    measure_comparison,
    measure_once,
    measurement_record,
    paired_change,
    revision,
    rust_version,
    trial_times,
)

FIELDS = ("cold_ms", "unchanged_ms", "affected_edit_ms", "unrelated_edit_ms")
CALLERS = 64


def summarize(
    samples: list[dict[str, object]], fields: tuple[str, ...]
) -> dict[str, object]:
    return {
        **measurement_record(samples),
        "median_ms": {
            field: median(
                value for sample in samples for value in trial_times(sample, field)
            )
            for field in fields
        },
    }


def compare_manifests(baseline: Path, candidate: Path) -> dict[str, object]:
    binaries = {"baseline": baseline, "candidate": candidate}
    samples: dict[str, list[dict[str, object]]] = {side: [] for side in binaries}
    orders: list[list[str]] = []
    for trial in range(PROCESS_REPEATS):
        order = list(binaries) if trial % 2 == 0 else list(reversed(binaries))
        orders.append(order)
        for side in order:
            samples[side].append(measure_once(binaries[side], "manifest", CALLERS))
    return {
        "workload": "installed-manifest",
        "caller_functions": CALLERS,
        "pair_order": orders,
        "paired_change": {
            field: paired_change(samples["baseline"], samples["candidate"], field)
            for field in FIELDS
        },
        **{side: summarize(runs, FIELDS) for side, runs in samples.items()},
    }


def capture(baseline: Path, candidate: Path) -> dict[str, object]:
    reference = baseline / "target/release/examples"
    proposed = candidate / "target/release/examples"
    binaries = {
        "baseline_inherited": reference / "benchmark_phase06",
        "candidate_inherited": proposed / "benchmark_phase06",
        "baseline_manifest": reference / "benchmark_phase08",
        "candidate_manifest": proposed / "benchmark_phase08",
        "candidate_review": proposed / "benchmark_phase09",
    }
    for binary in binaries.values():
        if not binary.is_file():
            raise FileNotFoundError(f"build the optimized benchmark first: {binary}")
    return {
        "schema_version": 1,
        "generated_at_utc": datetime.now(UTC).isoformat(),
        "baseline_tag": "v0.8.0",
        "baseline_tag_commit": subprocess.run(
            ["git", "rev-parse", "v0.8.0^{commit}"],
            cwd=candidate,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip(),
        "candidate_base_revision": revision(candidate),
        "platform": platform.platform(),
        "rustc": rust_version(candidate),
        "process_repeats": PROCESS_REPEATS,
        "binary_sha256": {
            label: hashlib.sha256(binary.read_bytes()).hexdigest()
            for label, binary in binaries.items()
        },
        "comparable_workloads": [
            measure_comparison(
                binaries["baseline_inherited"],
                binaries["candidate_inherited"],
                workload,
            )
            for workload in COMPARABLE_WORKLOADS
        ],
        "installed_manifest": compare_manifests(
            binaries["baseline_manifest"], binaries["candidate_manifest"]
        ),
        "candidate_workloads": {
            workload: summarize(
                [
                    measure_once(binaries["candidate_review"], workload, CALLERS)
                    for _ in range(PROCESS_REPEATS)
                ],
                (*FIELDS, "comparison_ms"),
            )
            for workload in ("review", "work")
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--baseline", required=True, type=Path)
    _ = parser.add_argument("--candidate", default=Path.cwd(), type=Path)
    _ = parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    receipt = capture(
        cast(Path, args.baseline).resolve(), cast(Path, args.candidate).resolve()
    )
    destination = cast(Path, args.output)
    destination.parent.mkdir(parents=True, exist_ok=True)
    _ = destination.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded Phase 0.9 performance evidence at {destination}")


if __name__ == "__main__":
    main()

"""Record paired CodePact 0.9.0 baselines and Phase 0.10 workload costs."""

from __future__ import annotations

import argparse
from datetime import UTC, datetime
import hashlib
import json
from pathlib import Path
import platform
import subprocess

from capture_phase07_performance import (
    COMPARABLE_WORKLOADS,
    PROCESS_REPEATS,
    measure_comparison,
    measure_once,
    revision,
    rust_version,
)
from capture_phase09_performance import (
    CALLERS,
    FIELDS,
    compare_manifests,
    paired_workload,
    summarize,
)

BASELINE_TAG = "v0.9.0"
REVIEW_FIELDS = (*FIELDS, "comparison_ms")
PROTOCOL_FIELDS = (*REVIEW_FIELDS, "profile_edit_ms")


def git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()


def capture(baseline: Path, candidate: Path) -> dict[str, object]:
    baseline_commit = git(candidate, "rev-parse", f"{BASELINE_TAG}^{{commit}}")
    if revision(baseline) != baseline_commit or git(baseline, "status", "--porcelain"):
        raise ValueError(f"baseline must be a clean checkout of {BASELINE_TAG}")

    reference = baseline / "target/release/examples"
    proposed = candidate / "target/release/examples"
    binaries = {
        "baseline_inherited": reference / "benchmark_phase06",
        "candidate_inherited": proposed / "benchmark_phase06",
        "baseline_manifest": reference / "benchmark_phase08",
        "candidate_manifest": proposed / "benchmark_phase08",
        "baseline_review": reference / "benchmark_phase09",
        "candidate_review": proposed / "benchmark_phase09",
        "candidate_protocol": proposed / "benchmark_phase10",
    }
    for binary in binaries.values():
        if not binary.is_file():
            raise FileNotFoundError(f"build the release benchmark first: {binary}")
    return {
        "schema_version": 1,
        "generated_at_utc": datetime.now(UTC).isoformat(),
        "baseline_tag": BASELINE_TAG,
        "baseline_commit": baseline_commit,
        "candidate_revision": revision(candidate),
        "candidate_dirty": bool(git(candidate, "status", "--porcelain")),
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
        "inherited_review_workloads": {
            workload: paired_workload(
                binaries["baseline_review"],
                binaries["candidate_review"],
                workload,
                REVIEW_FIELDS,
            )
            for workload in ("review", "work")
        },
        "new_protocol_workloads": {
            workload: summarize(
                [
                    measure_once(binaries["candidate_protocol"], workload, CALLERS)
                    for _ in range(PROCESS_REPEATS)
                ],
                PROTOCOL_FIELDS,
            )
            for workload in ("queue", "ipc", "guard")
        },
    }


class Arguments(argparse.Namespace):
    baseline: Path
    candidate: Path
    output: Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--baseline", required=True, type=Path)
    _ = parser.add_argument("--candidate", default=Path.cwd(), type=Path)
    _ = parser.add_argument("--output", required=True, type=Path)
    arguments = Arguments()
    _ = parser.parse_args(namespace=arguments)
    receipt = capture(arguments.baseline.resolve(), arguments.candidate.resolve())
    destination = arguments.output
    destination.parent.mkdir(parents=True, exist_ok=True)
    _ = destination.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded Phase 0.10 performance evidence at {destination}")


if __name__ == "__main__":
    main()

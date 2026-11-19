"""Compare published 0.10.0 timings and collect new Phase 0.11 workload costs."""

from __future__ import annotations

import argparse
from datetime import UTC, datetime
import hashlib
import json
from pathlib import Path
import platform

from capture_phase07_performance import (
    PROCESS_REPEATS,
    measure_once,
    revision,
    rust_version,
)
from capture_phase09_performance import CALLERS, paired_workload, summarize
from capture_phase10_performance import PROTOCOL_FIELDS, REVIEW_FIELDS, git

BASELINE_TAG = "v0.10.0"
NEW_WORKLOADS = ("async-stream", "async-subprocess", "executor-bridge", "shared-memory")


def capture(baseline: Path, candidate: Path) -> dict[str, object]:
    baseline_commit = git(candidate, "rev-parse", f"{BASELINE_TAG}^{{commit}}")
    if revision(baseline) != baseline_commit or git(baseline, "status", "--porcelain"):
        raise ValueError(f"baseline must be a clean checkout of {BASELINE_TAG}")
    binaries = {
        f"{side}_{name}": root / "target/release/examples" / f"benchmark_phase{name}"
        for side, root in (("baseline", baseline), ("candidate", candidate))
        for name in ("09", "10")
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
        "inherited_workloads": {
            workload: paired_workload(
                binaries[f"baseline_{name}"],
                binaries[f"candidate_{name}"],
                workload,
                fields,
            )
            for name, workloads, fields in (
                ("09", ("work", "review"), REVIEW_FIELDS),
                ("10", ("queue", "ipc", "guard"), PROTOCOL_FIELDS),
            )
            for workload in workloads
        },
        "new_protocol_workloads": {
            workload: summarize(
                [
                    measure_once(binaries["candidate_10"], workload, CALLERS)
                    for _ in range(PROCESS_REPEATS)
                ],
                PROTOCOL_FIELDS,
            )
            for workload in NEW_WORKLOADS
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
    result = capture(arguments.baseline.resolve(), arguments.candidate.resolve())
    destination = arguments.output
    destination.parent.mkdir(parents=True, exist_ok=True)
    _ = destination.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded Phase 0.11 performance evidence at {destination}")


if __name__ == "__main__":
    main()

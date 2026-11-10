"""Measure seven fresh CLI processes per adoption workload with fixed oracles.

Usage from the repository: python reproduce.py BINARY RESULTS_JSON
Requires an optimized candidate binary. No target Python code is executed.
Peak RSS is cumulative across child processes, not a per-workload measurement.
"""

import hashlib
import json
from pathlib import Path
import platform
import resource
import re
import statistics
import subprocess
import sys
import tempfile
import time


def manifest(root):
    return {
        str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    destination = Path(sys.argv[2]).resolve()
    if sys.platform not in {"darwin", "linux"}:
        raise SystemExit("RSS reporting requires macOS or Linux")
    records = []
    with tempfile.TemporaryDirectory(prefix="codepact-adoption-bench-") as temporary:
        base = Path(temporary)
        mustuse = (
            "from codepact import must_use\n@must_use\ndef fetch(): return 1\n"
            + "".join(f"def run_{i}():\n    fetch()\n" for i in range(250))
        )

        def scenario(
            name, files, flags, exit_code, total=None, matched=None, returned=None
        ):
            root = base / name
            root.mkdir()
            for relative, contents in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents)
            before = manifest(root)
            command = [str(binary), "check", *flags]
            timings, hashes, sizes = [], [], []
            for _ in range(7):
                start = time.perf_counter_ns()
                result = subprocess.run(
                    command, cwd=root, capture_output=True, check=False
                )
                timings.append((time.perf_counter_ns() - start) / 1_000_000)
                assert result.returncode == exit_code, (
                    name,
                    result.stdout,
                    result.stderr,
                )
                if "concise" in flags:
                    assert (
                        result.stderr
                        == b"250 diagnostics; 250 actionable; 0 suppressed; 0 rule-disabled; 250 returned\n"
                    ), (name, result.stderr)
                else:
                    assert not result.stderr, (name, result.stderr)
                hashes.append(hashlib.sha256(result.stdout).hexdigest())
                sizes.append(len(result.stdout))
                if "--json" in flags:
                    report = json.loads(result.stdout)
                    if exit_code == 2:
                        assert report["execution_status"] == "failed", (name, report)
                        assert "diagnostics" not in report, (name, report)
                    else:
                        assert report["total_diagnostics"] == total, (name, report)
                        assert (
                            report["coverage"]["status"]
                            == "complete-for-supported-subset"
                        ), name
                        assert report["exit_code"] == exit_code, name
                        if matched is not None:
                            assert report["baseline"]["matched"] == matched, name
                        if returned is not None:
                            assert report["returned_diagnostics"] == returned, name
            assert len(set(hashes)) == 1, (name, "nondeterministic output")
            assert manifest(root) == before, (name, "target files changed")
            memory = subprocess.run(
                ["/usr/bin/time", "-l", *command],
                cwd=root,
                capture_output=True,
                check=False,
            )
            assert memory.returncode == exit_code, (name, memory.stderr)
            peak = re.search(rb"(\d+)\s+maximum resident set size", memory.stderr)
            assert peak is not None, (name, memory.stderr)
            assert manifest(root) == before, (
                name,
                "memory measurement changed target files",
            )
            records.append(
                {
                    "name": name,
                    "peak_rss_bytes": int(peak[1]),
                    "command": command,
                    "expected_exit": exit_code,
                    "expected_total": total,
                    "expected_matched": matched,
                    "expected_returned": returned,
                    "samples_ms": timings,
                    "median_ms": statistics.median(timings),
                    "min_ms": min(timings),
                    "max_ms": max(timings),
                    "output_sha256": hashes[0],
                    "output_bytes": sizes[0],
                    "input_sha256": before,
                }
            )

        preparation = base / "baseline-preparation"
        preparation.mkdir()
        (preparation / "main.py").write_text(mustuse)
        for filename, rules in [("dense.json", "TP3001"), ("empty.json", "TP1001")]:
            result = subprocess.run(
                [
                    str(binary),
                    "baseline",
                    "update",
                    "--no-config",
                    "--rules",
                    rules,
                    "--output",
                    filename,
                    "--reason",
                    "benchmark fixture",
                ],
                cwd=preparation,
                capture_output=True,
            )
            assert result.returncode == 0, result.stderr
        dense = (preparation / "dense.json").read_text()
        empty = (preparation / "empty.json").read_text()
        json_flags = ["--no-config", "--json", "--fail-on", "warning"]
        scenario("mustuse-json", {"main.py": mustuse}, json_flags, 1, 250, returned=250)
        scenario(
            "mustuse-limited",
            {"main.py": mustuse},
            [*json_flags, "--max-diagnostics", "1"],
            1,
            250,
            returned=1,
        )
        for style in ["text", "concise"]:
            scenario(
                f"mustuse-{style}",
                {"main.py": mustuse},
                ["--no-config", "--output-format", style, "--fail-on", "warning"],
                1,
            )
        for name, source, baseline, expected, matched in [
            ("baseline-dense", mustuse, dense, 250, 250),
            ("baseline-empty", mustuse, empty, 250, 0),
            (
                "baseline-changed",
                mustuse + "def new_site():\n    fetch()\n",
                dense,
                251,
                250,
            ),
            (
                "baseline-ambiguous",
                mustuse.replace(
                    "def run_0():\n    fetch()\n",
                    "def run_0():\n    fetch()\n    fetch()\n",
                ),
                dense,
                251,
                249,
            ),
        ]:
            scenario(
                name,
                {"main.py": source, "baseline.json": baseline},
                [*json_flags, "--baseline", "baseline.json"],
                0 if matched == expected else 1,
                expected,
                matched,
            )
        config = "[tool.codepact]\nconfig-version=1\n"
        for size in [len(config), 65536]:
            scenario(
                f"config-{size}",
                {
                    "main.py": "pass\n",
                    "pyproject.toml": config + "#" * (size - len(config)),
                },
                ["--json"],
                0,
                0,
            )
        for count in [10, 10000]:
            scenario(
                f"discovery-{count}",
                {f"f{i:05}.py": "pass\n" for i in range(count)},
                ["--no-config", "--json"],
                0,
                0,
            )
        for size in [1000000, 1000001]:
            scenario(
                f"source-bytes-{size}",
                {"main.py": "#" * size},
                ["--no-config", "--json"],
                0 if size == 1000000 else 2,
                0,
            )
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    destination.write_text(
        json.dumps(
            {
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "source_sha256": {
                    str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted(Path("crates/codepact/src").glob("*.rs"))
                },
                "runner_sha256": hashlib.sha256(
                    Path(__file__).read_bytes()
                ).hexdigest(),
                "python": platform.python_version(),
                "host": platform.platform(),
                "repeats": 7,
                "peak_child_rss_bytes": rss if sys.platform == "darwin" else rss * 1024,
                "method": "Fresh CLI processes; launch and output capture included; OS caches not flushed. RSS is cumulative across children, not per case or retained-session memory. No in-process query reuse claim.",
                "scenarios": records,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        f"Passed {len(records)} workloads, seven samples each; retained {destination}"
    )


if __name__ == "__main__":
    main()

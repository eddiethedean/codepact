"""Measure cold CLI and helper-import baselines on macOS or Linux.

Build the release binary first, then run with uv run --locked python
scripts/benchmark_inspector.py. Timings include subprocess startup; peak child
RSS is the maximum across the entire run, not a per-scenario allocation count.
"""

import argparse
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time


class Arguments(argparse.Namespace):
    binary: Path = Path("target/release/typepact")
    repeats: int = 7
    classes: int = 1000


def measure(command: list[str], directory: Path, repeats: int) -> list[float]:
    durations: list[float] = []
    for _ in range(repeats):
        start = time.perf_counter()
        _ = subprocess.run(command, cwd=directory, capture_output=True, check=True)
        durations.append((time.perf_counter() - start) * 1000)
    return durations


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--binary", type=Path, default=Arguments.binary)
    _ = parser.add_argument("--repeats", type=int, default=Arguments.repeats)
    _ = parser.add_argument("--classes", type=int, default=Arguments.classes)
    args = Arguments()
    _ = parser.parse_args(namespace=args)
    if args.repeats < 3 or not 1 <= args.classes <= 2000:
        parser.error("use at least 3 repeats and between 1 and 2000 classes")
    if sys.platform not in {"darwin", "linux"}:
        parser.error("RSS reporting is qualified only on macOS and Linux")
    binary = args.binary.resolve(strict=True)
    prefix = "from typepact import resource\nfrom typepact.rust import Borrowed\n"
    source = prefix + "".join(
        f"@resource(closed_by='close')\nclass C{index}: pass\n"
        f"def use{index}(value: Borrowed[C{index}]) -> None: pass\n"
        for index in range(args.classes)
    )
    small = prefix + (
        "@resource(closed_by='close')\nclass Connection: pass\n"
        "def use(value: Borrowed[Connection]) -> None: pass\n"
    )
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        (directory / "small.py").write_text(small, encoding="utf-8")
        (directory / "synthetic.py").write_text(source, encoding="utf-8")
        cases = [
            ("Small CLI", [str(binary), "inspect-contracts", "small.py"]),
            ("Synthetic CLI", [str(binary), "inspect-contracts", "synthetic.py"]),
            ("Helper import", [sys.executable, "-I", "-c", "import typepact"]),
        ]
        print("# Cold-process baseline (corpus v1)")
        print(f"\nHost: {platform.platform()}, {platform.machine()}")
        print(f"\nPython: {sys.version.split()[0]}; repeats: {args.repeats}")
        print(f"\nBinary: {binary}")
        print(f"\nSmall source: {len(small.encode())} bytes; 2 contracts")
        print(
            f"\nSynthetic source: {len(source.encode())} bytes; "
            f"{args.classes} classes; {2 * args.classes} contracts"
        )
        print("\n| Scenario | Median ms | Min ms | Max ms |")
        print("| --- | ---: | ---: | ---: |")
        for name, command in cases:
            values = measure(command, directory, args.repeats)
            print(
                f"| {name} | {statistics.median(values):.3f} | "
                f"{min(values):.3f} | {max(values):.3f} |"
            )
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    peak_bytes = rss if sys.platform == "darwin" else rss * 1024
    print(f"\nMaximum child RSS across all scenarios: {peak_bytes / 1048576:.2f} MiB")
    print(
        "\nIncludes process startup and output capture; OS file caches are not flushed."
    )
    print(
        "Each CLI sample creates a fresh session; this is not a warm-query benchmark."
    )


if __name__ == "__main__":
    main()

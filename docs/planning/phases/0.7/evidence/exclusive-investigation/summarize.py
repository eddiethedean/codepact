"""Summarize balanced blocks. Resample blocks, keeping the seven trials together."""

import json
import random
from collections import Counter
from itertools import permutations
from pathlib import Path
from statistics import median


def summarize(
    samples: list[dict[str, list[float]]], reference: str, challenger: str
) -> dict[str, object]:
    baseline = [median(s[reference]) for s in samples]
    candidate = [median(s[challenger]) for s in samples]
    delta = [b - a for a, b in zip(baseline, candidate)]
    ratios = [(b / a - 1) * 100 for a, b in zip(baseline, candidate)]
    rng = random.Random(701)
    boots = sorted(median(rng.choices(ratios, k=len(ratios))) for _ in range(10000))
    return dict(
        baseline_us=median(baseline),
        candidate_us=median(candidate),
        paired_delta_us=median(delta),
        paired_percent=median(ratios),
        boot95_percent=[boots[249], boots[9749]],
        pooled_percent=100
        * (
            median(x for s in samples for x in s[challenger])
            / median(x for s in samples for x in s[reference])
            - 1
        ),
        pairs=len(samples),
        slower=sum(d > 0 for d in delta),
    )


def upper_median(values: list[float]) -> float:
    """Match the original capture program's choice for even-sized samples."""
    return sorted(values)[len(values) // 2]


def report(root: Path) -> dict[str, object]:
    def read_blocks(
        name: str, arms: tuple[str, ...], repeats: int
    ) -> list[dict[str, list]]:
        blocks = json.loads((root / (name + ".json")).read_text())
        expected = {order: repeats for order in permutations(arms)}
        if Counter(tuple(block) for block in blocks) != expected:
            raise ValueError(f"{name} has incomplete or unbalanced process blocks")
        if any(len(trials) != 7 for block in blocks for trials in block.values()):
            raise ValueError(f"{name} requires seven sessions per process")
        return blocks

    stocks = {
        "stock": read_blocks("stock", ("baseline", "candidate"), 8),
        "stock-repeat": read_blocks("stock-repeat", ("baseline", "candidate"), 12),
    }
    stocks["combined"] = stocks["stock"] + stocks["stock-repeat"]
    results = {}
    for name, blocks in stocks.items():
        samples = [
            {arm: [ms * 1000 for ms in values] for arm, values in block.items()}
            for block in blocks
        ]
        result = summarize(samples, "baseline", "candidate")
        result["first_arm_median_paired_percent"] = {
            arm: summarize(
                [s for s in samples if next(iter(s)) == arm], "baseline", "candidate"
            )["paired_percent"]
            for arm in ("baseline", "candidate")
        }
        upper = {
            arm: upper_median([x for sample in samples for x in sample[arm]])
            for arm in ("baseline", "candidate")
        }
        result["upper_median_us"] = upper
        result["upper_median_percent"] = 100 * (
            upper["candidate"] / upper["baseline"] - 1
        )
        results[name] = result

    blocks = read_blocks("probe", ("baseline", "candidate", "bypass"), 3)
    for name, metric in [
        ("first", lambda r: sum(r["first_ns"])),
        ("hot", lambda r: r["hot_ns"]),
    ]:
        samples = [
            {k: [metric(r) / 1000 for r in vs] for k, vs in block.items()}
            for block in blocks
        ]
        results[name] = summarize(samples, "baseline", "candidate")
        results[name + "_bypass"] = summarize(samples, "candidate", "bypass")
    results["first_stages_us"] = {
        arm: [
            median(
                median(r["first_ns"][i] / 1000 for r in block[arm]) for block in blocks
            )
            for i in range(6)
        ]
        for arm in blocks[0]
    }
    groups = json.loads((root / "grouped.json").read_text())
    if [tuple(group) for group in groups] != [
        ("baseline", "candidate"),
        ("candidate", "baseline"),
    ]:
        raise ValueError("grouped capture requires both run orders")
    for group in groups:
        if any(
            len(runs) != 10 or any(len(times) != 7 for times in runs)
            for runs in group.values()
        ):
            raise ValueError("expected ten processes and seven sessions per process")
        upper = {
            arm: upper_median([ms * 1000 for run in runs for ms in run])
            for arm, runs in group.items()
        }
        results["grouped_" + next(iter(group)) + "_first"] = {
            "upper_median_us": upper,
            "upper_median_percent": 100 * (upper["candidate"] / upper["baseline"] - 1),
        }
    return results


if __name__ == "__main__":
    print(json.dumps(report(Path(__file__).resolve().parent), indent=2))

from pathlib import Path
import itertools
import json
import os
import random
import subprocess
import sys

root = Path(__file__).resolve().parent


def run(
    experiment: str, arms: list[str], blocks: int, *, seed: int = 700, suffix: str = ""
) -> None:
    rng = random.Random(seed)
    orders = list(itertools.permutations(arms)) * blocks
    rng.shuffle(orders)
    results = []
    for order in orders:
        row = {}
        for arm in order:
            side = "candidate" if arm == "bypass" else arm
            binary = root / (side + "-" + experiment)
            env = dict(os.environ)
            env.pop("G4_SKIP_FILTER", None)
            if arm == "bypass":
                env["G4_SKIP_FILTER"] = "1"
            data = json.loads(
                subprocess.check_output([binary, "exclusive", "250"], env=env)
            )
            row[arm] = data["warm_ms"] if experiment == "stock" else data
        results.append(row)
        (root / (experiment + suffix + ".json")).write_text(json.dumps(results) + "\n")


def run_grouped() -> None:
    wrapper = root / "candidate/scripts/capture_phase07_performance.py"
    blocks = []
    for order in (("baseline", "candidate"), ("candidate", "baseline")):
        block = {}
        for arm in order:
            command = [
                sys.executable,
                str(wrapper),
                "--measure-child",
                "--",
                str(root / (arm + "-stock")),
                "exclusive",
                "250",
            ]
            trials = []
            for _ in range(10):
                child = json.loads(subprocess.check_output(command))
                if child["return_code"]:
                    raise RuntimeError(child["stderr"])
                trials.append(json.loads(child["stdout"])["warm_ms"])
            block[arm] = trials
        blocks.append(block)
        (root / "grouped.json").write_text(json.dumps(blocks) + "\n")


if __name__ == "__main__":
    run("stock", ["baseline", "candidate"], 8)
    run("probe", ["baseline", "candidate", "bypass"], 3)
    run("stock", ["baseline", "candidate"], 12, seed=702, suffix="-repeat")
    run_grouped()

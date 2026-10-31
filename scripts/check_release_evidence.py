"""Require completed release-gate evidence before a version tag can publish."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path
from typing import cast


def section(text: str, title: str) -> str:
    heading = re.search(rf"^## {re.escape(title)}\s*$", text, re.M)
    if heading is None:
        return ""
    start = text.find("\n", heading.start())
    if start < 0:
        return ""
    end = re.search(r"\n## ", text[start:])
    return text[start : start + end.start()] if end else text[start:]


def table_rows(text: str, title: str) -> dict[str, tuple[str, str]]:
    rows: dict[str, tuple[str, str]] = {}
    for line in section(text, title).splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 3 or set(cells[0]) <= {"-", ":"}:
            continue
        gate = re.match(r"([A-Z]\d{1,2})\b", cells[0])
        if gate:
            gate_id = gate.group(1)
            rows[gate_id] = (
                ("Duplicate", "") if gate_id in rows else (cells[1], cells[2])
            )
    return rows


def is_pass(status: str) -> bool:
    return status.lower().split(maxsplit=1)[0] == "pass" if status else False


def required_phase_gates(root: Path, version: str) -> list[str]:
    match = re.fullmatch(r"0\.(\d+)\.(\d+)", version)
    if not match:
        return []
    minor = match.group(1)
    plans = sorted((root / "docs/planning/phases").glob(f"0.{minor}-*.md"))
    if len(plans) != 1:
        return []
    gate_pattern = re.compile(r"^\s*-\s*\[[ xX]\]\s*\*\*([A-Z]\d{2})\s+[—-]", re.M)
    return gate_pattern.findall(plans[0].read_text(encoding="utf-8"))


def release_evidence_errors(
    root: Path, version: str, tag_commit: str | None = None
) -> list[str]:
    evidence_path = root / "docs/releases" / f"{version}-evidence.md"
    if not evidence_path.is_file():
        return [f"missing release evidence record: {evidence_path.relative_to(root)}"]

    text = evidence_path.read_text(encoding="utf-8")
    errors: list[str] = []
    candidate = section(text, "Candidate") or section(text, "Candidate and scope")
    if version not in candidate:
        errors.append(f"candidate section does not identify version {version}")
    candidate_match = re.search(
        r"^\s*-\s*(?:Candidate commit|Implementation candidate):\s*"
        r"`([0-9a-f]{40})`\.?\s*$",
        candidate,
        re.M,
    )
    if candidate_match is None:
        errors.append("candidate section must identify the exact 40-character commit")
    elif tag_commit is not None:
        candidate_commit = candidate_match.group(1)
        ancestor = subprocess.run(
            ["git", "merge-base", "--is-ancestor", candidate_commit, tag_commit],
            cwd=root,
            check=False,
            capture_output=True,
            text=True,
        )
        if ancestor.returncode != 0:
            errors.append("candidate commit is not an ancestor of the release tag")
        else:
            evidence_relative = evidence_path.relative_to(root).as_posix()
            changed = subprocess.run(
                [
                    "git",
                    "diff",
                    "--name-only",
                    candidate_commit,
                    tag_commit,
                    "--",
                    ".",
                    f":!{evidence_relative}",
                ],
                cwd=root,
                check=False,
                capture_output=True,
                text=True,
            )
            if changed.returncode != 0:
                errors.append("could not compare the candidate and tag source trees")
            elif changed.stdout.strip():
                errors.append(
                    "tag changes files beyond its release evidence record "
                    "since the candidate"
                )

    common = table_rows(text, "Common gates")
    for gate in (f"G{number}" for number in range(1, 8)):
        status, evidence = common.get(gate, ("Missing", ""))
        if not is_pass(status):
            errors.append(f"{gate} status is {status}, expected Pass")
        if not evidence:
            errors.append(f"{gate} has no evidence")
    g7_evidence = common.get("G7", ("", ""))[1].lower()
    if "maintainer explicitly approved" not in g7_evidence:
        errors.append("G7 evidence must record that the maintainer explicitly approved")

    required = required_phase_gates(root, version)
    if not required:
        errors.append(f"could not determine phase gates for version {version}")
    phase = table_rows(text, "Phase-specific gates")
    if not phase:
        phase = table_rows(text, "Phase gates")
    for gate in required:
        status, evidence = phase.get(gate, ("Missing", ""))
        if not is_pass(status):
            errors.append(f"{gate} status is {status}, expected Pass")
        if not evidence:
            errors.append(f"{gate} has no evidence")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="stable X.Y.Z version from the release tag")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--tag-commit", help="release tag commit to compare with the candidate"
    )
    args = parser.parse_args()
    version = cast(str, args.version)
    root = cast(Path, args.root)
    tag_commit = cast(str | None, args.tag_commit)
    errors = release_evidence_errors(root, version, tag_commit)
    if errors:
        for error in errors:
            print(f"release evidence check failed: {error}")
        return 1
    print(f"Release evidence for {version} satisfies the pre-publication gates")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

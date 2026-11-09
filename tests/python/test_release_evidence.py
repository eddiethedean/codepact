from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from check_release_evidence import (
    release_evidence_errors,
    required_phase_gates,
    table_rows,
)


VERSION = "0.2.0"
COMMIT = "a" * 40


def write_candidate(
    root: Path, *, pending: str | None = None, commit: str = COMMIT
) -> None:
    (root / "docs/planning/phases").mkdir(parents=True, exist_ok=True)
    (root / "docs/releases").mkdir(parents=True, exist_ok=True)
    phase_gates = "\n".join(
        f"- [ ] **L{number:02} — Test gate.**" for number in range(1, 7)
    )
    (root / "docs/planning/phases/0.2-lifecycle.md").write_text(phase_gates)
    common = "\n".join(
        f"| G{number} | {'Pending' if pending == f'G{number}' else 'Pass'} | "
        "Evidence recorded. | `eddiethedean` / 2026-11-08 |"
        for number in range(1, 8)
    )
    phase = "\n".join(
        f"| L{number:02} | {'Pending' if pending == f'L{number:02}' else 'Pass'} | "
        "Fixture and result. | `eddiethedean` / 2026-11-08 |"
        for number in range(1, 7)
    )
    record = (
        f"# {VERSION} release evidence\n\n"
        "## Candidate\n\n"
        f"- Version and phase: {VERSION}\n"
        f"- Candidate commit: `{commit}`\n\n"
        "- Accountable maintainer, author, validator, reviewer and release approver: "
        "`eddiethedean`.\n\n"
        "## Common gates\n\n"
        "| Gate | Status | Evidence | Reviewer / date |\n"
        "| --- | --- | --- | --- |\n"
        f"{common}\n\n"
        "## Phase-specific gates\n\n"
        "| Gate ID | Status | Evidence | Reviewer |\n"
        "| --- | --- | --- | --- |\n"
        f"{phase}\n"
    ).replace(
        "G7 | Pass | Evidence recorded.", "G7 | Pass | Maintainer explicitly approved."
    )
    (root / f"docs/releases/{VERSION}-evidence.md").write_text(record)


class ReleaseEvidenceTest(unittest.TestCase):
    def test_multi_letter_phase_gate_ids_are_required_and_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            phase_plan = root / "docs/planning/phases/0.4-adoption.md"
            phase_plan.parent.mkdir(parents=True)
            phase_plan.write_text("- [ ] **CS02 — Async reporting.**\n")
            self.assertEqual(required_phase_gates(root, "0.4.0"), ["CS02"])

        rows = table_rows(
            "## Phase-specific gates\n"
            "| Gate ID | Status | Evidence | Reviewer |\n"
            "| --- | --- | --- | --- |\n"
            "| CS02 — Async reporting | Pending | Evidence needed. | |\n",
            "Phase-specific gates",
        )
        self.assertEqual(rows["CS02"], ("Pending", "Evidence needed.", ""))

    def test_maintainer_can_validate_and_approve_every_gate_alone(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_candidate(root)
            self.assertEqual(release_evidence_errors(root, VERSION), [])

    def test_pending_gate_blocks_release(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_candidate(root, pending="L04")
            errors = release_evidence_errors(root, VERSION)
            self.assertIn("L04 status is Pending, expected Pass", errors)

    def test_passed_gate_requires_accountable_maintainer_signoff(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_candidate(root)
            evidence = root / f"docs/releases/{VERSION}-evidence.md"
            text = evidence.read_text(encoding="utf-8")
            text = text.replace(
                "| L01 | Pass | Fixture and result. | `eddiethedean` / 2026-11-08 |",
                "| L01 | Pass | Fixture and result. | "
                "`another-reviewer` / 2026-11-08 |",
            )
            evidence.write_text(text, encoding="utf-8")
            errors = release_evidence_errors(root, VERSION)
            self.assertIn(
                "L01 Pass requires the accountable maintainer in its signoff", errors
            )

    def test_passed_gate_requires_a_valid_signoff_date(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_candidate(root)
            evidence = root / f"docs/releases/{VERSION}-evidence.md"
            text = evidence.read_text(encoding="utf-8")
            text = text.replace(
                "| G1 | Pass | Evidence recorded. | `eddiethedean` / 2026-11-08 |",
                "| G1 | Pass | Evidence recorded. | `eddiethedean` / Pending |",
            )
            evidence.write_text(text, encoding="utf-8")
            errors = release_evidence_errors(root, VERSION)
            self.assertIn("G1 Pass requires an ISO date in its signoff", errors)

    def test_passed_gate_rejects_an_invalid_signoff_date(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_candidate(root)
            evidence = root / f"docs/releases/{VERSION}-evidence.md"
            text = evidence.read_text(encoding="utf-8")
            text = text.replace(
                "| G1 | Pass | Evidence recorded. | `eddiethedean` / 2026-11-08 |",
                "| G1 | Pass | Evidence recorded. | `eddiethedean` / 2026-99-99 |",
            )
            evidence.write_text(text, encoding="utf-8")
            errors = release_evidence_errors(root, VERSION)
            self.assertIn("G1 signoff has an invalid ISO date", errors)

    def test_missing_evidence_record_blocks_release(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            errors = release_evidence_errors(Path(directory), VERSION)
            self.assertEqual(
                errors,
                ["missing release evidence record: docs/releases/0.2.0-evidence.md"],
            )

    def test_tag_may_add_only_the_qualification_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs/planning/phases").mkdir(parents=True)
            (root / "source.rs").write_text("candidate source\n")
            (root / "docs/planning/phases/0.2-lifecycle.md").write_text(
                "\n".join(
                    f"- [ ] **L{number:02} — Test gate.**" for number in range(1, 7)
                )
            )
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "config", "user.name", "Release test"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "config", "user.email", "release-test@example.invalid"],
                cwd=root,
                check=True,
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "candidate"], cwd=root, check=True)
            candidate = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=root, text=True
            ).strip()
            write_candidate(root, commit=candidate)
            subprocess.run(
                ["git", "add", "docs/releases/0.2.0-evidence.md"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "commit", "-qm", "record release qualification"],
                cwd=root,
                check=True,
            )
            tag_commit = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=root, text=True
            ).strip()
            self.assertEqual(
                release_evidence_errors(root, VERSION, tag_commit=tag_commit), []
            )

    def test_tag_rejects_source_changes_after_the_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs/planning/phases").mkdir(parents=True)
            (root / "source.rs").write_text("candidate source\n")
            (root / "docs/planning/phases/0.2-lifecycle.md").write_text(
                "\n".join(
                    f"- [ ] **L{number:02} — Test gate.**" for number in range(1, 7)
                )
            )
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "config", "user.name", "Release test"], cwd=root, check=True
            )
            subprocess.run(
                ["git", "config", "user.email", "release-test@example.invalid"],
                cwd=root,
                check=True,
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "candidate"], cwd=root, check=True)
            candidate = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=root, text=True
            ).strip()
            (root / "source.rs").write_text("changed after qualification\n")
            write_candidate(root, commit=candidate)
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(
                ["git", "commit", "-qm", "change source and record qualification"],
                cwd=root,
                check=True,
            )
            tag_commit = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=root, text=True
            ).strip()
            self.assertIn(
                "tag changes files beyond its release evidence record since the "
                "candidate",
                release_evidence_errors(root, VERSION, tag_commit=tag_commit),
            )


if __name__ == "__main__":
    unittest.main()

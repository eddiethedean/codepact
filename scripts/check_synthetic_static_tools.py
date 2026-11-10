"""Confirm synthetic bug/repair examples remain ordinary Python-static-clean."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path


def project_variants(corpus: Path) -> list[Path]:
    variants: list[Path] = []
    for project in sorted(corpus.iterdir()):
        if not project.is_dir():
            continue
        if (project / "buggy").is_dir():
            variants.extend((project / "buggy", project / "fixed"))
        elif (project / "boundary").is_dir():
            variants.append(project / "boundary")
    return variants


def main() -> int:
    repository = Path(__file__).resolve().parents[1]
    corpus = repository / "crates/codepact/tests/projects"
    pyright = shutil.which("pyright")
    if pyright is None:
        print("pyright is required to validate the synthetic project controls")
        return 2

    variants = project_variants(corpus)
    failures = 0
    for project in variants:
        config = project / "pyrightconfig.json"
        if config.exists():
            print(f"refusing to overwrite existing Pyright config: {config}")
            return 2

        config.write_text(
            json.dumps(
                {
                    "include": ["src"],
                    "extraPaths": ["src", str(repository / "python")],
                    "pythonVersion": "3.11",
                    "typeCheckingMode": "strict",
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        try:
            result = subprocess.run(
                [pyright, "--project", str(config)],
                cwd=project,
                text=True,
                capture_output=True,
                check=False,
            )
        finally:
            config.unlink(missing_ok=True)

        if result.returncode:
            failures += 1
            print(f"Pyright failed for {project.relative_to(corpus)}:")
            print(result.stdout, end="")
            print(result.stderr, end="")

    if failures:
        print(
            f"{failures} of {len(variants)} synthetic project variants "
            "failed strict Pyright"
        )
        return 1
    print(f"Strict Pyright passed for all {len(variants)} synthetic project variants")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

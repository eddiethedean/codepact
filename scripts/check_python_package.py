"""Verify the pure helper and native CLI wheels and source distributions."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile
import site


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        environment = os.environ.copy()
        environment["MATURIN_PEP517_ARGS"] = "--locked"
        environment["PYTHONPATH"] = os.pathsep.join(
            [*site.getsitepackages(), environment.get("PYTHONPATH", "")]
        ).rstrip(os.pathsep)
        for project in (root / "cli", root):
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "build",
                    "--no-isolation",
                    "--outdir",
                    str(work / "dist"),
                    str(project),
                ],
                env=environment,
                check=True,
            )
        artifacts = sorted((work / "dist").iterdir())
        assert len(artifacts) == 4
        for artifact in artifacts:
            if artifact.suffix == ".whl":
                cli = artifact.name.startswith("codepact_cli-")
                assert artifact.name.endswith("none-any.whl") != cli
                with zipfile.ZipFile(artifact) as archive:
                    names = archive.namelist()
                    assert (
                        any(".data/scripts/codepact" in name for name in names) == cli
                    )
                    assert ("codepact/py.typed" in names) != cli
        subprocess.run(
            [sys.executable, "-m", "twine", "check", "--strict", *map(str, artifacts)],
            check=True,
        )
        for index, artifact in enumerate(artifacts):
            venv = work / f"install-{index}"
            subprocess.run(
                ["uv", "venv", "--python", sys.executable, str(venv)], check=True
            )
            interpreter = venv / (
                "Scripts/python.exe" if os.name == "nt" else "bin/python"
            )
            cli = artifact.name.startswith("codepact_cli-")
            options = (
                ["--no-deps"]
                if cli
                else ["--no-index", "--find-links", str(work / "dist")]
            )
            if artifact.name.endswith(".tar.gz"):
                options.append("--no-build-isolation")
            subprocess.run(
                [
                    "uv",
                    "pip",
                    "install",
                    "--python",
                    str(interpreter),
                    *options,
                    str(artifact),
                ],
                cwd=work,
                env=environment,
                check=True,
            )
            arguments = ["--cli-only"] if cli else []
            subprocess.run(
                [
                    str(interpreter),
                    "-I",
                    str(root / "scripts/verify_installed_package.py"),
                    *arguments,
                ],
                cwd=work,
                check=True,
            )


if __name__ == "__main__":
    main()

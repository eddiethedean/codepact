"""Verify the pure helper and native CLI wheels and source distributions."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import zipfile
import site
from typing import cast


def locked_package_version(root: Path, name: str) -> str:
    lock = cast(dict[str, object], tomllib.loads((root / "uv.lock").read_text()))
    packages = lock.get("package")
    if not isinstance(packages, list):
        raise RuntimeError("uv.lock does not contain a package list")
    for package in cast(list[object], packages):
        if isinstance(package, dict):
            entry = cast(dict[str, object], package)
            version = entry.get("version")
            if entry.get("name") == name and isinstance(version, str):
                return version
    raise RuntimeError(f"uv.lock has no locked version for {name}")


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
        artifacts = sorted(
            artifact
            for artifact in (work / "dist").iterdir()
            if artifact.name.startswith(("codepact-", "codepact_cli-"))
        )
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
        typing_extensions_version = locked_package_version(root, "typing-extensions")
        python_executable = Path(sys.base_prefix) / (
            "python.exe"
            if os.name == "nt"
            else f"bin/python{sys.version_info.major}.{sys.version_info.minor}"
        )
        wheelhouse = work / "wheelhouse"
        wheelhouse.mkdir()
        subprocess.run(
            [
                str(python_executable),
                "-m",
                "pip",
                "download",
                "--no-deps",
                "--only-binary=:all:",
                "--dest",
                str(wheelhouse),
                f"typing-extensions=={typing_extensions_version}",
            ],
            env=environment,
            check=True,
        )
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
                else [
                    "--no-index",
                    "--find-links",
                    str(work / "dist"),
                    "--find-links",
                    str(wheelhouse),
                ]
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

"""Verify the pure helper and native CLI wheels and source distributions."""

import os
from pathlib import Path
import shutil
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
                    if not cli:
                        for module in (
                            "__init__.py",
                            "contracts.py",
                            "outcomes.py",
                            "rust.py",
                            "py.typed",
                        ):
                            assert f"codepact/{module}" in names
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
            arguments = ["--cli-only"] if cli else ["--helper-only"]
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

        extra_environment = work / "install-cli-extra"
        subprocess.run(
            ["uv", "venv", "--python", sys.executable, str(extra_environment)],
            check=True,
        )
        extra_interpreter = extra_environment / (
            "Scripts/python.exe" if os.name == "nt" else "bin/python"
        )
        helper_version = locked_package_version(root, "codepact")
        subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--python",
                str(extra_interpreter),
                "--no-index",
                "--find-links",
                str(work / "dist"),
                "--find-links",
                str(wheelhouse),
                f"codepact[cli]=={helper_version}",
            ],
            cwd=work,
            env=environment,
            check=True,
        )
        subprocess.run(
            [
                str(extra_interpreter),
                "-I",
                str(root / "scripts/verify_installed_package.py"),
            ],
            cwd=work,
            check=True,
        )

        source_artifacts = work / "cli-extra-sources"
        source_artifacts.mkdir()
        for artifact in artifacts:
            if artifact.suffix == ".gz":
                shutil.copy2(artifact, source_artifacts / artifact.name)
        source_environment = work / "install-cli-extra-from-sdists"
        subprocess.run(
            ["uv", "venv", "--python", sys.executable, str(source_environment)],
            check=True,
        )
        source_interpreter = source_environment / (
            "Scripts/python.exe" if os.name == "nt" else "bin/python"
        )
        subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--python",
                str(source_interpreter),
                "--no-index",
                "--no-build-isolation",
                "--find-links",
                str(source_artifacts),
                "--find-links",
                str(wheelhouse),
                f"codepact[cli]=={helper_version}",
            ],
            cwd=work,
            env=environment,
            check=True,
        )
        subprocess.run(
            [
                str(source_interpreter),
                "-I",
                str(root / "scripts/verify_installed_package.py"),
            ],
            cwd=work,
            check=True,
        )


if __name__ == "__main__":
    main()

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


def verify_installed_outcome_typing(interpreter: Path, root: Path, work: Path) -> None:
    work.mkdir(parents=True)
    python_version = subprocess.run(
        [
            str(interpreter),
            "-c",
            "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')",
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    valid = work / "outcomes_valid.py"
    invalid = work / "outcomes_invalid.py"
    shutil.copy2(root / "tests/checkers/outcomes_valid.py", valid)
    shutil.copy2(root / "tests/checkers/outcomes_invalid.py", invalid)

    environment = os.environ.copy()
    mypy = [
        "mypy",
        "--strict",
        "--python-executable",
        str(interpreter),
        "--python-version",
        python_version,
    ]
    subprocess.run([*mypy, str(valid)], cwd=work, env=environment, check=True)
    mypy_invalid = subprocess.run(
        [*mypy, str(invalid)],
        cwd=work,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    if mypy_invalid.returncode == 0 or "Incompatible types in assignment" not in (
        mypy_invalid.stdout + mypy_invalid.stderr
    ):
        raise RuntimeError("Mypy did not reject the installed outcome typing controls")

    pyright = [
        "pyright",
        "--pythonpath",
        str(interpreter),
        "--pythonversion",
        python_version,
    ]
    subprocess.run([*pyright, str(valid)], cwd=work, env=environment, check=True)
    pyright_invalid = subprocess.run(
        [*pyright, str(invalid)],
        cwd=work,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    if pyright_invalid.returncode == 0 or "reportAssignmentType" not in (
        pyright_invalid.stdout + pyright_invalid.stderr
    ):
        raise RuntimeError(
            "Pyright did not reject the installed outcome typing controls"
        )
    basedpyright = [
        "basedpyright",
        "--project",
        str(root / "pyproject.toml"),
        "--pythonpath",
        str(interpreter),
        "--pythonversion",
        python_version,
    ]
    subprocess.run([*basedpyright, str(valid)], cwd=work, env=environment, check=True)
    basedpyright_invalid = subprocess.run(
        [*basedpyright, str(invalid)],
        cwd=work,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    if basedpyright_invalid.returncode == 0 or "reportAssignmentType" not in (
        basedpyright_invalid.stdout + basedpyright_invalid.stderr
    ):
        raise RuntimeError(
            "Basedpyright did not reject the installed outcome typing controls"
        )
    print(
        "Installed outcome typing passed Mypy, Pyright and Basedpyright "
        f"on Python {python_version}"
    )


def verify_documented_upgrade(dist: Path, work: Path) -> None:
    """Replay documented upgrades from 0.5.1 and 0.6.0 to the 0.7 CLI extra."""
    for previous_version in ("0.5.1", "0.6.0"):
        project = work / f"upgrade-project-{previous_version}"
        project.mkdir()
        (project / "pyproject.toml").write_text(
            "[project]\n"
            'name = "codepact-upgrade-qualification"\n'
            'version = "0.1.0"\n'
            'requires-python = ">=3.11"\n'
            f'dependencies = ["codepact=={previous_version}"]\n'
            "\n[tool.uv]\n"
            "package = false\n",
            encoding="utf-8",
        )

        def uv(*arguments: str, capture: bool = False) -> str:
            result = subprocess.run(
                ["uv", *arguments],
                cwd=project,
                check=True,
                capture_output=capture,
                text=True,
            )
            return result.stdout.strip() if capture else ""

        uv("sync")
        old_command = uv("run", "--locked", "codepact", "--version", capture=True)
        expected_old_command = f"codepact {previous_version}"
        if old_command != expected_old_command:
            raise RuntimeError(
                f"the pre-upgrade command has unexpected version: {old_command}"
            )

        uv(
            "add",
            "codepact[cli]==0.7.0",
            "--find-links",
            str(dist),
        )
        uv("sync", "--locked")
        version = uv("run", "--locked", "codepact", "--version", capture=True)
        if version != "codepact 0.7.0":
            raise RuntimeError(
                f"the upgraded command has unexpected version: {version}"
            )
        uv(
            "run",
            "--locked",
            "python",
            "-c",
            "from codepact import Ok; from codepact.rust import Ok as RustOk; "
            "assert Ok is RustOk",
        )
        print(f"Documented {previous_version}-to-0.7.0 helper and CLI upgrade passed")


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
        verify_documented_upgrade(work / "dist", work)
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
            if not cli:
                subprocess.run(
                    [
                        str(interpreter),
                        "-I",
                        str(root / "tests/python/test_outcomes.py"),
                    ],
                    cwd=work,
                    check=True,
                )
                verify_installed_outcome_typing(
                    interpreter, root, work / f"typing-check-{index}"
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

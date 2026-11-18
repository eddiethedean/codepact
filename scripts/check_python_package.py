"""Verify the pure helper and native CLI wheels and source distributions."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import site
import zipfile

if __package__:
    from .locked_dependencies import locked_package_version
    from .qualify_inline_provider import verify_installed_marked_provider
else:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        from locked_dependencies import locked_package_version
        from qualify_inline_provider import verify_installed_marked_provider
    finally:
        _ = sys.path.pop(0)


def verify_installed_outcome_typing(interpreter: Path, root: Path, work: Path) -> None:
    work.mkdir(parents=True)
    for name in ("outcomes_valid.py", "outcomes_invalid.py"):
        shutil.copy2(root / "tests/checkers" / name, work / name)
    subprocess.run(
        [
            sys.executable,
            str(root / "scripts/check_typing_contracts.py"),
            "--fixtures",
            str(work),
            "--installed-python",
            str(interpreter),
            "--outcomes-only",
        ],
        cwd=work,
        check=True,
    )


def verify_documented_upgrade(dist: Path, work: Path, candidate_version: str) -> None:
    """Replay upgrades to the candidate's exact helper and CLI extra."""
    environment = os.environ.copy()
    # Each upgrade must use its disposable project's own environment.
    _ = environment.pop("UV_PROJECT_ENVIRONMENT", None)
    for previous_version in ("0.5.1", "0.6.0", "0.7.0"):
        project = work / f"upgrade-project-{previous_version}"
        project.mkdir()
        interpreter_package = (
            "codepact[cli]" if previous_version == "0.7.0" else "codepact"
        )
        (project / "pyproject.toml").write_text(
            "[project]\n"
            'name = "codepact-upgrade-qualification"\n'
            'version = "0.1.0"\n'
            'requires-python = ">=3.11"\n'
            f'dependencies = ["{interpreter_package}=={previous_version}"]\n'
            "\n[tool.uv]\n"
            "package = false\n",
            encoding="utf-8",
        )

        def uv(*arguments: str, capture: bool = False) -> str:
            result = subprocess.run(
                ["uv", *arguments],
                cwd=project,
                env=environment,
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
            f"codepact[cli]=={candidate_version}",
            "--find-links",
            str(dist),
        )
        uv("sync", "--locked")
        version = uv("run", "--locked", "codepact", "--version", capture=True)
        if version != f"codepact {candidate_version}":
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
        print(
            f"Documented {previous_version}-to-{candidate_version} helper "
            "and CLI upgrade passed"
        )


def build_distributions(
    root: Path, destination: Path, environment: dict[str, str]
) -> None:
    for project in (root / "cli", root):
        subprocess.run(
            [
                sys.executable,
                "-m",
                "build",
                "--no-isolation",
                "--outdir",
                str(destination),
                str(project),
            ],
            env=environment,
            check=True,
        )


def check_local_inventory(directory: Path) -> list[Path]:
    artifacts = sorted(
        artifact
        for artifact in directory.iterdir()
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
                assert any(".data/scripts/codepact" in name for name in names) == cli
                assert ("codepact/py.typed" in names) != cli
    return artifacts


def create_environment(directory: Path) -> Path:
    subprocess.run(
        ["uv", "venv", "--python", sys.executable, str(directory)], check=True
    )
    return directory / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def check_individual_installs(
    root: Path,
    work: Path,
    artifacts: list[Path],
    wheelhouse: Path,
    environment: dict[str, str],
) -> None:
    for index, artifact in enumerate(artifacts):
        interpreter = create_environment(work / f"install-{index}")
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


def check_extra_install(
    root: Path,
    work: Path,
    distributions: Path,
    wheelhouse: Path,
    version: str,
    environment: dict[str, str],
    *,
    from_sdists: bool = False,
) -> Path:
    name = "install-cli-extra-from-sdists" if from_sdists else "install-cli-extra"
    interpreter = create_environment(work / name)
    options = ["--no-build-isolation"] if from_sdists else []
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            str(interpreter),
            "--no-index",
            *options,
            "--find-links",
            str(distributions),
            "--find-links",
            str(wheelhouse),
            f"codepact[cli]=={version}",
        ],
        cwd=work,
        env=environment,
        check=True,
    )
    subprocess.run(
        [str(interpreter), "-I", str(root / "scripts/verify_installed_package.py")],
        cwd=work,
        check=True,
    )
    return interpreter


def check_provider_installs(root: Path, work: Path, interpreter: Path) -> None:
    scripts_directory = Path(
        subprocess.run(
            [
                str(interpreter),
                "-c",
                "import sysconfig; print(sysconfig.get_path('scripts'))",
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    )
    verify_installed_marked_provider(
        interpreter,
        scripts_directory / ("codepact.exe" if os.name == "nt" else "codepact"),
        work,
    )
    publication_checks = [
        sys.executable,
        str(root / "scripts/check_phase08_artifacts.py"),
        "--cli",
        str(scripts_directory / ("codepact.exe" if os.name == "nt" else "codepact")),
    ]
    if receipt := os.environ.get("CODEPACT_PHASE08_ARTIFACT_EVIDENCE"):
        publication_checks.extend(["--receipt", str(Path(receipt).resolve())])
    subprocess.run(publication_checks, cwd=work, check=True)


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        environment = os.environ.copy()
        environment["MATURIN_PEP517_ARGS"] = "--locked"
        environment["PYTHONPATH"] = os.pathsep.join(
            [*site.getsitepackages(), environment.get("PYTHONPATH", "")]
        ).rstrip(os.pathsep)
        build_distributions(root, work / "dist", environment)
        artifacts = check_local_inventory(work / "dist")
        helper_version = locked_package_version(root, "codepact")
        verify_documented_upgrade(work / "dist", work, helper_version)
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
        check_individual_installs(root, work, artifacts, wheelhouse, environment)
        interpreter = check_extra_install(
            root, work, work / "dist", wheelhouse, helper_version, environment
        )
        check_provider_installs(root, work, interpreter)

        source_artifacts = work / "cli-extra-sources"
        source_artifacts.mkdir()
        for artifact in artifacts:
            if artifact.suffix == ".gz":
                shutil.copy2(artifact, source_artifacts / artifact.name)
        _ = check_extra_install(
            root,
            work,
            source_artifacts,
            wheelhouse,
            helper_version,
            environment,
            from_sdists=True,
        )


if __name__ == "__main__":
    main()

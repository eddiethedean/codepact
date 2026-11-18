"""Install and exercise native-wheel and source-archive release candidates."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import site
import subprocess
import sys
import tempfile
import venv

if __package__:
    from .distribution_inputs import release_inputs
    from .locked_dependencies import locked_package_version
else:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        from distribution_inputs import release_inputs
        from locked_dependencies import locked_package_version
    finally:
        _ = sys.path.pop(0)


def environment_python(environment: Path) -> Path:
    scripts = "Scripts" if os.name == "nt" else "bin"
    executable = "python.exe" if os.name == "nt" else "python"
    return environment / scripts / executable


def run(command: list[str], *, cwd: Path, environment: dict[str, str]) -> None:
    subprocess.run(command, cwd=cwd, env=environment, check=True)


def exercise(environment: Path, workdir: Path, env: dict[str, str]) -> None:
    python = environment_python(environment)
    binary_name = "codepact.exe" if os.name == "nt" else "codepact"
    binary = environment / ("Scripts" if os.name == "nt" else "bin") / binary_name
    helper_check = workdir / "helper-smoke.py"
    helper_check.write_text(
        "from typing import Annotated\n"
        "import codepact\n"
        "from codepact.contracts import MustUseResult\n"
        "from codepact.rust import MustUse\n\n"
        "@codepact.must_use\n"
        "def fetch() -> int:\n"
        "    return 1\n\n"
        "assert fetch.__codepact_must_use__ == MustUseResult()\n"
        "assert MustUse[int].__origin__ is int\n"
        "assert Annotated[int, MustUseResult()].__origin__ is int\n",
        encoding="utf-8",
    )
    run([str(python), str(helper_check)], cwd=workdir, environment=env)
    root = Path(__file__).resolve().parents[1]
    run(
        [str(python), "-I", str(root / "scripts/verify_installed_package.py")],
        cwd=workdir,
        environment=env,
    )
    smoke = root / "scripts/smoke_release_cli.py"
    run(
        [str(python), "-I", str(smoke), "--binary", str(binary)],
        cwd=workdir,
        environment=env,
    )


def exercise_cli_extra(
    uv: str,
    version: str,
    artifacts: list[Path],
    wheelhouse: Path,
    workdir: Path,
    env: dict[str, str],
    *,
    from_sdists: bool,
) -> None:
    workdir.mkdir(parents=True)
    venv_path = workdir / "venv"
    venv.EnvBuilder(with_pip=False).create(venv_path)
    python = environment_python(venv_path)
    find_links = workdir / "artifacts"
    find_links.mkdir()
    for artifact in artifacts:
        shutil.copy2(artifact, find_links / artifact.name)
    command = [
        uv,
        "pip",
        "install",
        "--python",
        str(python),
        "--no-index",
    ]
    if from_sdists:
        command.append("--no-build-isolation")
    command.extend(
        [
            "--find-links",
            str(find_links),
            "--find-links",
            str(wheelhouse),
            f"codepact[cli]=={version}",
        ]
    )
    run(command, cwd=workdir, environment=env)
    exercise(venv_path, workdir, env)


def smoke(directory: Path, version: str, *, wheel_only: bool = False) -> None:
    helper_wheel, cli_wheel, helper_source, cli_source = release_inputs(
        directory, version
    )
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    environment["MATURIN_PEP517_ARGS"] = "--locked"
    environment["PYTHONPATH"] = os.pathsep.join(site.getsitepackages())
    uv = shutil.which("uv")
    if uv is None:
        raise RuntimeError("uv is required to create isolated artifact environments")

    with tempfile.TemporaryDirectory(prefix="codepact-release-") as temp:
        root = Path(__file__).resolve().parents[1]
        workdir = Path(temp)
        wheelhouse = workdir / "wheelhouse"
        wheelhouse.mkdir()
        system_python = Path(sys.base_prefix) / (
            "python.exe"
            if os.name == "nt"
            else f"bin/python{sys.version_info.major}.{sys.version_info.minor}"
        )
        typing_extensions = locked_package_version(root, "typing-extensions")
        run(
            [
                str(system_python),
                "-m",
                "pip",
                "download",
                "--no-deps",
                "--only-binary=:all:",
                "--dest",
                str(wheelhouse),
                f"typing-extensions=={typing_extensions}",
            ],
            cwd=workdir,
            environment=environment,
        )

        wheel_dir = workdir / "wheel"
        wheel_dir.mkdir()
        venv_path = wheel_dir / "venv"
        venv.EnvBuilder(with_pip=False).create(venv_path)
        python = environment_python(venv_path)
        run(
            [
                uv,
                "pip",
                "install",
                "--python",
                str(python),
                "--no-index",
                "--find-links",
                str(wheelhouse),
                str(cli_wheel),
                str(helper_wheel),
            ],
            cwd=wheel_dir,
            environment=environment,
        )
        exercise(venv_path, wheel_dir, environment)
        exercise_cli_extra(
            uv,
            version,
            [helper_wheel, cli_wheel],
            wheelhouse,
            Path(temp) / "wheel-extra",
            environment,
            from_sdists=False,
        )

        if not wheel_only:
            workdir = Path(temp) / "sdist"
            workdir.mkdir()
            venv_path = workdir / "venv"
            venv.EnvBuilder(with_pip=False).create(venv_path)
            python = environment_python(venv_path)
            run(
                [
                    uv,
                    "pip",
                    "install",
                    "--python",
                    str(python),
                    "--no-index",
                    "--find-links",
                    str(wheelhouse),
                    f"typing-extensions=={typing_extensions}",
                ],
                cwd=workdir,
                environment=environment,
            )
            for source in (cli_source, helper_source):
                run(
                    [
                        uv,
                        "pip",
                        "install",
                        "--python",
                        str(python),
                        "--no-deps",
                        str(source),
                    ],
                    cwd=workdir,
                    environment=environment,
                )
            exercise(venv_path, workdir, environment)
            exercise_cli_extra(
                uv,
                version,
                [helper_source, cli_source],
                wheelhouse,
                Path(temp) / "sdist-extra",
                environment,
                from_sdists=True,
            )

    scope = "wheel" if wheel_only else "wheel and source archive"
    print(f"{scope.title()} install/smoke checks passed: {version}")


class Arguments(argparse.Namespace):
    directory: Path = Path("dist")
    version: str = ""
    wheel_only: bool


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument(
        "--wheel-only",
        action="store_true",
        help="skip source archive build/install checks for a Python compatibility run",
    )
    arguments = Arguments()
    _ = parser.parse_args(namespace=arguments)
    smoke(arguments.directory, arguments.version, wheel_only=arguments.wheel_only)

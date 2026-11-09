"""Install and exercise native-wheel and source-archive release candidates."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import venv

from packaging.tags import sys_tags
from packaging.utils import parse_sdist_filename, parse_wheel_filename
from packaging.version import Version


def require_one(paths: list[Path], label: str) -> Path:
    if len(paths) != 1:
        raise ValueError(f"Expected one {label}, found {len(paths)}")
    return paths[0]


def release_inputs(directory: Path, version: str) -> tuple[Path, Path, Path, Path]:
    helper_wheel = require_one(
        [path for path in directory.glob("codepact-*.whl")], "helper wheel"
    )
    if parse_wheel_filename(helper_wheel.name)[1] != Version(version):
        raise ValueError("Helper wheel version does not match the release")

    supported = set(sys_tags())
    native_wheels: list[Path] = []
    for wheel in directory.glob("codepact_cli-*.whl"):
        _, wheel_version, _, tags = parse_wheel_filename(wheel.name)
        if wheel_version == Version(version) and tags & supported:
            native_wheels.append(wheel)
    cli_wheel = require_one(native_wheels, "host-compatible CLI wheel")

    sources = sorted(directory.glob("*.tar.gz"))
    by_name: dict[str, Path] = {}
    for source in sources:
        name, source_version = parse_sdist_filename(source.name)
        if source_version == Version(version):
            by_name[name] = source
    if set(by_name) != {"codepact", "codepact-cli"}:
        raise ValueError("Expected matching helper and CLI source archives")
    return helper_wheel, cli_wheel, by_name["codepact"], by_name["codepact-cli"]


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
    smoke = root / "scripts/smoke_release_cli.py"
    run(
        [str(python), str(smoke), "--binary", str(binary)],
        cwd=workdir,
        environment=env,
    )


def smoke(directory: Path, version: str) -> None:
    helper_wheel, cli_wheel, helper_source, cli_source = release_inputs(
        directory, version
    )
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    environment["MATURIN_PEP517_ARGS"] = "--locked"
    uv = shutil.which("uv")
    if uv is None:
        raise RuntimeError("uv is required to create isolated artifact environments")

    with tempfile.TemporaryDirectory(prefix="codepact-release-wheel-") as temp:
        workdir = Path(temp)
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
                str(cli_wheel),
                str(helper_wheel),
            ],
            cwd=workdir,
            environment=environment,
        )
        exercise(venv_path, workdir, environment)

    with tempfile.TemporaryDirectory(prefix="codepact-release-sdist-") as temp:
        workdir = Path(temp)
        venv_path = workdir / "venv"
        venv.EnvBuilder(with_pip=False).create(venv_path)
        python = environment_python(venv_path)
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

    print(f"Wheel and source archive install/smoke checks passed: {version}")


class Arguments(argparse.Namespace):
    directory: Path = Path("dist")
    version: str = ""


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    arguments = Arguments()
    _ = parser.parse_args(namespace=arguments)
    smoke(arguments.directory, arguments.version)

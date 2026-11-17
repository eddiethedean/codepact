"""Verify the pure helper and native CLI wheels and source distributions."""

import hashlib
import importlib.util
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import site
import zipfile
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


def verify_installed_marked_provider(
    interpreter: Path, executable: Path, work: Path
) -> None:
    """Build and install marked provider wheels, then qualify CLI discovery."""
    source = work / "provider-source"
    package = source / "demo"
    distributions = work / "provider-wheels"
    project = work / "provider-caller"
    package.mkdir(parents=True)
    distributions.mkdir()
    project.mkdir()
    sentinel = work / "provider-imported"
    (source / "pyproject.toml").write_text(
        '[build-system]\nrequires = ["setuptools"]\n'
        'build-backend = "setuptools.build_meta"\n',
        encoding="utf-8",
    )
    (source / "setup.py").write_text(
        "import os\n"
        "from setuptools import setup\n"
        "setup(name='demo-dist', version=os.environ['DEMO_PROVIDER_VERSION'], "
        "packages=['demo'], package_data={'demo': ['py.codepact']})\n",
        encoding="utf-8",
    )
    (package / "__init__.py").write_text(
        "from ._impl import decorated\n"
        "from ._impl import typed as public_typed\n"
        "from ._impl import annotated as public_annotated\n"
        "from ._impl import ordinary\n",
        encoding="utf-8",
    )
    (package / "_sentinel.py").write_text(
        f"open({str(sentinel)!r}, 'w').close()\n", encoding="utf-8"
    )
    (package / "_impl.py").write_text(
        "import demo._sentinel\n"
        "from typing import Annotated\n"
        "from codepact import must_use\n"
        "from codepact.contracts import MustUse, MustUseResult\n"
        "@must_use\n"
        "def decorated() -> int: return 1\n"
        "def typed() -> MustUse[int]: return 2\n"
        "def annotated() -> Annotated[int, MustUseResult()]: return 3\n"
        "def ordinary() -> int: return 4\n",
        encoding="utf-8",
    )
    (package / "py.codepact").write_text("", encoding="utf-8")
    environment = os.environ.copy()
    wheels: dict[str, Path] = {}
    provider_build = [sys.executable, "-m", "build"]
    if importlib.util.find_spec("setuptools") is not None:
        provider_build.append("--no-isolation")
    for version in ("1.0.0", "2.0.0"):
        build_environment = environment.copy()
        build_environment["DEMO_PROVIDER_VERSION"] = version
        subprocess.run(
            [
                *provider_build,
                "--wheel",
                "--outdir",
                str(distributions),
                str(source),
            ],
            env=build_environment,
            check=True,
        )
        wheels[version] = next(distributions.glob(f"demo_dist-{version}-*.whl"))

    purelib = Path(
        subprocess.run(
            [
                str(interpreter),
                "-c",
                "import sysconfig; print(sysconfig.get_path('purelib'))",
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    )
    (project / "pyproject.toml").write_text(
        "[tool.codepact]\n"
        "config-version = 1\n"
        'enabled-rules = ["TP3001"]\n'
        "\n[tool.codepact.dependencies]\n"
        'required-providers = ["demo-dist"]\n',
        encoding="utf-8",
    )
    (project / "main.py").write_text(
        "from codepact import discard\n"
        "from demo import decorated, ordinary, public_typed as alias_typed\n"
        "from demo import public_annotated\n"
        "decorated()\n"
        "alias_typed()\n"
        "public_annotated()\n"
        "handled = ordinary()\n"
        "discard(decorated())\n",
        encoding="utf-8",
    )
    environment["CODEPACT_PROVIDER_SENTINEL"] = str(sentinel)

    def install(version: str) -> None:
        subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--python",
                str(interpreter),
                "--no-deps",
                "--force-reinstall",
                str(wheels[version]),
            ],
            cwd=work,
            env=environment,
            check=True,
        )

    def check(*arguments: str) -> dict[str, object]:
        command = [str(executable), "check", "--root", "."]
        if "--dependency-root" not in arguments:
            command.extend(["--dependency-root", str(purelib)])
        command.extend(["--json", *arguments])
        result = subprocess.run(
            command,
            cwd=project,
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode not in (0, 1):
            raise RuntimeError(
                f"installed provider check failed ({result.returncode}): "
                f"{result.stdout}\n{result.stderr}"
            )
        return cast(dict[str, object], json.loads(result.stdout))

    install("1.0.0")
    report = check()
    diagnostics = cast(list[dict[str, object]], report["diagnostics"])
    findings = [item for item in diagnostics if item.get("code") == "TP3001"]
    evidence = [cast(dict[str, object], item["provider_evidence"]) for item in findings]
    diagnostic_summary = [
        (
            item.get("code"),
            item.get("message"),
            cast(dict[str, object], item["primary"]).get("line"),
            item.get("provider_evidence"),
        )
        for item in diagnostics
    ]
    assert len(findings) == 3, diagnostic_summary
    assert {item["version"] for item in evidence} == {"1.0.0"}, report
    assert {item["distribution"] for item in evidence} == {"demo-dist"}, report
    assert any(
        item["public_symbol"] == "demo.public_annotated"
        and item["canonical_symbol"] == "demo._impl.annotated"
        for item in evidence
    ), report
    assert any(
        item["public_symbol"] == "demo.public_typed"
        and item["canonical_symbol"] == "demo._impl.typed"
        for item in evidence
    ), report
    assert not sentinel.exists(), "the installed provider module was executed"

    disabled = check("--rules", "TP1001")
    disabled_provider_findings = [
        item
        for item in cast(list[dict[str, object]], disabled["diagnostics"])
        if item.get("code") == "TP3001"
    ]
    assert len(disabled_provider_findings) == 3 and all(
        item["disposition"] == "rule-disabled" for item in disabled_provider_findings
    ), disabled_provider_findings

    baseline = project / "provider-baseline.json"
    baseline_result = subprocess.run(
        [
            str(executable),
            "baseline",
            "update",
            "--root",
            ".",
            "--dependency-root",
            str(purelib),
            "--output",
            str(baseline),
            "--reason",
            "qualify installed provider disposition",
            "--json",
        ],
        cwd=project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    if baseline_result.returncode != 0:
        raise RuntimeError(
            "installed provider baseline update failed: "
            f"{baseline_result.stdout}\n{baseline_result.stderr}"
        )
    baselined = check("--baseline", str(baseline))
    assert baselined["coverage"] == report["coverage"], baselined
    assert all(
        item["disposition"] == "suppressed-baseline"
        for item in cast(list[dict[str, object]], baselined["diagnostics"])
        if item["code"] == "TP3001"
    ), baselined

    install("2.0.0")
    changed_provider = check("--baseline", str(baseline))
    changed_findings = [
        item
        for item in cast(list[dict[str, object]], changed_provider["diagnostics"])
        if item["code"] == "TP3001"
    ]
    changed_evidence = [
        cast(dict[str, object], item["provider_evidence"]) for item in changed_findings
    ]
    assert len(changed_findings) == 3, changed_provider
    assert {item["version"] for item in changed_evidence} == {"2.0.0"}, changed_provider
    assert all(
        item["disposition"] != "suppressed-baseline" for item in changed_findings
    ), changed_provider

    unmarked = work / "unmarked-site"
    unmarked.mkdir()
    shutil.copytree(
        purelib / "demo",
        unmarked / "demo",
        ignore=shutil.ignore_patterns("py.codepact"),
    )
    for info in purelib.glob("demo_dist-*.dist-info"):
        shutil.copytree(info, unmarked / info.name)
    unresolved = check("--dependency-root", str(unmarked))
    unresolved_coverage = cast(dict[str, object], unresolved["coverage"])
    assert unresolved_coverage["status"] == "incomplete", unresolved
    assert any(
        item["code"] == "TP9001"
        for item in cast(list[dict[str, object]], unresolved["diagnostics"])
    ), unresolved

    shadow = project / "demo"
    shadow.mkdir()
    (shadow / "__init__.py").write_text(
        "from ._impl import annotated as public_annotated\n", encoding="utf-8"
    )
    (shadow / "_impl.py").write_text(
        "def decorated() -> int: return 1\n"
        "def typed() -> int: return 2\n"
        "def annotated() -> int: return 3\n"
        "def ordinary() -> int: return 4\n",
        encoding="utf-8",
    )
    local_shadow = check()
    assert not any(
        item.get("code") == "TP3001"
        for item in cast(list[dict[str, object]], local_shadow["diagnostics"])
    ), local_shadow
    assert not sentinel.exists(), "provider source execution sentinel was created"
    evidence_path = os.environ.get("CODEPACT_PHASE07_PROVIDER_EVIDENCE")
    if evidence_path:
        try:
            source_revision = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=Path(__file__).resolve().parent.parent,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            source_revision = "unavailable"
        record = {
            "schema_version": 1,
            "qualification": "phase-0.7-installed-provider-wheel",
            "source_revision": source_revision,
            "platform": platform.platform(),
            "python": sys.version,
            "cli_version": subprocess.run(
                [str(executable), "--version"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip(),
            "provider_wheels": [
                {
                    "name": wheel.name,
                    "version": version,
                    "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
                    "installed_outside_source_tree": True,
                }
                for version, wheel in wheels.items()
            ],
            "marked_provider": {
                "coverage": report["coverage"],
                "must_use_findings": [
                    {
                        "line": cast(dict[str, object], item["primary"])["line"],
                        "provider_evidence": item["provider_evidence"],
                    }
                    for item in findings
                ],
                "no_execution_sentinel_created": not sentinel.exists(),
            },
            "disabled_rule": {
                "TP3001_dispositions": [
                    item["disposition"] for item in disabled_provider_findings
                ]
            },
            "baseline": {
                "coverage_preserved": baselined["coverage"] == report["coverage"],
                "TP3001_dispositions": [
                    item["disposition"]
                    for item in cast(list[dict[str, object]], baselined["diagnostics"])
                    if item["code"] == "TP3001"
                ],
                "changed_provider_version_is_not_suppressed": all(
                    item["disposition"] != "suppressed-baseline"
                    for item in changed_findings
                ),
            },
            "unmarked_required_provider": {
                "coverage": unresolved_coverage["status"],
                "has_unknown_boundary": any(
                    item["code"] == "TP9001"
                    for item in cast(list[dict[str, object]], unresolved["diagnostics"])
                ),
            },
            "local_shadow": {
                "active_TP3001": sum(
                    item["code"] == "TP3001" and item["disposition"] == "active"
                    for item in cast(
                        list[dict[str, object]], local_shadow["diagnostics"]
                    )
                )
            },
            "provider_source_execution": False,
            "commands": [
                [
                    "codepact",
                    "check",
                    "--root",
                    ".",
                    "--dependency-root",
                    "<installed-site-packages>",
                    "--json",
                ],
                [
                    "codepact",
                    "check",
                    "--root",
                    ".",
                    "--dependency-root",
                    "<installed-site-packages>",
                    "--rules",
                    "TP1001",
                    "--json",
                ],
                [
                    "codepact",
                    "baseline",
                    "update",
                    "--root",
                    ".",
                    "--dependency-root",
                    "<installed-site-packages>",
                    "--output",
                    "provider-baseline.json",
                    "--reason",
                    "qualification",
                    "--json",
                ],
                [
                    "codepact",
                    "check",
                    "--root",
                    ".",
                    "--dependency-root",
                    "<unmarked-site-packages>",
                    "--json",
                ],
            ],
        }
        destination = Path(evidence_path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(
        "Installed marked-provider wheel passed provenance, version, baseline, "
        "disabled-rule, required-provider, local-shadow and no-execution checks"
    )


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
        scripts_directory = Path(
            subprocess.run(
                [
                    str(extra_interpreter),
                    "-c",
                    "import sysconfig; print(sysconfig.get_path('scripts'))",
                ],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        )
        verify_installed_marked_provider(
            extra_interpreter,
            scripts_directory / ("codepact.exe" if os.name == "nt" else "codepact"),
            work,
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

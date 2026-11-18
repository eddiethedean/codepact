"""Replay inline provider publication against installed wheels."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
from typing import cast


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

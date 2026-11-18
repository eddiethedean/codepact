"""Qualify the installed author CLI and downstream wheel workflow."""

from __future__ import annotations

import argparse
from base64 import urlsafe_b64encode
import csv
import hashlib
import importlib.util
import io
import json
import shutil
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
from typing import cast
import zipfile


DISTRIBUTION = "codepact-provider-probe"
PACKAGE = "reviewed_provider"
MANIFEST = "codepact.contracts.json"


def command(argv: list[str], cwd: Path, *, exit_code: int = 0) -> str:
    completed = subprocess.run(
        argv, cwd=cwd, capture_output=True, text=True, check=False
    )
    if completed.returncode != exit_code:
        raise RuntimeError(
            f"{argv!r} exited {completed.returncode}, expected {exit_code}\n"
            f"{completed.stdout}\n{completed.stderr}"
        )
    return completed.stdout


def build(source: Path, destination: Path) -> Path:
    destination.mkdir()
    argv = [sys.executable, "-m", "build", "--wheel", "--outdir", str(destination)]
    if importlib.util.find_spec("setuptools") is not None:
        argv.append("--no-isolation")
    _ = command(argv, source)
    [wheel] = sorted(destination.glob("*.whl"))
    return wheel


def project(source: Path, name: str, module: str, version: str) -> Path:
    package = source / module
    package.mkdir(parents=True)
    _ = (source / "pyproject.toml").write_text(
        '[build-system]\nrequires = ["setuptools"]\n'
        'build-backend = "setuptools.build_meta"\n'
        f'[project]\nname = "{name}"\nversion = "{version}"\n'
        f'[tool.setuptools]\npackages = ["{module}"]\n'
        f'[tool.setuptools.package-data]\n{module} = ["py.codepact", "{MANIFEST}"]\n',
        encoding="utf-8",
    )
    return package


def install(work: Path, environment: str, wheels: list[Path]) -> Path:
    root = work / environment
    _ = command(["uv", "venv", "--python", sys.executable, str(root)], work)
    python = root / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    _ = command(
        [
            "uv",
            "pip",
            "install",
            "--no-deps",
            "--no-index",
            "--python",
            str(python),
            *map(str, wheels),
        ],
        work,
    )
    return Path(
        command(
            [
                str(python),
                "-I",
                "-c",
                "import sysconfig; print(sysconfig.get_path('purelib'))",
            ],
            work,
        ).strip()
    )


def rewrite_wheel(
    original: Path,
    destination: Path,
    changes: dict[str, bytes | None],
    *,
    refresh_record: bool = True,
) -> Path:
    destination.mkdir()
    rewritten = destination / original.name
    with zipfile.ZipFile(original) as archive:
        members = {name: archive.read(name) for name in archive.namelist()}
    for name, data in changes.items():
        if data is None:
            del members[name]
        else:
            members[name] = data
    if refresh_record:
        record = next(name for name in members if name.endswith(".dist-info/RECORD"))
        records = io.StringIO()
        writer = csv.writer(records, lineterminator="\n")
        for name, data in sorted(members.items()):
            if name == record:
                _ = cast(int, writer.writerow([name, "", ""]))
            else:
                digest = (
                    urlsafe_b64encode(hashlib.sha256(data).digest())
                    .rstrip(b"=")
                    .decode("ascii")
                )
                _ = cast(int, writer.writerow([name, f"sha256={digest}", len(data)]))
        members[record] = records.getvalue().encode()
    with zipfile.ZipFile(rewritten, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in members.items():
            archive.writestr(name, data)
    return rewritten


def verify_project_model(
    cli: Path, config: Path, work: Path, inline: Path, caller_site: Path
) -> dict[str, object]:
    wheel = rewrite_wheel(
        inline, work / "unmarked-wheel", {f"{PACKAGE}/py.codepact": None}
    )
    site = install(work, "isolated-unmarked", [wheel])
    contract = work / "project-model.json"
    lock = work / "uv.lock"
    _ = contract.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "models": [
                    {
                        "distribution": DISTRIBUTION,
                        "version": "1.0",
                        "factory": f"{PACKAGE}.acquire",
                        "resource": f"{PACKAGE}._impl.Channel",
                        "close_method": "close",
                        "provenance": "Phase 0.8 installed package control",
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    with config.open("a", encoding="utf-8") as stream:
        _ = stream.write(
            "\n[tool.codepact.dependencies]\n"
            'project-contracts = ["project-model.json"]\n'
            'uv-lock = "uv.lock"\n'
        )
    args = [
        str(cli),
        "check",
        "--root",
        str(caller_site),
        "--config",
        str(config),
        "--dependency-root",
        str(site),
        "--python-version",
        "3.13",
        "--json",
    ]

    def locked(version: str) -> str:
        return (
            f'version = 1\nrevision = 3\n[[package]]\nname = "{DISTRIBUTION}"\n'
            f'version = "{version}"\n'
            'source = { registry = "https://pypi.org/simple" }\n'
        )

    versions = locked("1.0")
    _ = lock.write_text(versions, encoding="utf-8")
    report = cast(dict[str, object], json.loads(command(args, work, exit_code=1)))
    foundations = cast(dict[str, object], report["foundations"])
    uses = cast(list[dict[str, object]], foundations["model_uses"])
    assert any(
        str(item["origin"]).startswith("project-model:") and item["applicable"]
        for item in uses
    )

    def rejection(content: str, reason: str) -> None:
        _ = lock.write_text(content, encoding="utf-8")
        report = cast(dict[str, object], json.loads(command(args, work)))
        coverage = cast(dict[str, object], report["coverage"])
        assert any(reason in value for value in cast(list[str], coverage["reasons"]))
        assert all(
            item["code"] != "TP1002"
            for item in cast(list[dict[str, object]], report["diagnostics"])
        )

    rejection(locked("2.0"), "uv.lock version 2.0 disagrees")
    duplicate = versions[versions.index("[[package]]") :]
    rejection(versions + duplicate, "ambiguous or conditional")
    model_source = contract.read_text(encoding="utf-8")
    _ = contract.write_text(
        model_source.replace('"close_method": "close"', '"close_method": "dispose"'),
        encoding="utf-8",
    )
    rejection(versions, "cleanup method reviewed_provider._impl.Channel.dispose")
    _ = contract.write_text(model_source, encoding="utf-8")
    module = site / PACKAGE / "_impl.py"
    modified = module.with_suffix(".modified")
    _ = modified.write_text(
        module.read_text(encoding="utf-8").replace("return Channel()", "return None"),
        encoding="utf-8",
    )
    _ = modified.replace(module)
    rejection(versions, "RECORD hash or size mismatch")
    _ = contract.write_text('{"schema_version":1,"models":[{}]}', encoding="utf-8")
    _ = command(args, work, exit_code=2)
    return {
        "unmarked_wheel_sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
        "selected_lock_sha256": foundations["lock_sha256"],
        "selected_model_sha256": foundations["project_models_sha256"],
        "rejected_inputs": [
            "lock-mismatch",
            "ambiguous-lock",
            "unresolved-cleanup-method",
            "modified-installed-source",
            "malformed-model",
        ],
    }


def compare_published_upgrades(
    cli: Path,
    work: Path,
    original_source: Path,
    original: Path,
    caller_check: list[str],
) -> dict[str, object]:
    results: dict[str, object] = {}
    for label, version in (
        ("compatible-move", "1.1"),
        ("new-caller-obligation", "1.2"),
        ("weakened-guarantee", "1.3"),
    ):
        stage = work / f"upgrade-source-{label}"
        package = project(stage, DISTRIBUTION, PACKAGE, version)
        _ = shutil.copytree(original_source / PACKAGE, package, dirs_exist_ok=True)
        implementation = package / "_impl.py"
        if label == "compatible-move":
            _ = implementation.rename(package / "_relocated.py")
            module = package / "__init__.py"
            _ = module.write_text(
                module.read_text(encoding="utf-8").replace("._impl", "._relocated"),
                encoding="utf-8",
            )
        elif label == "new-caller-obligation":
            operations = package / "operations.py"
            _ = operations.write_text(
                operations.read_text(encoding="utf-8").replace(
                    "def receipt():", "@must_use\ndef receipt():"
                ),
                encoding="utf-8",
            )
        else:
            _ = implementation.write_text(
                implementation.read_text(encoding="utf-8").replace(
                    "def acquire() -> 'Owned[Channel]':", "def acquire():"
                ),
                encoding="utf-8",
            )
        manifest = command(
            [
                str(cli),
                "contracts",
                "export",
                "--root",
                str(stage),
                "--distribution",
                DISTRIBUTION,
                "--version",
                version,
                "--import-scope",
                PACKAGE,
                "--python-version",
                "3.13",
            ],
            work,
        )
        _ = (package / MANIFEST).write_text(manifest, encoding="utf-8")
        candidate = build(stage, work / f"upgrade-wheel-{label}")
        _ = command([str(cli), "contracts", "verify", str(candidate)], work)
        args = [str(cli), "contracts", "diff", str(original), str(candidate)]
        status = 0 if label == "compatible-move" else 1
        json_text = command([*args, "--json"], work, exit_code=status)
        assert json_text == command([*args, "--json"], work, exit_code=status)
        result = cast(dict[str, object], json.loads(json_text))
        changes = cast(list[dict[str, object]], result["changes"])
        assert result["compatible"] == (status == 0)
        if status == 0:
            assert any(
                "implementation moved" in str(change["detail"]) for change in changes
            )
        else:
            assert any(change["kind"] == label for change in changes)
        text = command(args, work, exit_code=status)
        assert ("compatible" if status == 0 else label) in text
        site = install(work, f"isolated-upgrade-{label}", [candidate])
        check_args = caller_check.copy()
        check_args[check_args.index("--dependency-root") + 1] = str(site)
        check_exit = 0 if label == "weakened-guarantee" else 1
        check = cast(
            dict[str, object],
            json.loads(command(check_args, work, exit_code=check_exit)),
        )
        findings = cast(list[dict[str, object]], check["diagnostics"])
        if label == "weakened-guarantee":
            coverage = cast(dict[str, object], check["coverage"])
            assert coverage["status"] == "incomplete"
            foundations = cast(dict[str, object], check["foundations"])
            assert not any(
                use["effect"] == "owned-return" and use["applicable"]
                for use in cast(list[dict[str, object]], foundations["model_uses"])
            )
        if label == "new-caller-obligation":
            assert sum(item["code"] == "TP3001" for item in findings) == 2
        else:
            assert sum(item["code"] == "TP3001" for item in findings) == 1
        results[label] = {
            "wheel_sha256": hashlib.sha256(candidate.read_bytes()).hexdigest(),
            "changes": changes,
        }

    with zipfile.ZipFile(original) as archive:
        manifest_text = archive.read(f"{PACKAGE}/{MANIFEST}")
    manifest_document = cast(dict[str, object], json.loads(manifest_text))
    symbols = cast(list[dict[str, object]], manifest_document["symbols"])
    facts = cast(list[dict[str, object]], symbols[0]["contracts"])
    facts[0]["required_capabilities"] = ["unsupported-future-effect"]
    unavailable = rewrite_wheel(
        original,
        work / "unsupported-diff-wheel",
        {f"{PACKAGE}/{MANIFEST}": json.dumps(manifest_document).encode()},
    )
    uncertain = cast(
        dict[str, object],
        json.loads(
            command(
                [
                    str(cli),
                    "contracts",
                    "diff",
                    str(original),
                    str(unavailable),
                    "--json",
                ],
                work,
                exit_code=1,
            )
        ),
    )
    assert not uncertain["compatible"]
    assert any(
        change["kind"] == "insufficient-evidence"
        for change in cast(list[dict[str, object]], uncertain["changes"])
    )
    results["insufficient-evidence"] = {
        "wheel_sha256": hashlib.sha256(unavailable.read_bytes()).hexdigest(),
        "changes": uncertain["changes"],
    }
    return results


def verify_artifacts(cli: Path, work: Path) -> dict[str, object]:
    provider_source = work / "provider"
    provider = project(provider_source, DISTRIBUTION, PACKAGE, "1.0")
    caller = project(
        work / "caller", "codepact-caller-probe", "published_caller", "1.0"
    )
    provider_sentinel = work / "PROVIDER_IMPORTED"
    caller_sentinel = work / "CALLER_IMPORTED"
    _ = (provider / "__init__.py").write_text(
        "from ._impl import Channel as Channel, acquire as acquire, "
        "consume as consume\nfrom . import _startup\n",
        encoding="utf-8",
    )
    _ = (provider / "_startup.py").write_text(
        f"open({str(provider_sentinel)!r}, 'w').close()\n", encoding="utf-8"
    )
    _ = (provider / "operations.py").write_text(
        "from codepact import must_use\n@must_use\ndef result(): return 1\n"
        "def receipt(): return 1\n",
        encoding="utf-8",
    )
    _ = (provider / "_impl.py").write_text(
        "from __future__ import annotations\n"
        "from codepact import resource\nfrom codepact.rust import Owned\n"
        "@resource(closed_by='close')\nclass Channel:\n    def close(self): pass\n"
        "    def read(self): return 1\n"
        "def acquire() -> 'Owned[Channel]':\n    return Channel()\n"
        "def consume(resource: 'Owned[Channel]', flag=False):\n"
        "    if flag: raise ValueError('worker failure')\n"
        "    resource.close()\n",
        encoding="utf-8",
    )
    _ = (provider / "py.codepact").write_bytes(b"")
    _ = (caller / "__init__.py").write_text(
        f"open({str(caller_sentinel)!r}, 'w').close()\n", encoding="utf-8"
    )
    _ = (caller / "work.py").write_text(
        f"from {PACKAGE} import acquire, operations as ops, consume\n"
        "from codepact import move\nops.result()\nops.receipt()\n"
        "def leak():\n    value = acquire()\n"
        "def repair():\n    value = acquire()\n    value.close()\n"
        "def run():\n    value = acquire()\n"
        "    consume(move(value))\n    value.read()\n",
        encoding="utf-8",
    )
    inline = build(provider_source, work / "inline-wheel")
    export = [
        str(cli),
        "contracts",
        "export",
        "--root",
        str(provider_source),
        "--distribution",
        DISTRIBUTION,
        "--version",
        "1.0",
        "--import-scope",
        PACKAGE,
        "--python-version",
        "3.13",
    ]
    manifest = command(export, work)
    if manifest != command(export, work):
        raise RuntimeError(
            "the installed CLI produced different manifest bytes on the same inputs"
        )
    _ = (provider / MANIFEST).write_text(manifest, encoding="utf-8")
    wheel = build(provider_source, work / "manifest-wheel")
    verified = cast(
        dict[str, object],
        json.loads(command([str(cli), "contracts", "verify", str(wheel)], work)),
    )
    assert verified["status"] == "verified"
    module_path = f"{PACKAGE}/_impl.py"
    edited_module = (
        provider / "_impl.py"
    ).read_bytes() + b"\n# edited artifact input\n"
    invalid_wheels = {
        "native-shadow": rewrite_wheel(
            wheel,
            work / "native-shadow",
            {f"{PACKAGE}/operations.so": b"controlled native shadow"},
        ),
        "marker-missing": rewrite_wheel(
            wheel, work / "no-marker", {f"{PACKAGE}/py.codepact": None}
        ),
        "manifest-missing": inline,
        "record-mismatch": rewrite_wheel(
            wheel,
            work / "bad-record",
            {module_path: edited_module},
            refresh_record=False,
        ),
        "source-stale": rewrite_wheel(
            wheel, work / "stale-wheel", {module_path: edited_module}
        ),
    }
    for name, value in (
        ("distribution", "another-provider"),
        ("import_scope", "another_package"),
        ("schema_version", 99),
    ):
        metadata = cast(dict[str, object], json.loads(manifest))
        metadata[name] = value
        invalid_wheels[name] = rewrite_wheel(
            wheel, work / name, {f"{PACKAGE}/{MANIFEST}": json.dumps(metadata).encode()}
        )
    for invalid in invalid_wheels.values():
        _ = command([str(cli), "contracts", "verify", str(invalid)], work, exit_code=2)
    caller_wheel = build(caller.parent, work / "caller-wheel")
    caller_site = install(work, "isolated-caller", [caller_wheel])
    config = work / "caller-check.toml"
    _ = config.write_text(
        '[tool.codepact]\nconfig-version = 1\ninclude = ["published_caller/**/*.py"]\n',
        encoding="utf-8",
    )
    reports: list[dict[str, object]] = []
    candidates: dict[str, tuple[Path, list[str]]] = {}
    for origin, artifact in (("inline-source", inline), ("manifest", wheel)):
        site = install(work, f"isolated-{origin}", [artifact])
        args = [
            str(cli),
            "check",
            "--root",
            str(caller_site),
            "--config",
            str(config),
            "--dependency-root",
            str(site),
            "--python-version",
            "3.13",
            "--experimental-ownership",
            "--experimental-advanced-contracts",
            "--json",
        ]
        candidates[origin] = (site, args)
        report = cast(dict[str, object], json.loads(command(args, work, exit_code=1)))
        foundations = cast(dict[str, object], report["foundations"])
        models = cast(list[dict[str, object]], foundations["model_uses"])
        assert any(
            item["origin"] == origin
            and item["effect"] == "owned-return"
            and item["applicable"]
            for item in models
        )
        reports.append(report)
    for report in reports:
        diagnostics = cast(list[dict[str, object]], report["diagnostics"])
        assert {"TP1002", "TP2002", "TP3001"} <= {
            finding["code"] for finding in diagnostics
        }

    def findings(
        report: dict[str, object],
    ) -> list[tuple[object, object, object, object]]:
        return [
            (item["code"], item["certainty"], item["primary"], item["completion"])
            for item in cast(list[dict[str, object]], report["diagnostics"])
        ]

    if (
        findings(reports[0]) != findings(reports[1])
        or reports[0]["coverage"] != reports[1]["coverage"]
    ):
        raise RuntimeError(
            "independently installed inline and manifest providers disagree"
        )
    manifest_site, manifest_check = candidates["manifest"]
    artifact_diff = compare_published_upgrades(
        cli, work, provider_source, wheel, manifest_check
    )
    _ = command(
        [
            str(cli),
            "contracts",
            "diff",
            str(wheel),
            str(invalid_wheels["source-stale"]),
            "--json",
        ],
        work,
        exit_code=2,
    )
    installed_native = manifest_site / PACKAGE / "operations.so"
    _ = installed_native.write_bytes(b"controlled native shadow")
    shadowed = cast(dict[str, object], json.loads(command(manifest_check, work)))
    coverage = cast(dict[str, object], shadowed["coverage"])
    assert any(
        "native extension" in reason for reason in cast(list[str], coverage["reasons"])
    )
    assert all(item[0] != "TP3001" for item in findings(shadowed))
    installed_native.unlink()
    installed_manifest = manifest_site / PACKAGE / MANIFEST
    published = installed_manifest.read_bytes()
    installed_manifest.unlink()
    missing = cast(dict[str, object], json.loads(command(manifest_check, work)))
    coverage = cast(dict[str, object], missing["coverage"])
    assert any(
        "manifest listed in RECORD is missing" in reason
        for reason in cast(list[str], coverage["reasons"])
    )
    assert all(
        item["code"] not in {"TP1002", "TP2002", "TP3001"}
        for item in cast(list[dict[str, object]], missing["diagnostics"])
    )
    _ = installed_manifest.write_bytes(published)
    installed = manifest_site / PACKAGE / "_impl.py"
    modified = installed.with_suffix(".modified")
    _ = modified.write_bytes(edited_module)
    _ = modified.replace(installed)
    patched = cast(dict[str, object], json.loads(command(manifest_check, work)))
    coverage = cast(dict[str, object], patched["coverage"])
    assert any(
        "stale contract manifest" in reason
        for reason in cast(list[str], coverage["reasons"])
    )
    project_evidence = verify_project_model(cli, config, work, inline, caller_site)
    if provider_sentinel.exists() or caller_sentinel.exists():
        raise RuntimeError("an author or checker command executed fixture source")
    return {
        "status": "passed",
        "cli_version": command([str(cli), "--version"], work).strip(),
        "cli_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(),
        "wheel_sha256": {
            artifact.parent.name: hashlib.sha256(artifact.read_bytes()).hexdigest()
            for artifact in (inline, wheel, caller_wheel)
        },
        "verified_manifest": verified,
        "artifact_diff": artifact_diff,
        "rejected_wheels": sorted(invalid_wheels),
        "rejected_installations": [
            "native-shadow",
            "manifest-removed",
            "source-modified",
        ],
        "inline_findings": findings(reports[0]),
        "project_model": project_evidence,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--cli", required=True, type=Path)
    _ = parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    with TemporaryDirectory(prefix="codepact-phase08-") as directory:
        result = verify_artifacts(
            cast(Path, args.cli).resolve(), Path(directory).resolve()
        )
    report = json.dumps(result, indent=2, sort_keys=True) + "\n"
    receipt = cast(Path | None, args.receipt)
    if receipt is not None:
        receipt.parent.mkdir(parents=True, exist_ok=True)
        _ = receipt.write_text(report, encoding="utf-8")
    print(report, end="")


if __name__ == "__main__":
    main()

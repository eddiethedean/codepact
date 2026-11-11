"""Replay the documented first check, project tutorial, and CI gate."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path
from typing import cast

DOCS = Path(__file__).resolve().parents[1] / "docs"
EXAMPLES = DOCS / "_examples"


def snippet(page: str, name: str, language: str = "sh") -> str:
    text = (DOCS / page).read_text(encoding="utf-8")
    block = rf"<!-- smoke: {name} -->\n```{language}\n(.*?)\n```"
    match = re.search(block, text, re.DOTALL)
    if match is None:
        raise ValueError(f"{page} has no {language} snippet named {name}")
    return match[1]


def shell(cwd: Path, command: str, expected: int = 0) -> str:
    result = subprocess.run(
        ["sh", "-ec", command],
        cwd=cwd,
        env={**os.environ, "UV_TOOL_DIR": str(cwd / ".uv-tools")},
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != expected:
        raise RuntimeError(
            f"{command}\nExpected exit {expected}, got {result.returncode}\n"
            f"{result.stdout}{result.stderr}"
        )
    return result.stdout


def json_object(text: str) -> dict[str, object]:
    return cast(dict[str, object], json.loads(text))


def report(path: Path) -> dict[str, object]:
    return json_object(path.read_text(encoding="utf-8"))


def replay(workspace: Path, checkout_version: str | None) -> None:
    setup = snippet("quickstart.md", "setup")
    version = re.search(r"uv add codepact==([^\s]+)", setup)
    assert version is not None, "Quick start should pin a helper and CLI version"
    published_version = version[1]
    selected_version = checkout_version or published_version
    setup = setup.replace(
        f"codepact=={published_version}", f"codepact=={selected_version}"
    )
    assert f"codepact {selected_version}" in shell(workspace, setup)

    source = workspace / "src"
    connection = source / "connection.py"
    shutil.copyfile(DOCS.parent / "examples/lifecycle.py", connection)
    finding = shell(workspace, snippet("quickstart.md", "finding"), 1)
    expected = snippet("quickstart.md", "finding-output", "text")
    assert finding.rstrip() == expected, finding
    lines = connection.read_text(encoding="utf-8").splitlines(keepends=True)
    assert len(lines) == 23
    _ = connection.write_text("".join(lines[:-1]), encoding="utf-8")
    assert shell(workspace, snippet("quickstart.md", "repair")) == ""

    shutil.copyfile(EXAMPLES / "connection.py", workspace / "connection.py")
    inspection = shell(workspace, snippet("inspector.md", "inspection"))
    assert json_object(inspection) == report(EXAMPLES / "inspection.json")

    project = EXAMPLES / "project"
    for file in project.glob("*.py"):
        shutil.copyfile(file, source / file.name)
    project_check = snippet("project-guide.md", "project-finding")
    finding = shell(workspace, project_check, 1)
    assert "app.py:8:5: error TP1001" in finding
    assert "related: app.py:7:5" in finding

    config_text = (project / "codepact.toml").read_text(encoding="utf-8")
    with (workspace / "pyproject.toml").open("a", encoding="utf-8") as file:
        _ = file.write(f"\n{config_text}")
    settings = shell(workspace, snippet("project-guide.md", "project-settings"))
    assert json_object(settings)["root"] == str(source.resolve())
    assert json_object(settings)["mode"] == "strict"
    inspection_command = snippet("project-guide.md", "project-inspection")
    inspection_report = json_object(shell(workspace, inspection_command))
    coverage = cast(dict[str, object], inspection_report["coverage"])
    assert coverage["status"] == "complete-for-supported-subset"

    nested = workspace / "services/billing"
    shadow = nested / "src/nested"
    shadow.mkdir(parents=True)
    _ = (nested / "pyproject.toml").write_text(config_text, encoding="utf-8")
    _ = (shadow / "pyproject.toml").write_text("[tool.other]\n", encoding="utf-8")
    settings = shell(workspace, snippet("cli.md", "nested-settings"))
    assert json_object(settings)["root"] == str(shadow.parent.resolve())
    origins = cast(dict[str, object], json_object(settings)["settings_origins"])
    assert origins["root"] == "pyproject.toml"
    shadowed = shell(
        workspace,
        "uv run --locked codepact check --root services/billing/src/nested "
        "--show-settings --json",
    )
    shadowed_settings = json_object(shadowed)
    assert shadowed_settings["config_file"] == str(
        (shadow / "pyproject.toml").resolve()
    )
    assert shadowed_settings["mode"] == "advisory"

    shutil.copyfile(EXAMPLES / "check-report.sh", workspace / "check-report.sh")
    ci = snippet("ci-reporting.md", "ci-report")
    output = workspace / "codepact-report.json"
    _ = shell(workspace, ci, 1)
    assert report(output)["actionable_diagnostics"] == 1
    _ = shell(
        workspace,
        "uv run --locked codepact baseline update --output baseline.json "
        '--reason "reviewed demo"',
    )
    _ = shell(workspace, f"{ci} --baseline baseline.json")
    assert report(output)["audit_diagnostics"] == 1
    _ = shell(workspace, f"{ci} --rules TP1002")
    assert report(output)["audit_diagnostics"] == 1
    _ = shell(workspace, f"{ci} --config missing.toml", 2)
    assert report(output)["execution_status"] == "failed"

    shutil.copyfile(source / "app.py", source / "another_app.py")
    _ = shell(workspace, f"{ci} --max-diagnostics 1", 1)
    assert report(output)["details_truncated"] is True
    (source / "another_app.py").unlink()

    workflow = (DOCS / "automation.md").read_text(encoding="utf-8")
    run_step = re.search(
        r"^        run: >-\n((?:          [^\n]+\n)+)", workflow, re.MULTILINE
    )
    assert run_step is not None, "GitHub Actions example needs a runnable check step"
    check_step = " ".join(line.strip() for line in run_step[1].splitlines())
    assert f"codepact-cli=={published_version}" in check_step
    check_step = check_step.replace(
        f"codepact-cli=={published_version}", f"codepact-cli=={selected_version}"
    )
    _ = shell(workspace, check_step, 1)

    app = source / "app.py"
    buggy = app.read_text(encoding="utf-8")
    repaired = buggy.replace(
        "    conn.close()\n    read(conn)", "    read(conn)\n    conn.close()"
    )
    assert repaired != buggy
    _ = app.write_text(repaired, encoding="utf-8")
    _ = shell(workspace, project_check)
    _ = shell(workspace, ci)
    _ = shell(workspace, check_step)

    boundary = source / "boundary.py"
    _ = boundary.write_text("from __future__ import annotations\n", encoding="utf-8")
    _ = shell(workspace, ci, 1)
    _ = shell(workspace, ci.replace("--mode strict", "--mode advisory"))
    coverage = cast(dict[str, object], report(output)["coverage"])
    assert coverage["status"] == "incomplete"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument(
        "--checkout",
        action="store_true",
        help="test checkout wheels while keeping the published versions in the docs",
    )
    checkout_version = None
    if cast(bool, parser.parse_args().checkout):
        manifest = (DOCS.parent / "pyproject.toml").read_text(encoding="utf-8")
        checkout_version = cast(str, tomllib.loads(manifest)["project"]["version"])

    with tempfile.TemporaryDirectory(prefix="codepact-docs-") as temp:
        replay(Path(temp), checkout_version)
    print("Replayed the first check, project tutorial, inspection, and CI report cases")


if __name__ == "__main__":
    main()

"""Replay published provider and work examples and the 0.10 protocol cases."""

from __future__ import annotations

import argparse
import shlex
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import cast

from check_docs_examples import EXAMPLES, json_object, shell, snippet
from sync_docs_reference import PROTOCOLS_PUBLISHED

COMPLETE = "complete-for-supported-subset"


def check(checker: Path, cwd: Path, status: int, *options: str) -> dict[str, object]:
    command = [str(checker), "check", *options, "--json"]
    result = subprocess.run(
        command, cwd=cwd, text=True, capture_output=True, check=False
    )
    if result.returncode != status:
        raise ValueError(
            f"{cwd}, {command}: expected {status}, got {result.returncode}\n"
            f"{result.stdout}{result.stderr}"
        )
    return json_object(result.stdout)


def findings(result: dict[str, object]) -> set[str]:
    return {
        cast(str, finding["code"])
        for finding in cast(list[dict[str, object]], result["diagnostics"])
    }


def foundations(result: dict[str, object]) -> dict[str, object]:
    assert result["report_schema_version"] == 4
    assert result["check_schema_version"] == 2
    return cast(dict[str, object], result["foundations"])


def effects(result: dict[str, object]) -> set[tuple[str, str, bool]]:
    uses = cast(list[dict[str, object]], foundations(result)["model_uses"])
    return {
        (
            cast(str, use["symbol"]),
            cast(str, use["effect"]),
            cast(bool, use["applicable"]),
        )
        for use in uses
    }


def verify_provider(checker: Path, workspace: Path) -> None:
    provider = workspace / "provider"
    _ = shutil.copytree(EXAMPLES / "provider", provider)
    path = f'export PATH={shlex.quote(str(checker.parent))}:"$PATH"\n'
    _ = shell(provider, path + snippet("publish-provider.md", "export-provider"))
    build = path + snippet("publish-provider.md", "build-provider")
    assert '"status": "verified"' in shell(provider, build)
    _ = shell(provider, snippet("consume-providers.md", "install-provider"))

    caller = provider / "caller"
    selected = ("--dependency-root", "../installed")
    result = check(checker, caller, 1, *selected)
    assert "TP3001" in findings(result)
    providers = cast(list[dict[str, object]], foundations(result)["providers"])
    assert len(providers) == 1 and providers[0]["origin"] == "manifest", providers
    assert providers[0]["distribution"] == "acme-storage"
    assert providers[0]["version"] == "1.2.0"
    _ = shutil.copyfile(caller / "variants/repaired.py", caller / "src/main.py")
    assert "TP3001" not in findings(check(checker, caller, 0, *selected))

    boundary = check(checker, caller, 1, "--mode", "strict")
    coverage = cast(dict[str, object], boundary["coverage"])
    assert coverage["status"] == "incomplete"
    reasons = cast(list[str], coverage["reasons"])
    assert any(
        "required CodePact provider acme-storage" in reason for reason in reasons
    )


def verify_work(checker: Path, workspace: Path) -> None:
    work = workspace / "work"
    _ = shutil.copytree(EXAMPLES / "work", work)
    result = check(checker, work, 1)
    assert {"TP3003", "TP5002", "TP5005"} <= findings(result)
    assert ("asyncio.Task", "await-local-task", True) in effects(result)


def verify_concurrency(published: Path, checker: Path, workspace: Path) -> None:
    project = workspace / "concurrency"
    _ = shutil.copytree(EXAMPLES / "concurrency", project)
    source = project / "src"
    source.mkdir()
    cases = (
        ("queue", "valid", 0, None, ("asyncio.Queue.join", "counter-zero", True)),
        ("queue", "finding", 1, "TP7001", ("asyncio.Queue.get", "get", True)),
        ("queue", "boundary", 1, "TP9001", ("asyncio.Queue.get", "get", True)),
        ("guard", "valid", 0, None, ("asyncio.Lock", "guarded-access", True)),
        ("guard", "finding", 1, "TP7002", ("asyncio.Lock", "guarded-access", True)),
        ("guard", "boundary", 1, "TP9001", ("asyncio.Lock", "guarded-access", False)),
        (
            "spawn",
            "valid",
            0,
            None,
            ("multiprocessing.Process.start", "qualified-worker-entry", True),
        ),
        (
            "spawn",
            "finding",
            1,
            "TP7005",
            (
                "multiprocessing.Process.start",
                "serialization-rejected-before-launch",
                True,
            ),
        ),
        ("spawn", "boundary", 1, "TP9001", ("multiprocessing.Process", "join", False)),
    )
    for topic, variant, status, rule, effect in cases:
        _ = shutil.copyfile(project / topic / f"{variant}.py", source / "main.py")
        result = check(
            checker, project, status, "--config", f"{topic}.toml", "--mode", "strict"
        )
        assert effect in effects(result), (topic, variant, effect)
        coverage = cast(dict[str, object], result["coverage"])
        assert coverage["status"] == (
            "incomplete" if variant == "boundary" else COMPLETE
        )
        if rule is None:
            assert not findings(result), (topic, variant, findings(result))
        else:
            assert rule in findings(result), (topic, variant, rule)
        if (topic, variant) == ("spawn", "boundary"):
            uses = effects(result)
            assert ("multiprocessing.SimpleQueue", "receive", False) in uses
            assert (
                "multiprocessing.Process.start",
                "qualified-worker-entry",
                True,
            ) in uses
            assert ("multiprocessing.Process", "close", True) not in uses

    if not PROTOCOLS_PUBLISHED:
        for topic, candidate_setting in (
            ("guard", "async-guards"),
            ("spawn", "spawn-main-script"),
        ):
            result = check(published, project, 2, "--config", f"{topic}.toml")
            error = cast(dict[str, str], result["error"])
            assert f"unknown field `{candidate_setting}`" in error["message"], error


class Arguments(argparse.Namespace):
    published_cli: Path
    checkout_cli: Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--published-cli", type=Path, required=True)
    _ = parser.add_argument("--checkout-cli", type=Path, required=True)
    args = parser.parse_args(namespace=Arguments())
    with tempfile.TemporaryDirectory(prefix="codepact-protocol-docs-") as directory:
        workspace = Path(directory)
        published = args.published_cli.resolve()
        verify_provider(published, workspace)
        verify_work(published, workspace)
        verify_concurrency(published, args.checkout_cli.resolve(), workspace)
    print("Replayed provider, work-result, queue, guard, and spawn docs cases")


if __name__ == "__main__":
    main()

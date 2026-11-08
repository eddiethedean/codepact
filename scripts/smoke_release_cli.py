"""Install and exercise one native CodePact CLI release wheel."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import cast


class Arguments(argparse.Namespace):
    wheel_dir: Path | None
    binary: Path | None


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel-dir", type=Path)
    parser.add_argument("--binary", type=Path)
    args = Arguments()
    _ = parser.parse_args(namespace=args)
    require(not (args.wheel_dir and args.binary), "choose a wheel directory or binary")

    if args.wheel_dir is not None:
        wheels = sorted(args.wheel_dir.glob("codepact_cli-*.whl"))
        require(len(wheels) == 1, "expected exactly one native CLI wheel")
        subprocess.run(
            [
                sys.executable,
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-deps",
                str(wheels[0]),
            ],
            check=True,
        )

    expected_version = importlib.metadata.version("codepact-cli")
    binary = (
        str(args.binary)
        if args.binary is not None
        else shutil.which("codepact") or shutil.which("codepact.exe")
    )
    if binary is None:
        raise SystemExit("installed codepact executable was not found")

    version = subprocess.run(
        [binary, "--version"], capture_output=True, text=True, check=False
    )
    require(
        version.returncode == 0
        and version.stdout.strip() == f"codepact {expected_version}",
        "installed CLI version did not match the wheel metadata",
    )
    help_result = subprocess.run(
        [binary, "--help"], capture_output=True, text=True, check=False
    )
    require(
        help_result.returncode == 0
        and "check" in help_result.stdout + help_result.stderr,
        "installed CLI help smoke failed",
    )

    with tempfile.TemporaryDirectory(prefix="codepact-release-smoke-") as temporary:
        root = Path(temporary)
        marker = root / "source-executed"
        source = f"""open({str(marker)!r}, 'w').close()
from codepact import resource

@resource(closed_by='close')
class Connection:
    def close(self): pass
    def status(self): return 'ready'

connection = Connection()
connection.close()
connection.status()
"""
        (root / "connection.py").write_text(source, encoding="utf-8")
        checked = subprocess.run(
            [binary, "check", "--root", str(root), "--json"],
            capture_output=True,
            text=True,
            check=False,
        )
        require(
            checked.returncode == 1, "lifecycle smoke returned an unexpected exit code"
        )
        try:
            decoded = cast(object, json.loads(checked.stdout))
        except json.JSONDecodeError as error:
            raise SystemExit("lifecycle smoke did not return JSON") from error
        if not isinstance(decoded, dict):
            raise SystemExit("lifecycle smoke JSON was not an object")
        payload = cast(dict[str, object], decoded)
        raw_diagnostics = payload.get("diagnostics")
        if not isinstance(raw_diagnostics, list):
            raise SystemExit("lifecycle smoke JSON did not contain diagnostics")
        diagnostics = cast(list[object], raw_diagnostics)
        require(
            any(
                isinstance(item, dict)
                and cast(dict[str, object], item).get("code") == "TP1001"
                and cast(dict[str, object], item).get("certainty") == "definite"
                for item in diagnostics
            ),
            "lifecycle smoke did not report definite use-after-close",
        )
        require(not marker.exists(), "the analyzer executed inspected Python source")

    print(f"Installed CodePact CLI {expected_version}; lifecycle smoke passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Exercise an installed wheel or source distribution outside the checkout."""

import importlib.metadata
import json
from pathlib import Path
import subprocess
import sys
import sysconfig
import tempfile
from typing import TypedDict, cast


class Diagnostic(TypedDict):
    code: str
    certainty: str
    related: list[dict[str, object]]


class CheckResult(TypedDict):
    diagnostics: list[Diagnostic]


def main() -> None:
    version = importlib.metadata.version("codepact-cli")
    assert importlib.metadata.metadata("codepact-cli")["Requires-Python"] == ">=3.11"
    if "--cli-only" not in sys.argv:
        import codepact

        assert importlib.metadata.version("codepact") == version
        assert f"codepact-cli=={version}" in (
            importlib.metadata.requires("codepact") or []
        )
        assert codepact.__file__ is not None
        package = Path(codepact.__file__).parent
        assert package.is_relative_to(Path(sys.prefix))
        assert (package / "py.typed").is_file()
        assert "codepact._native" not in sys.modules
    executable = Path(sysconfig.get_path("scripts")) / (
        "codepact.exe" if sys.platform == "win32" else "codepact"
    )
    assert executable.is_file(), executable
    result = subprocess.run(
        [str(executable), "--version"], check=True, capture_output=True, text=True
    )
    assert result.stdout.strip() == f"codepact {version}", result.stdout
    subprocess.run([str(executable), "--help"], check=True, capture_output=True)
    with tempfile.TemporaryDirectory() as temporary:
        source = Path(temporary) / "contracts.py"
        source.write_text(
            "from codepact import resource\n"
            "from codepact.rust import Borrowed\n"
            "@resource(closed_by='close')\n"
            "class Connection: pass\n"
            "def use(conn: Borrowed[Connection]): pass\n",
            encoding="utf-8",
        )
        result = subprocess.run(
            [str(executable), "inspect-contracts", str(source)],
            cwd=temporary,
            check=True,
            capture_output=True,
            text=True,
        )
        assert '"complete-for-supported-subset"' in result.stdout
        assert '"use.conn"' in result.stdout
        marker = Path(temporary) / "executed"
        source.write_text(f"open({str(marker)!r}, 'w').close()\n", encoding="utf-8")
        result = subprocess.run(
            [str(executable), "inspect-contracts", str(source)],
            cwd=temporary,
            check=False,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1, result.stderr
        assert '"incomplete"' in result.stdout
        assert not marker.exists()

        check_root = Path(temporary) / "lifecycle"
        check_root.mkdir()
        lifecycle_source = check_root / "connection.py"
        lifecycle_marker = check_root / "executed"
        lifecycle_source.write_text(
            f"open({str(lifecycle_marker)!r}, 'w').close()\n"
            "from codepact import resource\n"
            "@resource(closed_by='close')\n"
            "class Connection:\n"
            "    def close(self): pass\n"
            "    def status(self): return 'ready'\n"
            "connection = Connection()\n"
            "connection.close()\n"
            "connection.status()\n",
            encoding="utf-8",
        )
        result = subprocess.run(
            [str(executable), "check", "--root", str(check_root), "--json"],
            cwd=temporary,
            check=False,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1, result.stderr
        checked = cast(CheckResult, json.loads(result.stdout))
        findings = [
            diagnostic
            for diagnostic in checked["diagnostics"]
            if diagnostic["code"] == "TP1001"
        ]
        assert len(findings) == 1, checked["diagnostics"]
        assert findings[0]["certainty"] == "definite"
        assert findings[0]["related"], findings[0]
        assert not lifecycle_marker.exists()
    print(f"Installed distribution and Rust CLI verified: {version}")


if __name__ == "__main__":
    main()

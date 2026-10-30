"""Exercise an installed wheel or source distribution outside the checkout."""

import importlib.metadata
from pathlib import Path
import subprocess
import sys
import sysconfig
import tempfile


def main() -> None:
    version = importlib.metadata.version("typepact-cli")
    assert importlib.metadata.metadata("typepact-cli")["Requires-Python"] == ">=3.11"
    if "--cli-only" not in sys.argv:
        import typepact

        assert importlib.metadata.version("typepact") == version
        assert f"typepact-cli=={version}" in (
            importlib.metadata.requires("typepact") or []
        )
        assert typepact.__file__ is not None
        package = Path(typepact.__file__).parent
        assert package.is_relative_to(Path(sys.prefix))
        assert (package / "py.typed").is_file()
        assert "typepact._native" not in sys.modules
    executable = Path(sysconfig.get_path("scripts")) / (
        "typepact.exe" if sys.platform == "win32" else "typepact"
    )
    assert executable.is_file(), executable
    result = subprocess.run(
        [str(executable), "--version"], check=True, capture_output=True, text=True
    )
    assert result.stdout.strip() == f"typepact {version}", result.stdout
    subprocess.run([str(executable), "--help"], check=True, capture_output=True)
    with tempfile.TemporaryDirectory() as temporary:
        source = Path(temporary) / "contracts.py"
        source.write_text(
            "from typepact import resource\n"
            "from typepact.rust import Borrowed\n"
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
    print(f"Installed distribution and Rust CLI verified: {version}")


if __name__ == "__main__":
    main()

"""Exercise an installed wheel or source distribution outside the checkout."""

import importlib.metadata
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import sysconfig
import tempfile
from dataclasses import FrozenInstanceError
from typing import TypedDict, cast


class Diagnostic(TypedDict):
    code: str
    certainty: str
    related: list[dict[str, object]]


class CheckResult(TypedDict):
    diagnostics: list[Diagnostic]


def main() -> None:
    modes = {"--helper-only", "--cli-only"}
    selected_modes = modes.intersection(sys.argv[1:])
    assert len(selected_modes) <= 1
    mode = next(iter(selected_modes), "--combined")
    if mode == "--helper-only":
        version = importlib.metadata.version("codepact")
        assert importlib.metadata.packages_distributions().get("codepact") == [
            "codepact"
        ]
        try:
            importlib.metadata.version("codepact-cli")
        except importlib.metadata.PackageNotFoundError:
            pass
        else:
            raise AssertionError(
                "helper-only installation unexpectedly includes the CLI"
            )
    elif mode == "--cli-only":
        version = importlib.metadata.version("codepact-cli")
    else:
        version = importlib.metadata.version("codepact")
        assert importlib.metadata.version("codepact-cli") == version
    if mode != "--helper-only":
        assert (
            importlib.metadata.metadata("codepact-cli")["Requires-Python"] == ">=3.11"
        )
    supports_ownership = tuple(map(int, version.split(".")[:2])) >= (0, 5)
    if mode != "--cli-only":
        import codepact
        import codepact.rust as rust_api
        from codepact.outcomes import Ok

        assert codepact.__file__ is not None
        package = Path(codepact.__file__).parent
        assert package.is_relative_to(Path(sys.prefix))
        assert (package / "py.typed").is_file()
        assert "codepact._native" not in sys.modules
        assert codepact.Ok is rust_api.Ok
        assert codepact.Result is rust_api.Result
        assert codepact.from_nullable(None) == codepact.Nothing()
        assert codepact.from_nullable(0) == codepact.Some(0)
        assert codepact.to_nullable(codepact.Some(None)) is None
        assert codepact.to_nullable(codepact.Some(0)) == 0
        immutable_value: Ok[int] = Ok(1)
        try:
            setattr(immutable_value, "value", 2)
        except FrozenInstanceError:
            pass
        else:
            raise AssertionError("outcome fields must be immutable")
        assert codepact.capture(lambda: 42, (ValueError,)) == codepact.Ok(42)
        captured = codepact.capture(
            lambda: (_ for _ in ()).throw(ValueError("invalid")), (ValueError,)
        )
        assert isinstance(captured, codepact.Err)
        assert captured.error.args == ("invalid",)
        if supports_ownership:
            resource = object()
            assert codepact.move(resource) is resource
    executable = Path(sysconfig.get_path("scripts")) / (
        "codepact.exe" if sys.platform == "win32" else "codepact"
    )
    if mode == "--helper-only":
        assert not executable.exists(), executable
    else:
        assert executable.is_file(), executable
        result = subprocess.run(
            [str(executable), "--version"], check=True, capture_output=True, text=True
        )
        assert result.stdout.strip() == f"codepact {version}", result.stdout
        subprocess.run([str(executable), "--help"], check=True, capture_output=True)
    if mode == "--helper-only":
        print(f"Helper-only distribution verified: {version}")
        return
    if mode == "--cli-only":
        assert not importlib.util.find_spec("codepact")
        print(f"Standalone CLI distribution verified: {version}")
        return
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
        if supports_ownership:
            lifecycle_source.write_text(
                f"open({str(lifecycle_marker)!r}, 'w').close()\n"
                "from codepact import move, resource\n"
                "@resource(closed_by='close')\n"
                "class Connection:\n"
                "    def close(self): pass\n"
                "def transfer():\n"
                "    source = Connection()\n"
                "    receiver = move(source)\n"
                "    source.close()\n"
                "    receiver.close()\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    str(executable),
                    "check",
                    "--root",
                    str(check_root),
                    "--experimental-ownership",
                    "--json",
                ],
                cwd=temporary,
                check=False,
                capture_output=True,
                text=True,
            )
            assert result.returncode == 1, result.stderr
            checked = cast(CheckResult, json.loads(result.stdout))
            moved = [
                finding
                for finding in checked["diagnostics"]
                if finding["code"] == "TP2002"
            ]
            assert len(moved) == 1 and moved[0]["certainty"] == "definite"
            assert moved[0]["related"]
            assert not lifecycle_marker.exists()
    print(f"Installed combined distribution and Rust CLI verified: {version}")


if __name__ == "__main__":
    main()

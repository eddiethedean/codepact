"""Run V04-02 config controls without executing project code.

Usage: python reproduce.py /absolute/path/to/codepact /path/to/results.json
The output retains each input, command, exit, stdout and stderr for review.
"""

import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile


def main() -> None:
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).resolve()
    invalid = [
        ("unknown-key=true", "unknown-key"),
        ("config-version=2", "version"),
        ("config-version='1'", "config-version"),
        ("root=1", "root"),
        ("python-version='3.14'", "3.14"),
        ("include='src/**/*.py'", "include"),
        ("exclude=[1]", "exclude"),
        ("enabled-rules='TP3001'", "enabled-rules"),
        ("enabled-rules=['TP9999']", "TP9999"),
        ("enabled-rules=['TP3001','TP3001']", "duplicate"),
        ("include=['../outside.py']", "pattern"),
        ("include=['src/../outside.py']", "pattern"),
        ("mode='unknown'", "mode"),
        ("fail-on='info'", "fail-on"),
        ("severity='warning'", "severity"),
        ("[tool.codepact.severity]\nTP3001='fatal'", "severity"),
        ("[tool.codepact.severity]\nTP9001='info'", "TP9001"),
        ("mode='strict'\nmode='advisory'", "duplicate"),
    ]
    cases = []
    for body, fragment in invalid:
        prefix = "" if body.startswith("config-version=") else "config-version=1\n"
        cases.append(
            (body, "[tool.codepact]\n" + prefix + body + "\n", [], 2, fragment)
        )
    cases.extend(
        [
            (
                "missing-version",
                "[tool.codepact]\nmode='strict'\n",
                [],
                2,
                "config-version",
            ),
            ("invalid-toml", "[tool.codepact\n", [], 2, "TOML"),
            (
                "unrelated-tool",
                "[tool.other]\nunknown='accepted'\n[tool.codepact]\nconfig-version=1\n",
                [],
                0,
                "",
            ),
            ("no-tool-table", "[tool.other]\nunknown='accepted'\n", [], 0, ""),
            (
                "conflicting-config-flags",
                "[tool.codepact]\nconfig-version=1\n",
                ["--config", "pyproject.toml", "--no-config"],
                2,
                "--config",
            ),
        ]
    )
    records = []
    with tempfile.TemporaryDirectory(
        prefix="codepact-config-qualification-"
    ) as temporary:
        root = Path(temporary)
        source = "from pathlib import Path\nPath('executed').touch()\n"
        (root / "main.py").write_text(source)
        for name, config, flags, expected_exit, fragment in cases:
            (root / "pyproject.toml").write_text(config)
            command = [
                str(binary),
                "check",
                *(["--show-settings"] if expected_exit == 0 else []),
                "--json",
                *flags,
            ]
            result = subprocess.run(
                command, cwd=root, capture_output=True, text=True, check=False
            )
            record = {
                "name": name,
                "config": config,
                "command": command,
                "exit": result.returncode,
                "expected_exit": expected_exit,
                "expected_message_fragment": fragment,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
            records.append(record)
            assert result.returncode == expected_exit, record
            report = json.loads(result.stdout)
            if expected_exit == 2:
                assert report["execution_status"] == "failed", record
                assert "diagnostics" not in report, record
                assert report["error"]["category"] == "configuration-or-execution", (
                    record
                )
                assert fragment.lower() in report["error"]["message"].lower(), record
            else:
                assert report["python_version"] == "3.11", record
            assert not (root / "executed").exists(), record
    output.write_text(
        json.dumps(
            {
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "python": platform.python_version(),
                "host": platform.platform(),
                "source_sha256": {
                    str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted(Path("crates/codepact/src").glob("*.rs"))
                },
                "runner_sha256": hashlib.sha256(
                    Path(__file__).read_bytes()
                ).hexdigest(),
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Passed {len(records)} configuration controls; retained {output}")


if __name__ == "__main__":
    main()

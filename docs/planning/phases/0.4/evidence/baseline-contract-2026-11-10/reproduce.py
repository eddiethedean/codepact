"""Retain the stale-contract baseline counterexample on old and fixed binaries.

Usage: python reproduce.py OLD_BINARY FIXED_BINARY RESULTS_JSON
"""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    provider = "from codepact import resource\n@resource(closed_by='close')\nclass Connection:\n    def close(self): pass\n    def release(self): pass\n"
    client = "from provider import Connection\ndef run():\n    conn = Connection()\n"
    records = []
    for binary_path, fixed in [(sys.argv[1], False), (sys.argv[2], True)]:
        binary = Path(binary_path).resolve(strict=True)
        with tempfile.TemporaryDirectory(
            prefix="codepact-contract-counterexample-"
        ) as temporary:
            root = Path(temporary)
            (root / "provider.py").write_text(provider)
            (root / "main.py").write_text(client)
            created = subprocess.run(
                [
                    str(binary),
                    "baseline",
                    "update",
                    "--no-config",
                    "--output",
                    "baseline.json",
                    "--reason",
                    "reviewed cleanup debt",
                    "--json",
                ],
                cwd=root,
                capture_output=True,
                text=True,
            )
            assert created.returncode == 0, created.stderr
            baseline = json.loads((root / "baseline.json").read_text())
            observations = []
            for label, source in [
                ("original", provider),
                (
                    "changed-contract",
                    provider.replace("closed_by='close'", "closed_by='release'"),
                ),
            ]:
                (root / "provider.py").write_text(source)
                command = [
                    str(binary),
                    "check",
                    "--no-config",
                    "--baseline",
                    "baseline.json",
                    "--json",
                ]
                result = subprocess.run(
                    command, cwd=root, capture_output=True, text=True
                )
                report = json.loads(result.stdout)
                finding = next(
                    d for d in report["diagnostics"] if d["code"] == "TP1002"
                )
                expected = (
                    "active"
                    if fixed and label == "changed-contract"
                    else "suppressed-baseline"
                )
                assert finding["disposition"] == expected, finding
                assert result.returncode == (1 if expected == "active" else 0)
                observations.append(
                    {
                        "label": label,
                        "provider_source": source,
                        "command": command,
                        "exit": result.returncode,
                        "report": report,
                    }
                )
            old_fp, changed_fp = [
                o["report"]["diagnostics"][0]["fingerprint"] for o in observations
            ]
            assert (old_fp != changed_fp) == fixed
            records.append(
                {
                    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "fixed": fixed,
                    "baseline": baseline,
                    "observations": observations,
                }
            )
    Path(sys.argv[3]).write_text(
        json.dumps({"client_source": client, "records": records}, indent=2) + "\n"
    )
    print(
        "Reproduced stale suppression on frozen candidate and active finding on fixed source"
    )


if __name__ == "__main__":
    main()

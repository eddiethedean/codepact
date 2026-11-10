"""Replay frozen source oracles and compare every corpus report to retained evidence.

Run from the frozen checkout using an installed exact-candidate native wheel:
python replay_artifact.py /absolute/venv/bin/codepact /absolute/results-directory
Only the two environment-specific absolute display paths are omitted from the
corpus comparison. Findings, spans, certainty, policy and snapshot IDs must agree.
"""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    record = Path(__file__).parent
    metadata = json.loads((record / "metadata.json").read_text())
    for name, expected in metadata["source_sha256"].items():
        # The original host path is display metadata; repository-relative
        # identity begins at crates/ and is checked by exact file digest.
        relative = "crates/" + name.split("/crates/", 1)[1]
        assert hashlib.sha256(Path(relative).read_bytes()).hexdigest() == expected
    environment = os.environ.copy()
    environment["CODEPACT_QUALIFICATION_BINARY"] = str(binary)
    environment["CODEPACT_QUALIFICATION_EVIDENCE"] = str(output / "cli")
    environment["CODEPACT_SYNTHETIC_RESULTS_DIR"] = str(output / "synthetic")
    commands = [
        ["cargo", "test", "-p", "codepact", "--test", "release_qualification", "--test", "synthetic_projects", "--locked", "--", "--nocapture"],
        [sys.executable, "scripts/check_schema_fixtures.py"],
        [sys.executable, str(record.parent / "config-qualification-2026-11-10/reproduce.py"), str(binary), str(output / "configuration.json")],
    ]
    for index, command in enumerate(commands):
        result = subprocess.run(command, capture_output=True, env=environment)
        (output / f"command-{index}.log").write_bytes(result.stdout + b"\nSTDERR\n" + result.stderr)
        assert result.returncode == 0, (command, result.returncode)
    digests = {}
    with tarfile.open(record / "raw-results.tar.gz") as archive:
        names = [name for name in archive.getnames() if name.startswith("synthetic/") and name.endswith(".json")]
        assert len(names) == 72
        for name in names:
            member = archive.extractfile(name)
            assert member is not None
            expected = json.load(member)
            actual = json.loads((output / name).read_text())
            for document in [expected, actual]:
                for key in ["project_root", "config_file"]:
                    del document[key]
            assert actual == expected, name
            digests[name] = hashlib.sha256(json.dumps(actual, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    result = {
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_archive_sha256": hashlib.sha256((record / "raw-results.tar.gz").read_bytes()).hexdigest(),
        "cli_records": len(list((output / "cli").glob("*.json"))),
        "corpus_reports": len(digests),
        "comparison_omits_only": ["project_root", "config_file"],
        "report_projection_sha256": hashlib.sha256(json.dumps(digests, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
        "report_sha256": digests,
        "real_world_usefulness": "unqualified",
    }
    (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "report_sha256"}, indent=2))


if __name__ == "__main__":
    main()

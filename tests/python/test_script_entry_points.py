from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"


class ScriptEntryPointTest(unittest.TestCase):
    def test_help_commands_start_in_isolated_python_outside_the_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as cwd:
            for script in (
                "consume_check_report.py",
                "check_schema_fixtures.py",
                "smoke_release_distributions.py",
            ):
                with self.subTest(script=script):
                    result = subprocess.run(
                        [sys.executable, "-I", str(SCRIPTS / script), "--help"],
                        cwd=cwd,
                        check=False,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("usage:", result.stdout)

    def test_package_check_restores_the_import_path(self) -> None:
        # This script builds packages when run as __main__. Load it under a
        # different name to exercise startup without triggering those builds.
        command = (
            "import runpy, sys; original = sys.path[:]; "
            "runpy.run_path(sys.argv[1], run_name='isolated_startup'); "
            "assert sys.path == original"
        )
        with tempfile.TemporaryDirectory() as cwd:
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    "-c",
                    command,
                    str(SCRIPTS / "check_python_package.py"),
                ],
                cwd=cwd,
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 0, result.stderr)

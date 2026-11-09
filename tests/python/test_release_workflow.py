from pathlib import Path
import re
import unittest

from scripts.verify_release_distributions import normalize_license_text


WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/release.yml"
ROOT = WORKFLOW.parents[2]
TAG_PUSH = "if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')"


def job_section(workflow: str, job_name: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(job_name)}:\n(.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        workflow,
    )
    if match is None:
        raise AssertionError(f"release workflow is missing the {job_name} job")
    return match.group(1)


class ReleaseWorkflowTest(unittest.TestCase):
    def test_cli_distribution_declares_the_repository_mit_license(self) -> None:
        project = (ROOT / "cli/pyproject.toml").read_text(encoding="utf-8")
        self.assertIn('license-files = ["LICENSE"]', project)
        self.assertEqual(
            (ROOT / "cli/LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes()
        )

    def test_license_text_comparison_accepts_windows_line_endings(self) -> None:
        self.assertEqual(
            normalize_license_text(b"MIT License\r\n\r\nCopyright\r\n"),
            normalize_license_text(b"MIT License\n\nCopyright\n"),
        )

    def test_tag_only_pypi_publishers_fail_closed_on_the_api_token_secret(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        for job_name in ("publish-cli", "publish-codepact"):
            with self.subTest(job=job_name):
                job = job_section(workflow, job_name)
                self.assertIn(TAG_PUSH, job)
                self.assertIn("PYPI_API_TOKEN: ${{ secrets.PYPI_API_TOKEN }}", job)
                self.assertIn('if [ -z "$PYPI_API_TOKEN" ]; then', job)
                self.assertIn("The PYPI_API_TOKEN secret is required", job)
                self.assertIn("pypa/gh-action-pypi-publish@release/v1", job)
                self.assertIn("user: __token__", job)
                self.assertIn("password: ${{ secrets.PYPI_API_TOKEN }}", job)

    def test_artifact_validation_smokes_wheels_and_source_archives(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        validate = job_section(workflow, "validate-artifacts")
        self.assertIn("scripts/verify_release_distributions.py", validate)
        self.assertIn("scripts/smoke_release_distributions.py", validate)

    def test_release_installs_wheels_on_every_supported_python_before_publishing(
        self,
    ) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        matrix = job_section(workflow, "smoke-python-wheels")
        self.assertIn("python-version: ['3.11', '3.12', '3.13', '3.14']", matrix)
        self.assertIn("--wheel-only", matrix)
        for publisher in ("publish-cli", "publish-codepact", "publish-crates"):
            with self.subTest(publisher=publisher):
                self.assertIn("smoke-python-wheels", job_section(workflow, publisher))


if __name__ == "__main__":
    unittest.main()

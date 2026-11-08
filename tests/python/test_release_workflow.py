from pathlib import Path
import re
import unittest


WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/release.yml"
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


if __name__ == "__main__":
    unittest.main()

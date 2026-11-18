from __future__ import annotations

import os
import sys
import tomllib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

from sphinx.application import Sphinx

DOCS = Path(__file__).resolve().parent
ROOT = DOCS.parent
sys.path.insert(0, str(ROOT / "python"))

project = "CodePact"
author = "CodePact contributors"
copyright = "2026, CodePact contributors"
project_metadata = tomllib.loads((ROOT / "pyproject.toml").read_text("utf-8"))
release = project_metadata["project"]["version"]
version = release.rsplit(".", 1)[0]
published = tomllib.loads((DOCS / "versions.toml").read_text("utf-8"))
published_version = published["published_version"]
published_source = (
    "https://github.com/eddiethedean/codepact/blob/"
    f"{published['published_revision']}/docs/README.md"
)
docs_revision = os.environ.get("READTHEDOCS_GIT_COMMIT_HASH", "")
tagged_release = os.environ.get("READTHEDOCS_VERSION_TYPE") == "tag" and os.environ.get(
    "READTHEDOCS_GIT_IDENTIFIER"
) in {release, f"v{release}"}
released_docs = release == published_version or tagged_release
archive_version, archive_source = (
    (
        release,
        f"https://github.com/eddiethedean/codepact/blob/{docs_revision}/docs/README.md",
    )
    if tagged_release and docs_revision
    else (published_version, published_source)
)

extensions = [
    "myst_parser",
    "sphinx.ext.autodoc",
    "sphinx.ext.viewcode",
]

root_doc = "index"
source_suffix = {".md": "markdown", ".rst": "restructuredtext"}
language = "en"
nitpicky = True
exclude_patterns = [
    "_build",
    "README.md",
    "planning/**",
    "releases/**",
    "Thumbs.db",
    ".DS_Store",
]

myst_enable_extensions = ["colon_fence"]
myst_heading_anchors = 3

autodoc_typehints = "description"
autodoc_member_order = "bysource"
autodoc_default_options = {"show-inheritance": True}
nitpick_ignore = [
    ("py:class", "collections.abc.Callable"),
    ("py:class", "collections.abc.Iterable"),
    ("py:class", "codepact.T"),
    ("py:class", "P"),
    ("py:class", "~P"),
    ("py:class", "codepact.R"),
    ("py:class", "codepact.outcomes.T_co"),
    ("py:class", "codepact.outcomes.E_co"),
    ("py:class", "codepact.outcomes.T"),
    ("py:class", "codepact.outcomes.E"),
    ("py:obj", "codepact.outcomes.T_co"),
    ("py:obj", "codepact.outcomes.E_co"),
    ("py:obj", "codepact.outcomes.T"),
    ("py:obj", "codepact.outcomes.E"),
    ("py:obj", "codepact.contracts.S"),
    ("py:obj", "typing.~P"),
    ("py:class", "collections.abc.Awaitable"),
]

html_theme = "sphinx_rtd_theme"
html_title = "CodePact documentation"
html_favicon = "_static/codepact-mark.png"
templates_path = ["_templates"]
html_baseurl = os.environ.get("READTHEDOCS_CANONICAL_URL", "")
html_context = {
    "display_github": True,
    "github_user": "eddiethedean",
    "github_repo": "codepact",
    "github_version": docs_revision or "main",
    "conf_py_path": "/docs/",
    "docs_version": release,
    "docs_channel": "Released" if released_docs else "Candidate",
    "docs_revision": docs_revision,
    "published_version": published_version,
    "archive_version": archive_version,
    "archive_source": archive_source,
}
html_theme_options = {
    "collapse_navigation": True,
    "navigation_depth": 3,
    "sticky_navigation": True,
}
pygments_style = "friendly"
html_show_sphinx = False
html_static_path = ["_static"]
html_extra_path = ["schemas"]
html_css_files = ["codepact.css"]


def build_example_archives(app: Sphinx, exception: Exception | None) -> None:
    """Bundle fixture inputs in a reproducible ZIP order."""
    if exception is not None or app.builder.format != "html":
        return
    patterns = {
        "pilot": ("pyproject.toml", "src/*.py", "variants/*.py"),
        "provider": (
            "pyproject.toml",
            "src/acme_storage/*.py",
            "src/acme_storage/py.codepact",
            "caller/pyproject.toml",
            "caller/src/*.py",
            "caller/variants/*.py",
        ),
        "work": ("pyproject.toml", "src/*.py"),
        "concurrency": ("*.toml", "queue/*.py", "guard/*.py", "spawn/*.py"),
    }
    for name, globs in patterns.items():
        source = DOCS / "_examples" / name
        inputs = sorted(path for pattern in globs for path in source.glob(pattern))
        output = Path(app.outdir) / "_static" / f"codepact-{name}.zip"
        with ZipFile(output, "w") as archive:
            for path in inputs:
                member = ZipInfo(path.relative_to(source).as_posix())
                member.compress_type = ZIP_DEFLATED
                member.external_attr = 0o644 << 16
                archive.writestr(member, path.read_bytes())


def setup(app: Sphinx) -> None:
    _ = app.connect("build-finished", build_example_archives)

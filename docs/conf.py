from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))

project = "CodePact"
author = "CodePact contributors"
copyright = "2026, CodePact contributors"
version = "0.5"
release = "0.5.1"

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
    ("py:class", "codepact.T"),
    ("py:class", "P"),
    ("py:class", "~P"),
    ("py:class", "codepact.R"),
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
    "github_version": os.environ.get("READTHEDOCS_GIT_COMMIT_HASH", "main"),
    "conf_py_path": "/docs/",
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

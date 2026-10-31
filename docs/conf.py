from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))

project = "TypePact"
author = "TypePact contributors"
copyright = "2026, TypePact contributors"
version = "0.2"
release = "0.2.0"

extensions = [
    "myst_parser",
    "sphinx.ext.autodoc",
    "sphinx.ext.viewcode",
]

root_doc = "index"
source_suffix = {".md": "markdown", ".rst": "restructuredtext"}
language = "en"
exclude_patterns = ["_build", "planning/**", "releases/**", "Thumbs.db", ".DS_Store"]

myst_enable_extensions = ["colon_fence"]
myst_heading_anchors = 3

autodoc_typehints = "description"
autodoc_member_order = "bysource"
autodoc_default_options = {"show-inheritance": True}
nitpick_ignore = [
    ("py:class", "collections.abc.Callable"),
    ("py:class", "typepact.T"),
]

html_theme = "sphinx_rtd_theme"
html_title = "TypePact documentation — 0.2.0 preparation"
html_theme_options = {
    "collapse_navigation": False,
    "navigation_depth": 3,
    "sticky_navigation": True,
}
html_show_sphinx = False
html_static_path = ["_static"]
html_css_files = ["typepact.css"]

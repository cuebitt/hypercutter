import os
import sys

sys.path.insert(0, os.path.abspath("../.."))

project = "hypercutter"
copyright = "2026"
author = "Cuebitt"

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
]

templates_path = ["_templates"]
exclude_patterns = []

html_theme = "sphinx_rtd_theme"

autodoc_default_options = {
    "members": True,
    "undoc-members": True,
}

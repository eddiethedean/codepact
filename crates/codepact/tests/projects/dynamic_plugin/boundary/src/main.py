import importlib
from pathlib import Path

# This source is input data. The checker must never execute this write.
Path("TARGET_WAS_EXECUTED").write_text("synthetic canary")

def load_plugin(name):
    plugin = importlib.import_module(name)
    return plugin.connect()

import os
from pathlib import Path

Path(os.environ.get("TYPEPACT_SIDE_EFFECT_MARKER", "/tmp/typepact-side-effect-marker")).touch()

import os
from pathlib import Path

Path(os.environ.get("CODEPACT_SIDE_EFFECT_MARKER", "/tmp/codepact-side-effect-marker")).touch()

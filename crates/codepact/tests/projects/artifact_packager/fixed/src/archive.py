from codepact import resource

@resource(closed_by="close")
class Archive:
    def inspect(self):
        return "manifest"

    def close(self):
        pass

from codepact import must_use

@must_use
def build_manifest() -> str:
    return "artifact-sha256"

from codepact import must_use, resource


@resource(closed_by="close")
class Archive:
    def inspect(self) -> str:
        return "manifest"

    def close(self) -> None:
        pass


@must_use
def build_manifest() -> str:
    return "artifact-sha256"

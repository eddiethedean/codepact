from codepact import resource


@resource(closed_by="close")
class Cursor:
    def read(self) -> str:
        return "cursor row"

    def close(self) -> None:
        pass

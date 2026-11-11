from codepact import resource


@resource(closed_by="close")
class Index:
    def read(self) -> None:
        pass

    def close(self) -> None:
        pass

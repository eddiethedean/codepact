from codepact import resource


@resource(closed_by="close")
class Receipt:
    def read(self) -> None:
        pass

    def close(self) -> None:
        pass

from codepact import resource


@resource(closed_by="close")
class Decoder:
    def close(self) -> None:
        pass

    def read(self) -> None:
        pass

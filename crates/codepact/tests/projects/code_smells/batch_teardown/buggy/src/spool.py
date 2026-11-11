from codepact import resource


@resource(closed_by="close")
class Spool:
    def write(self) -> None:
        pass

    def close(self) -> None:
        pass

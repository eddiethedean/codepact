from codepact import resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass

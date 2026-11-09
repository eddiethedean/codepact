from codepact import resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def escape():
    connection = Connection()
    return unknown_result(connection)

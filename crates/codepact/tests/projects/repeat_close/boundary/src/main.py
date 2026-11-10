from codepact import resource


@resource(closed_by="close")
class Connection:
    def close(self) -> None:
        pass


def teardown() -> None:
    connection = Connection()
    alias = connection
    connection.close()
    alias.close()

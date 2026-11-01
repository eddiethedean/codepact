from typepact import resource


@resource(closed_by="close")
class Connection:
    def close(self):
        pass

    def status(self):
        return "ready"


def leak() -> None:
    connection = Connection()
    return


def clean() -> None:
    connection = Connection()
    try:
        connection.status()
    finally:
        connection.close()

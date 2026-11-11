from codepact import move, resource


@resource(closed_by="close")
class Session:
    def read(self) -> str:
        return "ready"

    def close(self) -> None:
        pass


def inspect_session() -> str:
    source = Session()
    receiver = move(source)
    try:
        return source.read()
    finally:
        receiver.close()

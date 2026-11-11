from codepact import resource
from codepact.rust import Owned


@resource(closed_by="close")
class Buffer:
    def close(self) -> None:
        pass


def finish(buffer: Owned[Buffer]) -> None:
    buffer.close()

from collections.abc import Callable

from codepact import resource


@resource(closed_by="close")
class ExportSession:
    def close(self) -> None:
        pass


def export(factory: Callable[[], ExportSession]) -> None:
    session = factory()
    session.close()

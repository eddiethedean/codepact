from codepact import resource
from codepact.rust import MustUse


@resource(closed_by="finish")
class Writer:
    def write(self) -> None:
        return None

    def finish(self) -> None:
        pass


def report_checksum() -> MustUse[str]:
    return "checksum"

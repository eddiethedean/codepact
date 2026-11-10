from codepact import resource
from codepact.rust import MustUse


@resource(closed_by="finish")
class Cursor:
    def execute(self) -> int:
        return 1

    def finish(self) -> None:
        pass


def render_select() -> MustUse[str]:
    return "SELECT id FROM staging"

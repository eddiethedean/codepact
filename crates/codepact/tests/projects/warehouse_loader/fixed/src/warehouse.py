from codepact import resource

@resource(closed_by="finish")
class Cursor:
    def execute(self):
        return 1

    def finish(self):
        pass

from codepact.rust import MustUse

def render_select() -> MustUse[str]:
    return "SELECT id FROM staging"

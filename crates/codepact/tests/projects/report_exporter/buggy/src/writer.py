from codepact import resource

@resource(closed_by="finish")
class Writer:
    def write(self):
        return None

    def finish(self):
        pass

from codepact.rust import MustUse

def report_checksum() -> MustUse[str]:
    return "checksum"

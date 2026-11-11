from codepact import resource
from codepact.rust import Owned


@resource(closed_by="close")
class Upload:
    def write(self) -> None:
        pass

    def close(self) -> None:
        pass


def submit(upload: Owned[Upload]) -> None:
    upload.write()
    upload.close()

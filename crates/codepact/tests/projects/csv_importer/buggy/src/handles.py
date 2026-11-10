from codepact import resource

@resource(closed_by="close")
class CsvFile:
    def read(self):
        return "row"

    def close(self):
        pass

from codepact import must_use

@must_use
def validate_schema() -> bool:
    return True

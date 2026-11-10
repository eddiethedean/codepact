from codepact import must_use, resource


@resource(closed_by="close")
class CsvFile:
    def read(self) -> str:
        return "row"

    def close(self) -> None:
        pass


@must_use
def validate_schema() -> bool:
    return True

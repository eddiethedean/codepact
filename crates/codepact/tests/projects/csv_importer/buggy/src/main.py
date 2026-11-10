from codepact.rust import Owned
from handles import CsvFile, validate_schema


def abandon_input() -> None:
    stream = CsvFile()
    stream.read()


def preview_closed() -> str:
    stream = CsvFile()
    stream.close()
    return stream.read()


def return_closed_input() -> Owned[CsvFile]:
    stream = CsvFile()
    mirror = stream
    stream.close()
    return mirror


def ignore_validation() -> None:
    validate_schema()


def valid_preview() -> str:
    stream = CsvFile()
    preview = stream.read()
    stream.close()
    return preview

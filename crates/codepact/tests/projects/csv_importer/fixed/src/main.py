from codepact.rust import Owned
from handles import CsvFile, validate_schema


def abandon_input() -> str:
    stream = CsvFile()
    try:
        return stream.read()
    finally:
        stream.close()


def preview_closed() -> str:
    stream = CsvFile()
    preview = stream.read()
    stream.close()
    return preview


def return_closed_input() -> Owned[CsvFile]:
    stream = CsvFile()
    mirror = stream
    return mirror


def ignore_validation() -> bool:
    return validate_schema()


def valid_preview() -> str:
    stream = CsvFile()
    preview = stream.read()
    stream.close()
    return preview
